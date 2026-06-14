use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal,
};

use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color as RColor, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;

use crate::pet::PetIdentity;
use crate::tui::spinner::Spinner;

// ── Claude Code colour palette ────────────────────────────────────────────────
const CC_BG: RColor        = RColor::Rgb(15, 15, 23);     // near-black bg
const CC_BORDER: RColor    = RColor::Rgb(45, 45, 65);     // muted border
const CC_ACCENT: RColor    = RColor::Rgb(235, 130, 60);   // claude orange
const CC_BLUE: RColor      = RColor::Rgb(100, 160, 255);  // soft blue
const CC_GREEN: RColor     = RColor::Rgb(100, 210, 130);  // soft green
const CC_YELLOW: RColor    = RColor::Rgb(230, 195, 100);  // soft yellow
const CC_RED: RColor       = RColor::Rgb(230, 100, 100);  // soft red
const CC_PURPLE: RColor    = RColor::Rgb(190, 140, 230);  // soft purple
const CC_TEXT: RColor      = RColor::Rgb(215, 215, 225);  // main text
const CC_MUTED: RColor     = RColor::Rgb(100, 100, 120);  // muted text
const CC_TITLE: RColor     = RColor::Rgb(245, 245, 255);  // bright title

// ── Message type sent from workers to the TUI ────────────────────────────────
pub enum TuiMsg {
    Status(String),
    Output(String),
    ClawCount(usize),
    Done,
}

