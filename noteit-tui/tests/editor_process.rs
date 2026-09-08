mod support;
use noteit_core::{write, NoteItCore};
use std::{fs, os::unix::fs::PermissionsExt};
use support::{cleanup_coordination, store, wait, Tui};

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
