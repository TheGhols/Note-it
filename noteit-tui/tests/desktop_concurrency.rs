//! Dedicated proof with both real binaries; the isolated harness is unchanged.
mod support;
use noteit_core::{
    authority::{perform_at, WritePath},
    coordination::{WriteCoordinationPaths, WriterLease},
    state::{AppState, NoteWindowState},
    write::{self, NoteMutation, WriteError, WriteOperation},
    NoteItCore, StorePaths,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
};
use support::{cleanup_coordination, store, wait, Tui};

struct Desktop {
    child: Child,
    harness: PathBuf,
    root: PathBuf,
    paths: StorePaths,
}
impl Drop for Desktop {
    fn drop(&mut self) {
        let _ = Command::new(&self.harness)
            .arg("--root")
            .arg(&self.root)
            .arg("--stop")
            .output();
        let _ = self.child.wait();
        cleanup_coordination(&self.paths);
    }
}

#[test]
fn real_tui_and_isolated_desktop_enforce_revision_socket_and_pointwise_lease() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        assert!(
            std::env::var_os("NOTE_IT_REQUIRE_TUI_DESKTOP_TEST").is_none(),
            "Wayland required for mandatory real GUI x TUI proof"
        );
        eprintln!("SKIP real GUI x TUI: no Wayland; run with NOTE_IT_REQUIRE_TUI_DESKTOP_TEST=1 in graphical session");
        return;
    }
    let root = tempfile::Builder::new()
        .prefix("noteit-5d-")
        .tempdir()
        .unwrap();
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap());
    let (paths, id) = store(root.path(), &runtime, "R1 original");
    let core = NoteItCore::open_read_only_at(paths.clone());
    let coordination = WriteCoordinationPaths::for_store(&paths).unwrap();
    let editor = root.path().join("editor");
    fs::write(
        &editor,
        "#!/bin/sh\nexec /bin/cp -- \"$TMPDIR/edited\" \"$1\"\n",
    )
    .unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(root.path().join("edited"), "R1 local").unwrap();
    let mut tui = Tui::spawn(root.path(), |command| {
        command.env("EDITOR", &editor);
    });
    tui.wait_text("R1 original");
    tui.open_external_editor();
    tui.wait_text("Edição salva");
    assert_eq!(core.read_note(&id).unwrap().content, "R1 local");
    let lease = WriterLease::try_acquire(&coordination)
        .unwrap()
        .expect("TUI released its mutation lease while still alive");
    drop(lease);
    println!("C: real TUI mutated locally; lease released while TUI remained alive");

    let mut state = AppState::default();
    state.notes.insert(id, NoteWindowState::default());
    state.save_to_file(&paths.state_file_path()).unwrap();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let harness = repo.join("scripts/note-it-isolated");
    // Cargo builds this via workspace gates; allow an explicit existing binary.
    let binary = std::env::var_os("NOTE_IT_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("target/debug/note-it"));
    assert!(binary.is_file(), "build desktop: cargo build --bin note-it");
    let log = root.path().join("desktop.log");
    println!(
        "COMMAND: {} --root {} -- (NOTE_IT_BINARY={})",
        harness.display(),
        root.path().display(),
        binary.display()
    );
    let child = Command::new(&harness)
        // Cargo runs this integration test from noteit-tui/, whereas the
        // desktop resolves this checkout's frontend relative to repo root.
        .current_dir(&repo)
        .arg("--root")
        .arg(root.path())
        .arg("--")
        .env("NOTE_IT_BINARY", &binary)
        .env("NOTE_IT_LAYER_DIAGNOSTICS", "1")
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let desktop = Desktop {
        child,
        harness,
        root: root.path().to_path_buf(),
        paths: paths.clone(),
    };
    wait("desktop mapped", || {
        fs::read_to_string(&log)
            .unwrap_or_default()
            .contains(&format!("event=map note={id}"))
    });
    let verification = Command::new(&desktop.harness)
        .arg("--root")
        .arg(root.path())
        .arg("--verify")
        .output()
        .unwrap();
    println!(
        "COMMAND: scripts/note-it-isolated --root {} --verify\n{}{}",
        root.path().display(),
        String::from_utf8_lossy(&verification.stdout),
        String::from_utf8_lossy(&verification.stderr)
    );
    assert!(verification.status.success());
    assert!(WriterLease::try_acquire(&coordination).unwrap().is_none());
    println!("C: desktop acquired authority and mapped its window with the same TUI still alive");

    // Desktop executes the change through its real loaded WebView/receiver.
    // Readiness setup may append before the WebView loads (the unloaded
    // receiver path); continue until an actual loaded-editor commit is seen.
    // No TUI action or RevisionConflict is retried.
    wait("loaded desktop updates R2", || {
        match perform_at(
            &paths,
            &WriteOperation::MutateNote {
                selector: id.to_string(),
                expected_revision: Some(write::revision_of(&core.read_note(&id).unwrap()).unwrap()),
                mutation: NoteMutation::Append {
                    payload: "R2 desktop".into(),
                },
            },
        ) {
            Ok(result) => {
                assert_eq!(result.path, WritePath::Authority);
                assert!(result.outcome.ui_sync_warning.is_none());
                fs::read_to_string(&log)
                    .unwrap_or_default()
                    .contains("event=external-write-committed")
            }
            Err(WriteError::WriterBusy { .. }) => false,
            Err(error) => panic!("desktop setup: {error}"),
        }
    });
    let path = paths.notes_dir.join(format!("{id}.md"));
    let r2 = fs::read(&path).unwrap();
    let count_before = fs::read_to_string(&log)
        .unwrap()
        .matches("event=external-write-committed")
        .count();
    tui.output.clear();
    fs::write(root.path().join("edited"), "R3 TUI\n\n").unwrap();
    tui.input(b"e");
    tui.wait_text("Conflito de revision");
    tui.wait_text("R2 desktop");
    assert_eq!(fs::read(&path).unwrap(), r2);
    let recovered = fs::read_dir(root.path().join("state/note-it/tui-recovery"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(recovered.len(), 1);
    assert_eq!(fs::read(&recovered[0]).unwrap(), b"R3 TUI\n\n");
    assert_eq!(
        fs::read_to_string(&log)
            .unwrap()
            .matches("event=external-write-committed")
            .count(),
        count_before
    );
    println!("A: TUI sent its displayed R1; RevisionConflict; R2 bytes intact; TUI displayed R2; zero retry commits");

    // A second, explicit user action now uses the reloaded R2.
    tui.output.clear();
    tui.input(b"e");
    tui.wait_text("Edição salva");
    wait("one desktop commit", || {
        fs::read_to_string(&log)
            .unwrap()
            .matches("event=external-write-committed")
            .count()
            > count_before
    });
    assert_eq!(
        fs::read_to_string(&log)
            .unwrap()
            .matches("event=external-write-committed")
            .count(),
        count_before + 1
    );
    assert_eq!(core.read_note(&id).unwrap().content, "R3 TUI");
    assert!(WriterLease::try_acquire(&coordination).unwrap().is_none());
    assert_eq!(noteit_core::control::PROTOCOL_VERSION, 3);
    println!("B: valid TUI editor mutation committed exactly once by desktop receiver; private socket v3; desktop lease remained held");
    tui.finish();
    assert_eq!(
        fs::read_to_string(&log)
            .unwrap()
            .matches("event=external-write-committed")
            .count(),
        count_before + 1
    );
    let response = perform_at(
        &paths,
        &WriteOperation::MutateNote {
            selector: id.to_string(),
            expected_revision: Some(write::revision_of(&core.read_note(&id).unwrap()).unwrap()),
            mutation: NoteMutation::Append {
                payload: "desktop remains responsive".into(),
            },
        },
    )
    .unwrap();
    assert_eq!(response.path, WritePath::Authority);
    println!("C: desktop accepted and confirmed its next mutation after TUI; no perpetual lease or permanent GUI block");
    drop(desktop);
}
