use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Represents an active pseudo-terminal for driving and inspecting the interactive TUI.
struct TestPty {
    controller: OwnedFd,
    device: Option<OwnedFd>,
    device_raw: RawFd,
}

impl TestPty {
    fn open((cols, rows): (u16, u16)) -> Self {
        let mut controller_fd = 0;
        let mut device_fd = 0;
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let res = unsafe {
            libc::openpty(
                &mut controller_fd,
                &mut device_fd,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        };
        assert_eq!(
            res,
            0,
            "openpty falhou: {}",
            std::io::Error::last_os_error()
        );
        Self {
            controller: unsafe { OwnedFd::from_raw_fd(controller_fd) },
            device: Some(unsafe { OwnedFd::from_raw_fd(device_fd) }),
            device_raw: device_fd,
        }
    }

    fn device_stdio(&self) -> Stdio {
        Stdio::from(
            self.device
                .as_ref()
                .expect("device ainda aberto")
                .try_clone()
                .expect("falha ao clonar descritor do dispositivo"),
        )
    }

    fn get_termios(&self) -> libc::termios {
        unsafe {
            let mut termios: libc::termios = std::mem::zeroed();
            let res = libc::tcgetattr(self.device_raw, &mut termios);
            assert_eq!(
                res,
                0,
                "tcgetattr falhou: {}",
                std::io::Error::last_os_error()
            );
            termios
        }
    }

    fn resize(&self, (cols, rows): (u16, u16)) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let res = unsafe { libc::ioctl(self.controller.as_raw_fd(), libc::TIOCSWINSZ, &size) };
        assert_eq!(
            res,
            0,
            "TIOCSWINSZ falhou: {}",
            std::io::Error::last_os_error()
        );
    }

    fn write_input(&self, bytes: &[u8]) {
        let res = unsafe {
            libc::write(
                self.controller.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
            )
        };
        assert_eq!(
            res as usize,
            bytes.len(),
            "falha ao escrever no controlador pty"
        );
    }

    /// Closes the slave device endpoint to allow EOF on subsequent reads, then drains output.
    fn read_all_output(&mut self) -> String {
        self.device = None;
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = unsafe {
                libc::read(
                    self.controller.as_raw_fd(),
                    chunk.as_mut_ptr().cast(),
                    chunk.len(),
                )
            };
            match read {
                0 | -1 => break,
                count => buffer.extend_from_slice(&chunk[..count as usize]),
            }
        }
        String::from_utf8_lossy(&buffer).into_owned()
    }
}

fn create_test_store() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("tempdir");
    let notes_dir = temp.path().join("note-it").join("notes");
    std::fs::create_dir_all(&notes_dir).expect("create notes dir");
    (temp, notes_dir)
}

fn tui_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_noteit-tui"))
}

/// Helper to wait until the child process activates raw mode on the PTY device.
fn wait_for_raw_mode(pty: &TestPty, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let termios = pty.get_termios();
        if (termios.c_lflag & libc::ICANON) == 0 && (termios.c_lflag & libc::ECHO) == 0 {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn test_normal_exit_with_q_and_termios_restoration() {
    let (temp, _notes) = create_test_store();
    let mut pty = TestPty::open((80, 24));
    let initial_termios = pty.get_termios();

    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.env("XDG_CONFIG_HOME", temp.path().join("config"));
    cmd.env("XDG_STATE_HOME", temp.path().join("state"));
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());

    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);

    assert!(
        wait_for_raw_mode(&pty, Duration::from_secs(3)),
        "O processo não entrou em modo raw dentro do tempo limite"
    );

    // Send 'q' key to quit
    pty.write_input(b"q");

    let status = child.wait().expect("child wait");
    assert!(status.success(), "Processo encerrou com falha: {status:?}");

    let final_termios = pty.get_termios();
    assert_eq!(
        final_termios.c_lflag, initial_termios.c_lflag,
        "c_lflag não foi restaurado ao estado original"
    );
    assert_eq!(
        final_termios.c_iflag, initial_termios.c_iflag,
        "c_iflag não foi restaurado ao estado original"
    );
    assert_eq!(
        final_termios.c_oflag, initial_termios.c_oflag,
        "c_oflag não foi restaurado ao estado original"
    );

    let output = pty.read_all_output();
    assert!(
        output.contains("\x1b[?1049h"),
        "A TUI deve emitir a sequência EnterAlternateScreen ao iniciar"
    );
    assert!(
        output.contains("\x1b[?1049l"),
        "A TUI deve emitir a sequência LeaveAlternateScreen ao encerrar"
    );
    assert!(
        output.contains("NOTE-IT") && output.contains("Fase 5.0B"),
        "A tela inicial deve conter o título do produto e a fase"
    );
}

#[test]
fn test_normal_exit_with_esc_and_termios_restoration() {
    let (temp, _notes) = create_test_store();
    let pty = TestPty::open((80, 24));
    let initial_termios = pty.get_termios();

    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());

    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);

    assert!(wait_for_raw_mode(&pty, Duration::from_secs(3)));

    // Send Esc key
    pty.write_input(b"\x1b");

    let status = child.wait().expect("child wait");
    assert!(status.success());

    let final_termios = pty.get_termios();
    assert_eq!(final_termios.c_lflag, initial_termios.c_lflag);
}