// ── Analysis TUI (read-only streaming output) ────────────────────────────────
pub fn run_output_tui(
    pet: &PetIdentity,
    mut tui_rx: tokio_mpsc::UnboundedReceiver<TuiMsg>,
) -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut spinner = Spinner::new();
    let mut output_lines: Vec<String> = Vec::new();
    let mut status = String::from("Initialising…");
    let mut active_claws: usize = 0;
    let mut done = false;
    let mut tick: u64 = 0;

    loop {
        // Drain all pending messages
        loop {
            match tui_rx.try_recv() {
                Ok(msg) => match msg {
                    TuiMsg::Status(s)    => status = s,
                    TuiMsg::Output(line) => { if line != "__DONE__" { output_lines.push(line); } }
                    TuiMsg::ClawCount(n) => active_claws = n,
                    TuiMsg::Done        => done = true,
                },
                Err(_) => break,
            }
        }

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {}
                }
            }
        }

        if done { status = "Done! Press q to exit.".to_string(); }

        let frame = spinner.render();
        spinner.tick();
        tick = tick.wrapping_add(1);

        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(3),
                ])
                .split(size);

            // ── Top Bar ──────────────────────────────────────────────────────
            let s = &pet.stats;
            let top_line = Line::from(vec![
                Span::styled("◆ ", Style::default().fg(CC_ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(pet.name(), Style::default().fg(CC_TITLE).add_modifier(Modifier::BOLD)),
                Span::styled("  ", Style::default()),
                stat_span("dbg", s.debuggability, CC_GREEN),
                stat_span("cur", s.curiosity, CC_YELLOW),
                stat_span("chaos", s.unpredictability, CC_RED),
                stat_span("chat", s.chattiness, CC_PURPLE),
                stat_span("ped", s.pedantry, CC_BLUE),
                stat_span("emp", s.empathy, CC_GREEN),
            ]);
            let top_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CC_BORDER))
                .title(Span::styled(" rift ", Style::default().fg(CC_ACCENT).add_modifier(Modifier::BOLD)));
            f.render_widget(Paragraph::new(top_line).block(top_block), chunks[0]);

            // ── Main Output ───────────────────────────────────────────────────
            let visible_h = chunks[1].height.saturating_sub(2) as usize;
            let skip = output_lines.len().saturating_sub(visible_h);
            let display_lines: Vec<Line> = output_lines[skip..]
                .iter()
                .map(|l| colorise_output_line(l))
                .collect();

            let title_str = if active_claws > 0 {
                format!(" output  {} claws active ", active_claws)
            } else {
                " output ".to_string()
            };
            let main_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CC_BORDER))
                .title(Span::styled(title_str, Style::default().fg(CC_MUTED)));
            f.render_widget(
                Paragraph::new(Text::from(display_lines))
                    .block(main_block)
                    .wrap(Wrap { trim: false }),
                chunks[1],
            );

            // ── Status Bar ────────────────────────────────────────────────────
            let spinner_sym = frame.symbol.clone();
            let status_line = if done {
                Line::from(vec![
                    Span::styled("✓ ", Style::default().fg(CC_GREEN).add_modifier(Modifier::BOLD)),
                    Span::styled(&status, Style::default().fg(CC_TEXT)),
                    Span::styled("  [q] quit", Style::default().fg(CC_MUTED)),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!("{} ", spinner_sym), Style::default().fg(CC_ACCENT)),
                    Span::styled(&status, Style::default().fg(CC_MUTED)),
                    Span::styled("  [q] quit", Style::default().fg(CC_MUTED)),
                ])
            };
            let bottom_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CC_BORDER));
            f.render_widget(Paragraph::new(status_line).block(bottom_block), chunks[2]);
        })?;
    }

    execute!(terminal.backend_mut(), terminal::LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

// ── Interactive TUI (chat mode) ───────────────────────────────────────────────
pub fn run_interactive_tui(
    pet: &PetIdentity,
    mut tui_rx: tokio_mpsc::UnboundedReceiver<TuiMsg>,
    input_tx: std::sync::mpsc::SyncSender<String>,
) -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut spinner = Spinner::new();
    let mut output_lines: Vec<(String, bool)> = Vec::new(); // (text, is_user)
    let mut status = String::from("Ready. Type a message and press Enter.");
    let mut input_buf = String::new();
    let mut thinking = false;
    let mut tick: u64 = 0;

    loop {
        loop {
            match tui_rx.try_recv() {
                Ok(msg) => match msg {
                    TuiMsg::Status(s)    => { status = s; thinking = true; }
                    TuiMsg::Output(line) => { output_lines.push((line, false)); thinking = false; }
                    TuiMsg::Done        => { thinking = false; status = "Ready.".to_string(); }
                    TuiMsg::ClawCount(_) => {}
                },
                Err(_) => break,
            }
        }

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                match code {
                    KeyCode::Esc => break,
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Enter => {
                        if !input_buf.trim().is_empty() {
                            let msg = input_buf.trim().to_string();
                            output_lines.push((format!("> {}", msg), true));
                            let _ = input_tx.try_send(msg);
                            input_buf.clear();
                            thinking = true;
                            status = "Thinking…".to_string();
                        }
                    }
                    KeyCode::Backspace => { input_buf.pop(); }
                    KeyCode::Char(c) => { input_buf.push(c); }
                    _ => {}
                }
            }
        }

        let frame = spinner.render();
        spinner.tick();
        tick = tick.wrapping_add(1);

        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(3),
                    Constraint::Length(3),
                ])
                .split(size);

            // ── Top Bar ──────────────────────────────────────────────────────
            let s = &pet.stats;
            let top_line = Line::from(vec![
                Span::styled("◆ ", Style::default().fg(CC_ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(pet.name(), Style::default().fg(CC_TITLE).add_modifier(Modifier::BOLD)),
                Span::styled("  ", Style::default()),
                stat_span("dbg", s.debuggability, CC_GREEN),
                stat_span("cur", s.curiosity, CC_YELLOW),
                stat_span("chaos", s.unpredictability, CC_RED),
                stat_span("chat", s.chattiness, CC_PURPLE),
                stat_span("ped", s.pedantry, CC_BLUE),
                stat_span("emp", s.empathy, CC_GREEN),
            ]);
            let top_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CC_BORDER))
                .title(Span::styled(" rift interactive ", Style::default().fg(CC_ACCENT).add_modifier(Modifier::BOLD)));
            f.render_widget(Paragraph::new(top_line).block(top_block), chunks[0]);

            // ── Chat History ──────────────────────────────────────────────────
            let visible_h = chunks[1].height.saturating_sub(2) as usize;
            let skip = output_lines.len().saturating_sub(visible_h);
            let display: Vec<Line> = output_lines[skip..]
                .iter()
                .map(|(line, is_user)| {
                    if *is_user {
                        Line::from(Span::styled(
                            line.clone(),
                            Style::default().fg(CC_ACCENT).add_modifier(Modifier::BOLD),
                        ))
                    } else {
                        colorise_output_line(line)
                    }
                })
                .collect();
            let chat_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CC_BORDER))
                .title(Span::styled(" conversation ", Style::default().fg(CC_MUTED)));
            f.render_widget(
                Paragraph::new(Text::from(display))
                    .block(chat_block)
                    .wrap(Wrap { trim: false }),
                chunks[1],
            );

            // ── Status Bar ────────────────────────────────────────────────────
            let status_line = if thinking {
                Line::from(vec![
                    Span::styled(format!("{} ", frame.symbol), Style::default().fg(CC_ACCENT)),
                    Span::styled(&status, Style::default().fg(CC_MUTED)),
                ])
            } else {
                Line::from(vec![
                    Span::styled("● ", Style::default().fg(CC_GREEN)),
                    Span::styled(&status, Style::default().fg(CC_MUTED)),
                    Span::styled("  [Esc] quit", Style::default().fg(CC_MUTED)),
                ])
            };
            let status_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CC_BORDER));
            f.render_widget(Paragraph::new(status_line).block(status_block), chunks[2]);

            // ── Input Box ─────────────────────────────────────────────────────
            let cursor_blink = if (tick / 30) % 2 == 0 { "█" } else { " " };
            let input_display = format!("{}{}", input_buf, cursor_blink);
            let input_line = Line::from(vec![
                Span::styled("› ", Style::default().fg(CC_ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(input_display, Style::default().fg(CC_TEXT)),
            ]);
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CC_ACCENT))
                .title(Span::styled(" message ", Style::default().fg(CC_ACCENT)));
            f.render_widget(Paragraph::new(input_line).block(input_block), chunks[3]);
        })?;
    }

    execute!(terminal.backend_mut(), terminal::LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn stat_span(label: &str, val: u8, col: RColor) -> Span<'static> {
    Span::styled(
        format!("{}:{} ", label, val),
        Style::default().fg(col),
    )
}

