use crossterm::{
    cursor::{Hide, Show},
    event::DisableMouseCapture,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};
use std::panic;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// RAII guard responsible for restoring the terminal upon normal return, error, or unwind.
pub struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    /// Initializes terminal into raw mode and alternate screen, hiding the cursor.
    pub fn new() -> io::Result<(Self, Terminal<CrosstermBackend<Stdout>>)> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok((Self { active: true }, terminal))
    }

    /// Restores terminal to its standard cooked mode and leaves alternate screen.
    pub fn restore(&mut self) -> io::Result<()> {
        if self.active {
            self.active = false;
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen, Show, DisableMouseCapture);
            let _ = disable_raw_mode();
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Installs a panic hook that restores the terminal before formatting and printing panic information.
pub fn install_panic_hook() {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // First restore terminal state immediately so error output is readable
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, Show, DisableMouseCapture);
        let _ = disable_raw_mode();
        previous_hook(panic_info);
    }));
}

/// Registers SIGINT and SIGTERM handlers that set the provided atomic flag to true.
pub fn install_signal_handlers(term_flag: Arc<AtomicBool>) -> io::Result<()> {
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term_flag))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term_flag))?;
    Ok(())
}
