use noteit_core::{
    authority::perform_at,
    write::{NoteDraft, WriteOperation},
    StorePaths,
};
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

fn create_test_store_with_note() -> TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = StorePaths::from_custom_paths(
        temp.path().join("note-it/notes"),
        temp.path().join("config/note-it"),
        temp.path().join("state/note-it"),
        temp.path().join("runtime"),
    );
    perform_at(
        &paths,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "nota pelo mouse".into(),
                ..Default::default()
            },
        },
    )
    .unwrap();
    temp
}

fn create_test_store_with_tasks() -> TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = StorePaths::from_custom_paths(
        temp.path().join("note-it/notes"),
        temp.path().join("config/note-it"),
        temp.path().join("state/note-it"),
        temp.path().join("runtime"),
    );
    perform_at(
        &paths,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "- [ ] **mouse**\n- [ ] `teclado`".into(),
                ..Default::default()
            },
        },
    )
    .unwrap();
    temp
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
        output.contains("NOTE-IT — Interface de Terminal") && !output.contains("Fase 5.0B"),
        "A tela inicial deve conter o título durável do produto"
    );
    assert!(
        output.contains("\x1b[?1000h"),
        "captura de mouse deve ser habilitada"
    );
    assert!(
        output.contains("\x1b[?1000l"),
        "captura de mouse deve ser desabilitada"
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
fn test_real_pty_mouse_click_and_dirty_navigation_use_the_prompt() {
    let temp = create_test_store_with_note();
    let mut pty = TestPty::open((80, 24));
    let initial_termios = pty.get_termios();
    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.env("XDG_CONFIG_HOME", temp.path().join("config"));
    cmd.env("XDG_STATE_HOME", temp.path().join("state"));
    cmd.env("XDG_RUNTIME_DIR", temp.path().join("runtime"));
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());
    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);
    assert!(wait_for_raw_mode(&pty, Duration::from_secs(3)));

    // SGR mouse: click first visible row, type a dirty byte, click Trash.
    pty.write_input(b"\x1b[<0;5;5M\x1b[<0;5;5m");
    pty.write_input(b"X");
    pty.write_input(b"\x1b[<0;45;2M\x1b[<0;45;2m");
    thread::sleep(Duration::from_millis(100));
    pty.write_input(b"d");
    pty.write_input(b"q");
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().expect("child wait") {
            break status;
        }
        if Instant::now() >= deadline {
            unsafe {
                libc::kill(child.id() as i32, libc::SIGTERM);
            }
            let _ = child.wait();
            let output = pty.read_all_output();
            panic!("fluxo futuro não encerrou: {output:?}");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    assert_eq!(pty.get_termios().c_lflag, initial_termios.c_lflag);
    let output = pty.read_all_output();
    assert!(
        output.contains("Edição:"),
        "clique não abriu editor: {output:?}"
    );
    assert!(
        output.contains("Alterações não salvas"),
        "clique sujo não abriu confirmação: {output:?}"
    );
    assert!(output.contains("\x1b[?1000h") && output.contains("\x1b[?1000l"));
}

#[test]
fn test_real_pty_format_palette_writes_canonical_color() {
    let temp = create_test_store_with_note();
    let mut pty = TestPty::open((80, 24));
    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.env("XDG_CONFIG_HOME", temp.path().join("config"));
    cmd.env("XDG_STATE_HOME", temp.path().join("state"));
    cmd.env("XDG_RUNTIME_DIR", temp.path().join("runtime"));
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());
    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);
    assert!(wait_for_raw_mode(&pty, Duration::from_secs(3)));
    pty.write_input(b"\x1b[<0;5;5M\x1b[<0;5;5m");
    pty.write_input(b"\x01"); // Ctrl+A
    pty.write_input(b"\x1b[<0;21;24M\x1b[<0;21;24m"); // Formatar no rodapé
    thread::sleep(Duration::from_millis(50));
    pty.write_input(b"\x1b[<0;31;11M\x1b[<0;31;11m"); // Cor do texto
    thread::sleep(Duration::from_millis(50));
    pty.write_input(b"\x1b[<0;31;9M\x1b[<0;31;9m"); // Cinza
    thread::sleep(Duration::from_millis(50));
    pty.write_input(b"\x13"); // Ctrl+S
    thread::sleep(Duration::from_millis(50));
    pty.write_input(b"\x1b");
    thread::sleep(Duration::from_millis(50));
    pty.write_input(b"q");
    assert!(child.wait().expect("child wait").success());
    let output = pty.read_all_output();
    assert!(output.contains("Formatar seleção") && output.contains("Cor do texto"));
    let notes = std::fs::read_dir(temp.path().join("note-it/notes")).unwrap();
    let stored = notes
        .filter_map(Result::ok)
        .find(|entry| entry.path().extension().and_then(|v| v.to_str()) == Some("md"))
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .unwrap();
    assert!(stored.contains(
        "<span data-note-it-color=\"#64748B\" style=\"color:#64748B\">nota pelo mouse</span>"
    ));
}

