mod support;
use noteit_core::{write, NoteItCore};
use std::{fs, os::unix::fs::PermissionsExt};
use support::{cleanup_coordination, store, wait, Tui};

#[test]
fn real_tui_restores_exact_termios_after_editor_leaves_raw_no_echo() {
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "original");
    let editor = root.path().join("poison-editor");
    fs::write(
        &editor,
        "#!/bin/sh\nset -eu\nprintf 'edited\\n' > \"$1\"\nstty raw -echo\n",
    )
    .unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700)).unwrap();
    // Optional archived executable permits the SAME test to independently
    // exercise the published baseline, not a reimplementation of its guard.
    let binary = std::env::var_os("NOTEIT_TUI_REGRESSION_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_noteit-tui").into());
    let mut tui = Tui::spawn_program(root.path(), &binary, |cmd| {
        cmd.env("EDITOR", &editor)
            .env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("original");
    tui.input(b"\re");
    tui.wait_text("Edição salva");
    assert_eq!(
        NoteItCore::open_read_only_at(paths.clone())
            .read_note(&id)
            .unwrap()
            .content,
        "edited"
    );
    cleanup_coordination(&paths);
    // finish measures before any harness cleanup. Drop only reaps the child;
    // it never repairs termios and therefore cannot mask the application defect.
    tui.finish();
}

#[test]
fn repeated_poisoned_editors_restore_original_state_including_nonzero_exit() {
    let root = tempfile::tempdir().unwrap();
    let (paths, _) = store(root.path(), &root.path().join("runtime"), "original");
    let editor = root.path().join("poison-editor");
    fs::write(&editor, "#!/bin/sh\nset -eu\ncode=$(cat \"$TMPDIR/exit-code\")\nprintf 'cycle-%s' \"$code\" > \"$1\"\ntouch \"$TMPDIR/ready\"\nwhile [ ! -f \"$TMPDIR/release\" ]; do sleep 0.02; done\nstty raw -echo intr '^X' erase '^H' 19200 min 3 time 2\nexit \"$code\"\n").unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700)).unwrap();
    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("EDITOR", &editor)
            .env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("original");
    tui.input(b"\r");
    for code in [7, 0, 8] {
        fs::write(root.path().join("exit-code"), code.to_string()).unwrap();
        tui.output.clear();
        tui.input(b"e");
        wait("editor awaits release", || {
            root.path().join("ready").exists()
        });
        // Even the second/third editor must receive the exact original T0,
        // including speeds and control characters corrupted by its predecessor.
        tui.assert_restored();
        fs::write(root.path().join("release"), "").unwrap();
        tui.wait_text(if code == 0 {
            "Edição salva"
        } else {
            "Cópia preservada"
        });
        assert!(!tui.cooked());
        fs::remove_file(root.path().join("ready")).unwrap();
        fs::remove_file(root.path().join("release")).unwrap();
    }
    tui.finish();
    cleanup_coordination(&paths);
}