#[test]
fn test_ctrl_c_in_raw_mode_exits_cleanly() {
    let (temp, _notes) = create_test_store();
    let pty = TestPty::open((80, 24));
    let initial_termios = pty.get_termios();

    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());

    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);

    assert!(wait_for_raw_mode(&pty, Duration::from_secs(3)));

    // Send ASCII ETX (Ctrl+C = 0x03)
    pty.write_input(b"\x03");

    let status = child.wait().expect("child wait");
    assert!(status.success());

    let final_termios = pty.get_termios();
    assert_eq!(final_termios.c_lflag, initial_termios.c_lflag);
}

#[test]
fn test_sigterm_signal_restores_terminal() {
    let (temp, _notes) = create_test_store();
    let mut pty = TestPty::open((80, 24));
    let initial_termios = pty.get_termios();

    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());

    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);

    assert!(wait_for_raw_mode(&pty, Duration::from_secs(3)));

    // Send SIGTERM directly to the process
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }

    let status = child.wait().expect("child wait");
    assert!(
        status.success(),
        "Processo terminou com status inesperado sob SIGTERM: {status:?}"
    );

    let final_termios = pty.get_termios();
    assert_eq!(
        final_termios.c_lflag, initial_termios.c_lflag,
        "Terminal não foi restaurado ao receber SIGTERM"
    );

    let output = pty.read_all_output();
    assert!(
        output.contains("\x1b[?1049l"),
        "LeaveAlternateScreen deve ser emitido ao tratar SIGTERM"
    );
}

#[test]
fn test_controlled_panic_restores_terminal_via_panic_hook() {
    let (temp, _notes) = create_test_store();
    let mut pty = TestPty::open((80, 24));
    let initial_termios = pty.get_termios();

    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.env("NOTEIT_TUI_PANIC_FOR_TEST", "1");
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());

    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);

    let status = child.wait().expect("child wait");
    assert!(
        !status.success(),
        "Pânico controlado deveria falhar o processo"
    );

    let final_termios = pty.get_termios();
    assert_eq!(
        final_termios.c_lflag, initial_termios.c_lflag,
        "Panic hook falhou ao restaurar c_lflag do terminal"
    );

    let output = pty.read_all_output();
    assert!(
        output.contains("\x1b[?1049l"),
        "Panic hook deve emitir LeaveAlternateScreen antes de imprimir o pânico"
    );
    assert!(
        output.contains("pânico controlado para teste"),
        "A mensagem de pânico deve ser visível na saída"
    );
}

#[test]
fn test_resize_event_adapts_without_corruption() {
    let (temp, _notes) = create_test_store();
    let mut pty = TestPty::open((80, 24));
    let initial_termios = pty.get_termios();

    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());

    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);

    assert!(wait_for_raw_mode(&pty, Duration::from_secs(3)));

    // Resize to larger dimensions
    pty.resize((120, 40));
    thread::sleep(Duration::from_millis(80));

    // Resize to narrower dimensions
    pty.resize((45, 15));
    thread::sleep(Duration::from_millis(80));

    // Quit cleanly
    pty.write_input(b"q");

    let status = child.wait().expect("child wait");
    assert!(status.success());

    let final_termios = pty.get_termios();
    assert_eq!(final_termios.c_lflag, initial_termios.c_lflag);

    let output = pty.read_all_output();
    assert!(output.contains("\x1b[?1049l"));
}

#[test]
fn test_missing_store_fails_with_clear_message_before_raw_mode() {
    let temp = tempfile::tempdir().expect("tempdir");
    // We intentionally DO NOT create note-it/notes under temp.path()
    let mut pty = TestPty::open((80, 24));
    let initial_termios = pty.get_termios();

    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());

    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);

    let status = child.wait().expect("child wait");
    assert_eq!(
        status.code(),
        Some(1),
        "Deveria falhar com código 1 para store inexistente"
    );

    let final_termios = pty.get_termios();
    assert_eq!(
        final_termios.c_lflag, initial_termios.c_lflag,
        "Modo raw nunca deve ser ativado se a validação do store falhar"
    );

    let output = pty.read_all_output();
    assert!(
        output.contains("erro: store do Note-it não encontrado"),
        "A mensagem explicativa de store ausente deve ser impressa: {output}"
    );
    assert!(
        !output.contains("\x1b[?1049h"),
        "Alternate screen nunca deve ser ativado se o store estiver ausente"
    );
}

#[test]
fn test_non_tty_stdout_fails_with_clear_diagnostic() {
    let (temp, _notes) = create_test_store();
    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("spawn noteit-tui");
    let status = child.wait().expect("child wait");

    assert_eq!(status.code(), Some(1));

    let mut stderr_content = String::new();
    if let Some(mut err) = child.stderr.take() {
        err.read_to_string(&mut stderr_content)
            .expect("read stderr");
    }
    assert!(
        stderr_content.contains("erro: noteit-tui requer um terminal interativo (tty)."),
        "Mensagem clara de exigência de TTY deve ser impressa no stderr"
    );
}

#[test]
fn test_cli_flags_help_and_version() {
    // --help
    let output_help = Command::new(tui_binary())
        .arg("--help")
        .output()
        .expect("exec --help");
    assert!(output_help.status.success());
    let stdout_help = String::from_utf8_lossy(&output_help.stdout);
    assert!(stdout_help.contains("USO:") && stdout_help.contains("OPÇÕES:"));

    // --version
    let output_version = Command::new(tui_binary())
        .arg("--version")
        .output()
        .expect("exec --version");
    assert!(output_version.status.success());
    let stdout_version = String::from_utf8_lossy(&output_version.stdout);
    assert!(stdout_version.starts_with("noteit-tui "));
}