#[test]
fn test_real_pty_future_color_and_highlight_compose_without_selection() {
    let temp = create_test_store_with_note();
    let mut pty = TestPty::open((80, 24));
    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.env("XDG_CONFIG_HOME", temp.path().join("config"));
    cmd.env("XDG_STATE_HOME", temp.path().join("state"));
    cmd.env("XDG_RUNTIME_DIR", temp.path().join("runtime"));
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());
    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);
    assert!(wait_for_raw_mode(&pty, Duration::from_secs(3)));
    pty.write_input(b"\x1b[<0;5;5M\x1b[<0;5;5m");
    pty.write_input(b"\x1b[<0;21;24M\x1b[<0;21;24m");
    thread::sleep(Duration::from_millis(40));
    pty.write_input(b"\x1b[<0;31;11M\x1b[<0;31;11m");
    thread::sleep(Duration::from_millis(40));
    pty.write_input(b"\x1b[<0;31;10M\x1b[<0;31;10m"); // Vermelho
    pty.write_input(b"abc");
    pty.write_input(b"\x1b[<0;21;24M\x1b[<0;21;24m");
    thread::sleep(Duration::from_millis(40));
    pty.write_input(b"\x1b[<0;31;12M\x1b[<0;31;12m"); // Marca-texto
    thread::sleep(Duration::from_millis(40));
    pty.write_input(b"\x1b[<0;31;11M\x1b[<0;31;11m"); // Amarelo
    pty.write_input(b"def\x13");
    thread::sleep(Duration::from_millis(150));
    pty.write_input(b"\x03");
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().expect("child wait") {
            break status;
        }
        if Instant::now() >= deadline {
            unsafe {
                libc::kill(child.id() as i32, libc::SIGTERM);
            }
            let _ = child.wait();
            let output = pty.read_all_output();
            panic!("fluxo futuro não encerrou: {output:?}");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    let output = pty.read_all_output();
    assert!(output.contains("Cor: Vermelho") && output.contains("Marca: Amarelo"));
    let stored = std::fs::read_dir(temp.path().join("note-it/notes"))
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| entry.path().extension().and_then(|v| v.to_str()) == Some("md"))
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .unwrap();
    assert!(stored.contains(">abc</span>"));
    assert!(
        stored.contains("data-note-it-highlight=\"#FDE68A\"")
            && stored.contains(">def</span></mark>")
    );
}

#[test]
fn test_real_pty_task_checkbox_mouse_and_keyboard_toggle_rendered_tasks() {
    let temp = create_test_store_with_tasks();
    let mut pty = TestPty::open((80, 24));
    let mut cmd = Command::new(tui_binary());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd.env("XDG_CONFIG_HOME", temp.path().join("config"));
    cmd.env("XDG_STATE_HOME", temp.path().join("state"));
    cmd.env("XDG_RUNTIME_DIR", temp.path().join("runtime"));
    cmd.stdin(pty.device_stdio());
    cmd.stdout(pty.device_stdio());
    cmd.stderr(pty.device_stdio());
    let mut child = cmd.spawn().expect("spawn noteit-tui");
    drop(cmd);
    assert!(wait_for_raw_mode(&pty, Duration::from_secs(3)));
    pty.write_input(b"2");
    thread::sleep(Duration::from_millis(60));
    pty.write_input(b"\x1b[<0;4;5M\x1b[<0;4;5m");
    thread::sleep(Duration::from_millis(80));
    pty.write_input(b" \x03");
    assert!(child.wait().expect("child wait").success());
    let output = pty.read_all_output();
    assert!(output.contains("mouse") && output.contains("teclado"));
    assert!(!output.contains("**mouse**") && !output.contains("`teclado`"));
    let stored = std::fs::read_dir(temp.path().join("note-it/notes"))
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| entry.path().extension().and_then(|v| v.to_str()) == Some("md"))
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .unwrap();
    assert!(stored.contains("- [x] **mouse**") && stored.contains("- [x] `teclado`"));
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
