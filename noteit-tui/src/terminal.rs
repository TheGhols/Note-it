use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rustix::termios::{tcgetattr, tcsetattr, OptionalActions, Termios};
use std::fs::File;
use std::io::{self, Stdout};
use std::os::fd::{AsFd, OwnedFd};
use std::panic;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, Weak};

// The guard owns the immutable lifetime baseline; the hook only borrows its
// ownership. Never hold this mutex during terminal I/O or the previous hook.
static PANIC_TERMINAL: Mutex<Weak<OriginalTerminal>> = Mutex::new(Weak::new());

struct OriginalTerminal {
    tty: OwnedFd,
    attributes: Termios,
}

impl OriginalTerminal {
    fn capture() -> io::Result<Self> {
        // Match Crossterm 0.29's Unix tty_fd(), in both its libc and rustix
        // backends: stdin if it is a TTY, otherwise /dev/tty (NOT stdout).
        // Keep an owned reference to that terminal, without borrowing raw FDs.
        let stdin = io::stdin();
        let tty = if rustix::termios::isatty(&stdin) {
            stdin.as_fd().try_clone_to_owned()?
        } else {
            File::options()
                .read(true)
                .write(true)
                .open("/dev/tty")?
                .into()
        };
        let attributes = tcgetattr(&tty)?;
        Ok(Self { tty, attributes })
    }

    fn restore(&self) -> io::Result<()> {
        tcsetattr(&self.tty, OptionalActions::Now, &self.attributes)?;
        Ok(())
    }
}

#[derive(PartialEq)]
enum State {
    Inactive,
    Active,
    // A transition started but has not completed successfully. Cleanup must
    // still run; neither an early return nor a failed restore means inactive.
    Uncertain,
}

/// RAII guard responsible for restoring the terminal upon normal return, error, or unwind.
pub struct TerminalGuard {
    original: Arc<OriginalTerminal>,
    state: State,
}

impl TerminalGuard {
    /// Initializes terminal into raw mode and alternate screen, hiding the cursor.
    pub fn new() -> io::Result<(Self, Terminal<CrosstermBackend<Stdout>>)> {
        let original = Arc::new(OriginalTerminal::capture()?);
        {
            let mut registered = PANIC_TERMINAL.lock().unwrap_or_else(|e| e.into_inner());
            if registered.upgrade().is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "terminal guard already exists",
                ));
            }
            *registered = Arc::downgrade(&original);
        }
        let mut guard = Self {
            original,
            state: State::Inactive,
        };
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
        if self.state == State::Active {
            return Ok(());
        }
        // Clear any incomplete Crossterm transition, then restore T0 BEFORE
        // Crossterm captures its next baseline. An editor cannot redefine T0.
        self.restore()?;
        self.state = State::Uncertain;
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, Hide, EnableMouseCapture)?;
        self.state = State::Active;
        Ok(())
    }

    /// Restore the exact initial attributes, not an assumed "cooked" default.
    pub fn restore(&mut self) -> io::Result<()> {
        // Always clean up, even when inactive: the editor can have changed the
        // terminal while suspended. State records OUR completed transitions,
        // never a claim about an external process's terminal behavior.
        self.state = State::Uncertain;
        restore_terminal(Some(&self.original))?;
        self.state = State::Inactive;
        Ok(())
    }
}

fn restore_terminal(original: Option<&OriginalTerminal>) -> io::Result<()> {
    // Evaluate every operation, retaining the first error. In particular no
    // screen/cursor/raw-mode failure may skip the final authoritative tcsetattr.
    let screen = execute!(io::stdout(), LeaveAlternateScreen);
    let cursor = execute!(io::stdout(), Show);
    let mouse = execute!(io::stdout(), DisableMouseCapture);
    let raw = disable_raw_mode();
    let canonical = original.map_or(Ok(()), OriginalTerminal::restore);
    screen.and(cursor).and(mouse).and(raw).and(canonical)
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
        let original = PANIC_TERMINAL
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .upgrade();
        let _ = restore_terminal(original.as_deref());
        previous_hook(panic_info);
    }));
}

/// Registers the termination signals that set the provided atomic flag to true.
///
/// `SIGHUP` belongs here for the same reason the other two do, and it is the
/// one a person actually meets: closing the terminal window or dropping an ssh
/// session. Its default disposition kills the process outright, which would
/// take the terminal's restoration and any unsaved draft with it. On the flag,
/// it becomes an ordinary request to stop, and the shutdown path — restore the
/// terminal, preserve what was never written — runs like it does for the rest.
pub fn install_signal_handlers(term_flag: Arc<AtomicBool>) -> io::Result<()> {
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register(signal, Arc::clone(&term_flag))?;
    }
    Ok(())
}