#[test]
fn real_tui_restores_controlling_tty_when_stdin_is_redirected() {
    let root = tempfile::tempdir().unwrap();
    let (paths, _) = store(root.path(), &root.path().join("runtime"), "original");
    let editor = root.path().join("poison-editor");
    fs::write(
        &editor,
        "#!/bin/sh\nprintf 'edited' > \"$1\"\nstty raw -echo < /dev/tty\n",
    )
    .unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700)).unwrap();
    // setsid makes the existing slave the controlling TTY before sh redirects
    // stdin. Both Crossterm and the guard must then select /dev/tty, not fd 0.
    let mut tui = Tui::spawn_program(root.path(), "setsid".as_ref(), |cmd| {
        cmd.args([
            "--ctty",
            "--wait",
            "/bin/sh",
            "-c",
            "exec \"$@\" < /dev/null",
            "sh",
            env!("CARGO_BIN_EXE_noteit-tui"),
        ])
        .env("EDITOR", &editor)
        .env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("original");
    tui.input(b"\re");
    tui.wait_text("Edição salva");
    tui.finish();
    cleanup_coordination(&paths);
}

#[test]
fn panic_hook_restores_before_diagnostics_after_poisoned_editor() {
    use noteit_tui::terminal::{install_panic_hook, TerminalGuard};
    use rustix::termios::tcgetattr;
    use std::{io, panic, process::Command};

    const PROBE: &str = "NOTEIT_TERMINAL_PANIC_PROBE";
    if let Ok(mode) = std::env::var(PROBE) {
        // A subprocess of this integration test exercises the production hook
        // directly. No new test switches or fault-injection APIs in production.
        let before = format!("{:?}", tcgetattr(io::stdin()).unwrap());
        let marker =
            std::path::PathBuf::from(std::env::var_os("TMPDIR").unwrap()).join("hook-state");
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let restored = tcgetattr(io::stdin())
                .map(|state| format!("{state:?}") == before)
                .unwrap_or(false);
            fs::write(&marker, restored.to_string()).unwrap();
            previous(info);
        }));
        install_panic_hook();
        let (mut guard, _terminal) = TerminalGuard::new().unwrap();
        guard.suspend().unwrap();
        assert!(Command::new("stty")
            .args(["raw", "-echo"])
            .status()
            .unwrap()
            .success());
        if mode == "after-resume" {
            guard.resume().unwrap();
        }
        panic!("post-editor panic probe");
    }

    for mode in ["before-resume", "after-resume"] {
        let root = tempfile::tempdir().unwrap();
        let mut child = Tui::spawn_program(root.path(), &std::env::current_exe().unwrap(), |cmd| {
            cmd.args([
                "--exact",
                "panic_hook_restores_before_diagnostics_after_poisoned_editor",
                "--nocapture",
            ])
            .env(PROBE, mode);
        });
        wait("panic probe exits", || {
            child.drain();
            child.child.try_wait().unwrap().is_some()
        });
        assert!(!child.child.wait().unwrap().success());
        assert_eq!(
            fs::read_to_string(root.path().join("hook-state")).unwrap(),
            "true",
            "canonical restoration must precede diagnostics, not merely Drop: {mode}"
        );
        child.wait_text("post-editor panic probe");
        child.assert_restored();
        println!("PANIC: {mode}: canonical termios verified inside previous hook, before diagnostics and Drop");
    }
}

#[test]
fn screen_output_failure_never_skips_canonical_restore() {
    use noteit_tui::terminal::TerminalGuard;
    use rustix::termios::tcgetattr;
    use std::{
        io,
        process::{Command, Stdio},
    };

    const PROBE: &str = "NOTEIT_TERMINAL_OUTPUT_FAILURE_PROBE";
    if let Ok(mode) = std::env::var(PROBE) {
        let root = std::path::PathBuf::from(std::env::var_os("TMPDIR").unwrap());
        let original = format!("{:?}", tcgetattr(io::stdin()).unwrap());
        let mut guard = if mode == "initialize" {
            None
        } else {
            Some(TerminalGuard::new().unwrap().0)
        };
        if matches!(mode.as_str(), "resume" | "drop") {
            guard.as_mut().unwrap().suspend().unwrap();
            assert!(Command::new("stty")
                .args(["raw", "-echo"])
                .status()
                .unwrap()
                .success());
        }
        fs::write(root.join("ready"), "").unwrap();
        wait("parent closes output pipe", || {
            root.join("release").exists()
        });
        match mode.as_str() {
            "initialize" => {
                let error = TerminalGuard::new().err().unwrap();
                assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
                // Failed initialization must not leave a live registration.
                let error = TerminalGuard::new().err().unwrap();
                assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
            }
            "suspend" => {
                assert!(guard.as_mut().unwrap().suspend().is_err());
                assert_eq!(format!("{:?}", tcgetattr(io::stdin()).unwrap()), original);
                assert!(guard.as_mut().unwrap().restore().is_err());
            }
            "resume" => {
                assert!(guard.as_mut().unwrap().resume().is_err());
                assert_eq!(format!("{:?}", tcgetattr(io::stdin()).unwrap()), original);
                // A failed resume must not be misclassified as Active and
                // silently turn a second resume into a successful no-op.
                assert!(guard.as_mut().unwrap().resume().is_err());
            }
            "drop" => {}
            _ => unreachable!(),
        }
        drop(guard);
        assert_eq!(format!("{:?}", tcgetattr(io::stdin()).unwrap()), original);
        fs::write(root.join("restored"), "true").unwrap();
        // The test harness itself cannot report over the deliberately broken
        // stdout. All assertions and the guard's Drop have already run.
        std::process::exit(0);
    }

    for mode in ["initialize", "suspend", "resume", "drop"] {
        let root = tempfile::tempdir().unwrap();
        let mut child = Tui::spawn_program(root.path(), &std::env::current_exe().unwrap(), |cmd| {
            cmd.args([
                "--exact",
                "screen_output_failure_never_skips_canonical_restore",
                "--nocapture",
            ])
            .env(PROBE, mode)
            .stdout(Stdio::piped());
        });
        wait("guard probe ready", || root.path().join("ready").exists());
        drop(child.child.stdout.take().unwrap());
        fs::write(root.path().join("release"), "").unwrap();
        wait("output failure probe exits", || {
            child.drain();
            child.child.try_wait().unwrap().is_some()
        });
        assert!(
            child.child.wait().unwrap().success(),
            "{mode}: {}",
            String::from_utf8_lossy(&child.output)
        );
        assert_eq!(
            fs::read_to_string(root.path().join("restored")).unwrap(),
            "true"
        );
        child.assert_restored();
        println!("OUTPUT FAILURE: {mode}: canonical restoration survived BrokenPipe");
    }
}

