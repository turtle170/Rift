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
use std::io::{self};

use std::sync::mpsc;
use std::time::Duration;

use crate::pet::PetIdentity;
use crate::tui::spinner::Spinner;

/// Run the analysis TUI: shows spinner + streams LLM output.
/// `status_rx` receives status strings; `output_rx` receives LLM lines.
pub fn run_output_tui(
    pet: &PetIdentity,
    status_rx: mpsc::Receiver<String>,
    output_rx: mpsc::Receiver<String>,
) -> Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut spinner = Spinner::new();
    let mut output_lines: Vec<String> = Vec::new();
    let mut status = String::from("Initializing…");
    let mut done = false;

    loop {
        // Drain status updates
        while let Ok(s) = status_rx.try_recv() {
            if s == "__DONE__" {
                done = true;
            } else {
                status = s;
            }
        }

        // Drain LLM output lines
        while let Ok(line) = output_rx.try_recv() {
            output_lines.push(line);
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

        if done && output_rx.try_recv().is_err() {
            status = format!("Done! Press q to exit.");
        }

        let frame = spinner.render();
        spinner.tick();

        terminal.draw(|f| {
            let size = f.area();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Top bar: pet name + stats
                    Constraint::Min(0),    // Main output
                    Constraint::Length(3), // Bottom bar: spinner + status
                ])
                .split(size);

            // ── Top Bar ──────────────────────────────────────────────────────
            let stats = &pet.stats;
            let top_text = Line::from(vec![
                Span::styled("🦞 ", Style::default()),
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

            // Build lines, word-wrapping long ones
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
            let bottom_text = Line::from(vec![
                Span::styled(
                    format!(" {} ", frame.symbol),
                    Style::default().fg(spinner_color),
                ),
                Span::styled(
                    status.clone(),
                    Style::default().fg(RColor::Gray),
                ),
                Span::styled(
                    "  [q] quit",
                    Style::default().fg(RColor::DarkGray),
                ),
            ]);
            let bottom_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(RColor::DarkGray));
            let bottom_para = Paragraph::new(bottom_text).block(bottom_block);
            f.render_widget(bottom_para, chunks[2]);
        })?;

        if done && output_lines.last().map(|s| s.as_str()) == Some("__DONE__") {
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
