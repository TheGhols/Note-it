//! Real receiver test for R0, not a TUI feature or GUI x TUI test.
//! Uses the existing private-bus harness and a disposable shared store.
use noteit_core::authority::{perform_at, WritePath};
use noteit_core::coordination::WriteCoordinationPaths;
use noteit_core::state::{AppState, NoteWindowState};
use noteit_core::write::{self, NoteDraft, NoteMutation, WriteError, WriteOperation};
use noteit_core::{NoteItCore, StorageManager, StorePaths};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Desktop {
    root: tempfile::TempDir,
    child: Child,
    harness: PathBuf,
    coordination: WriteCoordinationPaths,
}

impl Drop for Desktop {
    fn drop(&mut self) {
        let _ = Command::new(&self.harness)
            .arg("--root")
            .arg(self.root.path())
            .arg("--stop")
            .output();
        let _ = self.child.wait();
        // Delete only the coordination key whose marker identifies this fixture.
        if fs::read_to_string(self.coordination.store_marker_path())
            .ok()
            .as_deref()
            .map(str::trim_end)
            == self.coordination.notes_dir().to_str()
        {
            let _ = fs::remove_dir_all(self.coordination.store_dir());
        }
    }
}

fn wait_for(description: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out: {description}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn log_contains(path: &Path, text: &str) -> bool {
    fs::read_to_string(path).unwrap_or_default().contains(text)
}

#[test]
fn isolated_desktop_discard_conflict_keeps_open_and_success_closes_canonical_state() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        assert!(
            std::env::var_os("NOTE_IT_REQUIRE_DESKTOP_TEST").is_none(),
            "Wayland required"
        );
        eprintln!(
            "SKIP R0 desktop receiver: no Wayland display; Core and receiver unit tests still run"
        );
        return;
    }
    let root = tempfile::Builder::new()
        .prefix("note-it-r0-")
        .tempdir()
        .unwrap();
    let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").expect("Wayland runtime"));
    let paths = StorePaths::from_custom_paths(
        root.path().join("data/note-it/notes"),
        root.path().join("config/note-it"),
        root.path().join("state/note-it"),
        runtime.join("note-it"),
    );
    let core = NoteItCore::from_storage(StorageManager::from_paths(paths.clone()).unwrap());
    let created = perform_at(
        &paths,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "R0 original".into(),
                ..Default::default()
            },
        },
    )
    .unwrap();
    let id = created.outcome.note_id;
    let old = write::revision_of(&core.read_note(&id).unwrap()).unwrap();
    let mut state = AppState::default();
    state.notes.insert(id, NoteWindowState::default());
    state.save_to_file(&paths.state_file_path()).unwrap();
    let harness = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/note-it-isolated");
    let log = root.path().join("desktop.log");
    let stderr = fs::File::create(&log).unwrap();
    let child = Command::new(&harness)
        .arg("--root")
        .arg(root.path())
        .arg("--")
        .env("NOTE_IT_BINARY", env!("CARGO_BIN_EXE_note-it"))
        .env("NOTE_IT_LAYER_DIAGNOSTICS", "1")
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .unwrap();
    let desktop = Desktop {
        root,
        child,
        harness,
        coordination: WriteCoordinationPaths::for_store(&paths).unwrap(),
    };
    wait_for("mapped desktop note", || {
        log_contains(&log, &format!("event=map note={id}"))
    });
    let verification = Command::new(&desktop.harness)
        .arg("--root")
        .arg(desktop.root.path())
        .arg("--verify")
        .output()
        .unwrap();
    assert!(
        verification.status.success(),
        "{}",
        String::from_utf8_lossy(&verification.stderr)
    );
    println!("scripts/note-it-isolated --root <temporary R0 store> --verify: PASS");

    // Update through the real receiver and wait for the page's acknowledgement.
    // The assertion on its diagnostic distinguishes a loaded editor from the
    // separate unloaded-window path; all setup retries are bounded.
    wait_for("loaded WebView ready for authority", || {
        let result = perform_at(
            &paths,
            &WriteOperation::MutateNote {
                selector: id.to_string(),
                expected_revision: Some(write::revision_of(&core.read_note(&id).unwrap()).unwrap()),
                mutation: NoteMutation::Append {
                    payload: "R0 updated".into(),
                },
            },
        );
        match result {
            Ok(result) => {
                assert_eq!(result.path, WritePath::Authority);
                assert!(result.outcome.ui_sync_warning.is_none());
                log_contains(&log, "event=external-write-committed")
            }
            Err(WriteError::WriterBusy { .. }) => false,
            Err(error) => panic!("setup failed: {error}"),
        }
    });
    let before = fs::read(core.storage().note_path(&id)).unwrap();
    let state_before = fs::read(paths.state_file_path()).unwrap();
    let result = perform_at(
        &paths,
        &WriteOperation::DiscardNote {
            selector: id.to_string(),
            expected_revision: old,
        },
    );
    assert!(
        matches!(result, Err(WriteError::RevisionConflict { .. })),
        "{result:?}"
    );
    assert_eq!(fs::read(core.storage().note_path(&id)).unwrap(), before);
    assert_eq!(fs::read(paths.state_file_path()).unwrap(), state_before);
    assert!(!core.storage().is_trashed(&id));
    assert!(AppState::load_from_file(&paths.state_file_path()).notes[&id].is_open);
    assert!(!log_contains(&log, &format!("event=unmap note={id}")));
    println!("stale discard via desktop: RevisionConflict; bytes and open state preserved");

    let current = write::revision_of(&core.read_note(&id).unwrap()).unwrap();
    let result = perform_at(
        &paths,
        &WriteOperation::DiscardNote {
            selector: id.to_string(),
            expected_revision: current,
        },
    )
    .unwrap();
    assert_eq!(result.path, WritePath::Authority);
    assert!(!core.storage().note_path(&id).exists());
    assert_eq!(
        fs::read(paths.trash_dir.join(format!("{id}.md"))).unwrap(),
        before
    );
    assert!(!AppState::load_from_file(&paths.state_file_path()).notes[&id].is_open);
    wait_for("window closed", || {
        log_contains(&log, &format!("event=unmap note={id}"))
    });
    assert!(log_contains(
        &log,
        &format!("event=conditional-discard note={id} loaded=true committed=true")
    ));
    println!(
        "current discard via desktop: Authority; bytes moved intact; state closed; window unmapped"
    );

    let expected = write::revision_of(&core.read_trash_note(&id).unwrap()).unwrap();
    let result = perform_at(
        &paths,
        &WriteOperation::RestoreFromTrashAtRevision {
            selector: id.to_string(),
            expected_revision: expected,
        },
    )
    .unwrap();
    assert_eq!(result.path, WritePath::Authority);
    assert_eq!(fs::read(core.storage().note_path(&id)).unwrap(), before);
    assert!(!AppState::load_from_file(&paths.state_file_path()).notes[&id].is_open);
    println!("conditional restore via desktop: Authority; bytes restored; no window reopened");
    // The harness tears down only this private session. Runtime directories are
    // keyed by store; let its existing cleanup remove that test-only key.
    drop(desktop);
}
