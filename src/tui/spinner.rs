use crossterm::style::Color;
use std::time::{Duration, Instant};

/// The 3-phase spinner state.
pub struct Spinner {
    phase: Phase,
    frame: usize,
    iteration: usize,   // how many times we've gone through the current phase
    last_tick: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Phase 1: spinning braille dots (runs 2 full cycles)
    Braille,
    /// Phase 2: rainbow filled circle ● (runs 2 full cycles)
    Rainbow,
    /// Phase 3: flashing ●/○ (runs indefinitely until next full loop)
    Flash,
}

/// Braille spinner frames (10 frames = 1 cycle)
const BRAILLE: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Rainbow colors cycling through the spectrum
const RAINBOW: &[Color] = &[
    Color::Red,
    Color::Rgb { r: 255, g: 127, b: 0 }, // orange
    Color::Yellow,
    Color::Green,
    Color::Cyan,
    Color::Blue,
    Color::Rgb { r: 148, g: 0, b: 211 }, // violet
];

const TICK_MS: u64 = 80;

impl Spinner {
    pub fn new() -> Self {
        Spinner {
            phase: Phase::Braille,
            frame: 0,
            iteration: 0,
            last_tick: Instant::now(),
        }
    }

    /// Returns true if it's time to redraw.
    pub fn tick(&mut self) -> bool {
        if self.last_tick.elapsed() < Duration::from_millis(TICK_MS) {
            return false;
        }
        self.last_tick = Instant::now();
        self.advance();
        true
    }

    fn advance(&mut self) {
        match self.phase {
            Phase::Braille => {
                self.frame += 1;
                if self.frame >= BRAILLE.len() {
                    self.frame = 0;
                    self.iteration += 1;
                    if self.iteration >= 2 {
                        self.phase = Phase::Rainbow;
                        self.iteration = 0;
                    }
                }
            }
            Phase::Rainbow => {
                self.frame += 1;
                if self.frame >= RAINBOW.len() {
                    self.frame = 0;
                    self.iteration += 1;
                    if self.iteration >= 2 {
                        self.phase = Phase::Flash;
                        self.iteration = 0;
                    }
                }
            }
            Phase::Flash => {
                self.frame = 1 - self.frame; // toggle between 0 and 1
                self.iteration += 1;
                // After 8 flashes, loop back to Braille
                if self.iteration >= 8 {
                    self.phase = Phase::Braille;
                    self.frame = 0;
                    self.iteration = 0;
                }
            }
        }
    }

    /// Render the current spinner frame as a styled string for crossterm.
    /// Returns (character_str, optional_color).
    pub fn render(&self) -> SpinnerFrame {
        match self.phase {
            Phase::Braille => SpinnerFrame {
                symbol: BRAILLE[self.frame].to_string(),
                color: Color::Cyan,
            },
            Phase::Rainbow => SpinnerFrame {
                symbol: "●".to_string(),
                color: RAINBOW[self.frame % RAINBOW.len()],
            },
            Phase::Flash => SpinnerFrame {
                symbol: if self.frame == 0 { "●" } else { "○" }.to_string(),
                color: Color::White,
            },
        }
    }
}

pub struct SpinnerFrame {
    pub symbol: String,
    pub color: Color,
}
