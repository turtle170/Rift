use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
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
use crate::tui::spinner::{ClawSnap, Spinner, RAINBOW};

// ── Message type sent from workers to the TUI ────────────────────────────────

/// All messages that worker tasks can send to the TUI.
pub enum TuiMsg {
    /// Update the status bar text.
    Status(String),
    /// Append a line to the main output view.
    Output(String),
    /// How many Claw workers are currently active.
    ClawCount(usize),
    /// All work is finished.
    Done,
}

/// Run the analysis TUI: shows spinner + claw snap + claw count + streams LLM output.
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
    let mut claw_snap = ClawSnap::new();
    let mut output_lines: Vec<String> = Vec::new();
    let mut status = String::from("Initializing…");
    let mut active_claws: usize = 0;
    let mut done = false;
    // Rainbow color index for "N Claws active" text
    let mut rainbow_idx: usize = 0;

    loop {
        // Drain all pending TUI messages (non-blocking)
        loop {
            match tui_rx.try_recv() {
                Ok(msg) => match msg {
                    TuiMsg::Status(s) => status = s,
                    TuiMsg::Output(line) => {
                        if line != "__DONE__" {
                            output_lines.push(line);
                        }
                    }
                    TuiMsg::ClawCount(n) => active_claws = n,
                    TuiMsg::Done => done = true,
                },
                Err(_) => break,
            }
        }

        // Handle keyboard events
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {}
                }
            }
        }

        if done {
            status = "Done! Press q to exit.".to_string();
        }

        let frame = spinner.render();
        spinner.tick();
        claw_snap.tick();

        // Advance rainbow color every tick
        rainbow_idx = (rainbow_idx + 1) % RAINBOW.len();
        let rainbow_color = crossterm_to_ratatui_color(RAINBOW[rainbow_idx]);

        terminal.draw(|f| {
            let size = f.area();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Top bar: pet name + stats
                    Constraint::Min(0),    // Main output
                    Constraint::Length(3), // Bottom bar: animations + status
                ])
                .split(size);

            // ── Top Bar ──────────────────────────────────────────────────────
            let stats = &pet.stats;
            let top_text = Line::from(vec![
                Span::styled(
                    pet.name(),
                    Style::default()
                        .fg(RColor::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("dbg:{} ", stats.debuggability),
                    Style::default().fg(RColor::Green),
                ),
                Span::styled(
                    format!("cur:{} ", stats.curiosity),
                    Style::default().fg(RColor::Yellow),
                ),
                Span::styled(
                    format!("chaos:{} ", stats.unpredictability),
                    Style::default().fg(RColor::Red),
                ),
                Span::styled(
                    format!("chat:{} ", stats.chattiness),
                    Style::default().fg(RColor::Magenta),
                ),
                Span::styled(
                    format!("ped:{} ", stats.pedantry),
                    Style::default().fg(RColor::Blue),
                ),
                Span::styled(
                    format!("emp:{}", stats.empathy),
                    Style::default().fg(RColor::LightGreen),
                ),
            ]);
            let top_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(RColor::DarkGray))
                .title(" Rift ");
            let top_para = Paragraph::new(top_text)
                .block(top_block)
                .alignment(Alignment::Left);
            f.render_widget(top_para, chunks[0]);

            // ── Main Output ───────────────────────────────────────────────────
            let main_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(RColor::DarkGray))
                .title(" Review ");

            let visible_height = chunks[1].height.saturating_sub(2) as usize;
            let skip = output_lines.len().saturating_sub(visible_height);
            let display_lines: Vec<Line> = output_lines[skip..]
                .iter()
                .map(|l| {
                    Line::from(Span::styled(
                        l.clone(),
                        Style::default().fg(RColor::White),
                    ))
                })
                .collect();

            let main_para = Paragraph::new(Text::from(display_lines))
                .block(main_block)
                .wrap(Wrap { trim: false });
            f.render_widget(main_para, chunks[1]);

            // ── Bottom Bar ────────────────────────────────────────────────────
            let spinner_color = crossterm_to_ratatui_color(frame.color);

            // Build bottom line spans
            let mut bottom_spans: Vec<Span> = vec![
                // Claw snap animation
                Span::styled(
                    format!("{} ", claw_snap.symbol()),
                    Style::default()
                        .fg(RColor::Rgb(255, 160, 0))
                        .add_modifier(Modifier::BOLD),
                ),
                // Main spinner
                Span::styled(
                    format!("{} ", frame.symbol),
                    Style::default().fg(spinner_color),
                ),
                // Status text
                Span::styled(
                    status.clone(),
                    Style::default().fg(RColor::Gray),
                ),
            ];

            // Show "N Claws active" in rainbow when any are running
            if active_claws > 0 {
                bottom_spans.push(Span::raw("  "));
                bottom_spans.push(Span::styled(
                    format!("{} Claws active", active_claws),
                    Style::default()
                        .fg(rainbow_color)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            bottom_spans.push(Span::styled(
                "  [q] quit",
                Style::default().fg(RColor::DarkGray),
            ));

            let bottom_text = Line::from(bottom_spans);
            let bottom_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(RColor::DarkGray));
            let bottom_para = Paragraph::new(bottom_text).block(bottom_block);
            f.render_widget(bottom_para, chunks[2]);
        })?;

        if done {
            // Give it one extra render then exit
            std::thread::sleep(Duration::from_millis(32));
            break;
        }
    }

    // Cleanup
    execute!(terminal.backend_mut(), terminal::LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

fn crossterm_to_ratatui_color(c: crossterm::style::Color) -> RColor {
    match c {
        crossterm::style::Color::Red => RColor::Red,
        crossterm::style::Color::Yellow => RColor::Yellow,
        crossterm::style::Color::Green => RColor::Green,
        crossterm::style::Color::Cyan => RColor::Cyan,
        crossterm::style::Color::Blue => RColor::Blue,
        crossterm::style::Color::White => RColor::White,
        crossterm::style::Color::Rgb { r, g, b } => RColor::Rgb(r, g, b),
        _ => RColor::White,
    }
}
