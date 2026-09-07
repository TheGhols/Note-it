use crate::ui;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct App {
    should_quit: bool,
    term_flag: Arc<AtomicBool>,
}

impl App {
    pub fn new(term_flag: Arc<AtomicBool>) -> Self {
        Self {
            should_quit: false,
            term_flag,
        }
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
        terminal.draw(ui::render)?;
        let mut needs_redraw = false;

        while !self.should_quit {
            if self.term_flag.load(Ordering::Relaxed) {
                break;
            }

            if needs_redraw {
                terminal.draw(ui::render)?;
                needs_redraw = false;
            }

            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            self.should_quit = true;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.should_quit = true;
                        }
                        _ => {}
                    },
                    Event::Resize(_cols, _rows) => {
                        terminal.autoresize()?;
                        needs_redraw = true;
                    }
                    _ => {}
                }
            }

            if self.term_flag.load(Ordering::Relaxed) {
                break;
            }
        }

        Ok(())
    }
}
