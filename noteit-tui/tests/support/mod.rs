use noteit_core::{
    authority::perform_at,
    coordination::WriteCoordinationPaths,
    write::{NoteDraft, WriteOperation},
    StorePaths, Uuid,
};
use std::{
    fs, io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub fn wait(description: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(Instant::now() < deadline, "timeout: {description}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn store(root: &Path, runtime: &Path, body: &str) -> (StorePaths, Uuid) {
    let paths = StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        runtime.join("note-it"),
    );
    let created = perform_at(
        &paths,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: body.into(),
                ..Default::default()
            },
        },
    )
    .unwrap();
    (paths, created.outcome.note_id)
}

pub fn cleanup_coordination(paths: &StorePaths) {
    let paths = WriteCoordinationPaths::for_store(paths).unwrap();
    if fs::read_to_string(paths.store_marker_path())
        .ok()
        .as_deref()
        .map(str::trim_end)
        == paths.notes_dir().to_str()
    {
        let _ = fs::remove_dir_all(paths.store_dir());
    }
}

pub struct Tui {
    master: OwnedFd,
    slave: OwnedFd,
    pub child: Child,
    pub output: Vec<u8>,
    cursor_queries: usize,
    original_termios: rustix::termios::Termios,
}

impl Tui {
    pub fn spawn(root: &Path, configure: impl FnOnce(&mut Command)) -> Self {
        Self::spawn_program(root, env!("CARGO_BIN_EXE_noteit-tui").as_ref(), configure)
    }

    pub fn spawn_program(
        root: &Path,
        program: &Path,
        configure: impl FnOnce(&mut Command),
    ) -> Self {
        let mut master = 0;
        let mut slave = 0;
        let size = libc::winsize {
            ws_row: 32,
            ws_col: 160,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: all output pointers are valid; descriptors become owned once.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    &size,
                )
            },
            0
        );
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        let original_termios = rustix::termios::tcgetattr(&slave).unwrap();
        assert_eq!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
            0
        );
        let mut command = Command::new(program);
        command
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_STATE_HOME", root.join("state"))
            .env("TMPDIR", root)
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("NOTEIT_TUI_PANIC_FOR_TEST")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
        configure(&mut command);
        let child = command.spawn().unwrap();
        Self {
            master,
            slave,
            child,
            output: Vec::new(),
            cursor_queries: 0,
            original_termios,
        }
    }
    pub fn input(&self, bytes: &[u8]) {
        assert_eq!(
            unsafe { libc::write(self.master.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) },
            bytes.len() as isize
        );
    }
    pub fn cooked(&self) -> bool {
        let mut state = std::mem::MaybeUninit::<libc::termios>::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(self.slave.as_raw_fd(), state.as_mut_ptr()) },
            0
        );
        let flags = unsafe { state.assume_init() }.c_lflag;
        flags & (libc::ICANON | libc::ECHO) == libc::ICANON | libc::ECHO
    }
    pub fn assert_restored(&self) {
        let after = rustix::termios::tcgetattr(&self.slave).unwrap();
        // Rustix Termios has no PartialEq. Its complete Debug representation
        // includes all flag bits, line discipline, every control code and speeds.
        let before = format!("{:?}", self.original_termios);
        let after = format!("{after:?}");
        println!("TERMIOS_BEFORE={before}\nTERMIOS_AFTER={after}");
        assert_eq!(
            after, before,
            "the application must restore the original PTY"
        );
    }
    pub fn drain(&mut self) {
        let mut bytes = [0u8; 8192];
        loop {
            let count = unsafe {
                libc::read(
                    self.master.as_raw_fd(),
                    bytes.as_mut_ptr().cast(),
                    bytes.len(),
                )
            };
            if count > 0 {
                self.output.extend_from_slice(&bytes[..count as usize]);
            } else {
                if count < 0 {
                    assert!(matches!(
                        io::Error::last_os_error().raw_os_error(),
                        Some(libc::EAGAIN | libc::EIO)
                    ));
                }
                break;
            }
        }
        // A PTY supplies termios, not a terminal emulator. Answer ANSI DSR
        // requested by Ratatui's clear() after returning from the editor.
        let queries = self
            .output
            .windows(4)
            .filter(|bytes| *bytes == b"\x1b[6n")
            .count();
        while self.cursor_queries < queries {
            self.input(b"\x1b[1;1R");
            self.cursor_queries += 1;
        }
    }
    pub fn wait_text(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.drain();
            if String::from_utf8_lossy(&self.output).contains(text) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timeout {text}: {}",
                String::from_utf8_lossy(&self.output)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    pub fn finish(&mut self) {
        self.input(b"q");
        wait("TUI exit", || {
            self.drain();
            self.child.try_wait().unwrap().is_some()
        });
        assert!(self.child.wait().unwrap().success());
        self.assert_restored();
        assert!(self.cooked());
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