/// Apply basic syntax colouring to LLM output lines.
fn colorise_output_line(line: &str) -> Line<'static> {
    let line = line.to_string();
    if line.starts_with("##") || line.starts_with("# ") {
        Line::from(Span::styled(line, Style::default().fg(CC_ACCENT).add_modifier(Modifier::BOLD)))
    } else if line.starts_with("**") || line.starts_with("- **") {
        Line::from(Span::styled(line, Style::default().fg(CC_BLUE).add_modifier(Modifier::BOLD)))
    } else if line.starts_with("```") {
        Line::from(Span::styled(line, Style::default().fg(CC_MUTED)))
    } else if line.starts_with("  [") || line.contains("[EJECTED") || line.contains("[RAN GREP") {
        Line::from(Span::styled(line, Style::default().fg(CC_PURPLE).add_modifier(Modifier::ITALIC)))
    } else if line.starts_with("  ⚠") || line.starts_with("Error") {
        Line::from(Span::styled(line, Style::default().fg(CC_RED)))
    } else if line.starts_with("> ") {
        Line::from(Span::styled(line, Style::default().fg(CC_MUTED).add_modifier(Modifier::ITALIC)))
    } else {
        Line::from(Span::styled(line, Style::default().fg(CC_TEXT)))
    }
}
