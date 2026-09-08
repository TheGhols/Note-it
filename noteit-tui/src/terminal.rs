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
        let mut guard = Self { active: false };
        guard.resume()?;
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        Ok((guard, terminal))
    }

    /// Leave the TUI through the same cleanup path used on exit.
    pub fn suspend(&mut self) -> io::Result<()> {
        self.restore()
    }

    /// Re-enter after an editor; idempotent and guarded on partial failure.
    pub fn resume(&mut self) -> io::Result<()> {
        if !self.active {
            self.active = true;
            enable_raw_mode()?;
            execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        }
        Ok(())
    }

    /// Restores terminal to its standard cooked mode and leaves alternate screen.
    pub fn restore(&mut self) -> io::Result<()> {
        if self.active {
            restore_terminal()?;
            self.active = false;
        }
        Ok(())
    }
}

fn restore_terminal() -> io::Result<()> {
    let screen = execute!(
        io::stdout(),
        LeaveAlternateScreen,
        Show,
        DisableMouseCapture
    );
    let raw = disable_raw_mode();
    screen.and(raw)
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
        let _ = restore_terminal();
        previous_hook(panic_info);
    }));
}

/// Registers SIGINT and SIGTERM handlers that set the provided atomic flag to true.
pub fn install_signal_handlers(term_flag: Arc<AtomicBool>) -> io::Result<()> {
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term_flag))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term_flag))?;
    Ok(())
}