#[test]
fn real_tui_editor_suspends_cooked_resumes_raw_and_saves_once() {
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "original");
    let editor = root.path().join("fake editor");
    fs::write(&editor, "#!/bin/sh\nset -eu\nprintf '%s' \"$1\" > \"$TMPDIR/temporary-path\"\nprintf 'edited\\n\\n' > \"$1\"\ntouch \"$TMPDIR/ready\"\nwhile [ ! -f \"$TMPDIR/release\" ]; do sleep 0.02; done\n").unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700)).unwrap();
    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("EDITOR", &editor)
            .env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("original");
    assert!(!tui.cooked());
    tui.input(b"\re");
    wait("fake editor ready", || root.path().join("ready").exists());
    assert!(tui.cooked());
    tui.drain();
    assert!(String::from_utf8_lossy(&tui.output).contains("\x1b[?1049l"));
    fs::write(root.path().join("release"), "").unwrap();
    tui.wait_text("Edição salva");
    assert!(!tui.cooked());
    let core = NoteItCore::open_read_only_at(paths.clone());
    assert_eq!(core.read_note(&id).unwrap().content, "edited");
    let temp = fs::read_to_string(root.path().join("temporary-path")).unwrap();
    assert!(!std::path::Path::new(&temp).exists());
    let revision = write::revision_of(&core.read_note(&id).unwrap()).unwrap();
    tui.finish();
    assert_eq!(
        write::revision_of(&core.read_note(&id).unwrap()).unwrap(),
        revision
    );
    assert_eq!(
        String::from_utf8_lossy(&tui.output)
            .matches("\x1b[?1049h")
            .count(),
        2
    );
    cleanup_coordination(&paths);
}

#[test]
fn missing_editor_uses_vi_and_nonzero_exit_preserves_edited_temp() {
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "original");
    let vi = root.path().join("vi");
    fs::write(&vi, "#!/bin/sh\nprintf '%s' \"$1\" > \"$TMPDIR/temporary-path\"\nprintf 'valuable' > \"$1\"\nexit 7\n").unwrap();
    fs::set_permissions(&vi, fs::Permissions::from_mode(0o700)).unwrap();
    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env_remove("EDITOR")
            .env("PATH", root.path())
            .env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("original");
    tui.input(b"\re");
    tui.wait_text("Cópia preservada");
    assert!(!tui.cooked());
    let path = fs::read_to_string(root.path().join("temporary-path")).unwrap();
    assert_eq!(fs::read(path).unwrap(), b"valuable");
    assert_eq!(
        NoteItCore::open_read_only_at(paths.clone())
            .read_note(&id)
            .unwrap()
            .content,
        "original"
    );
    tui.finish();
    cleanup_coordination(&paths);
}

#[test]
fn sigterm_during_editor_preserves_temp_and_restores_terminal() {
    let root = tempfile::tempdir().unwrap();
    let (paths, _) = store(root.path(), &root.path().join("runtime"), "original");
    let editor = root.path().join("editor");
    fs::write(&editor, "#!/bin/sh\nprintf '%s' \"$1\" > \"$TMPDIR/temporary-path\"\nprintf 'valuable' > \"$1\"\ntouch \"$TMPDIR/ready\"\nwhile :; do sleep 0.02; done\n").unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700)).unwrap();
    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("EDITOR", &editor)
            .env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("original");
    tui.input(b"\re");
    wait("editor ready", || root.path().join("ready").exists());
    assert_eq!(
        unsafe { libc::kill(tui.child.id() as i32, libc::SIGTERM) },
        0
    );
    wait("TUI exits after signal", || {
        tui.drain();
        tui.child.try_wait().unwrap().is_some()
    });
    assert!(tui.cooked());
    let path = fs::read_to_string(root.path().join("temporary-path")).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"valuable");
    tui.wait_text(&path);
    cleanup_coordination(&paths);
}
