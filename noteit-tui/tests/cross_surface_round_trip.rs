//! One store, three surfaces, in the order a person uses them.
//!
//! Every other test in this crate proves one surface at a time. Phase 5.0D.R5
//! asked for the one that proves they agree: a note the CLI creates, the TUI
//! edits in the *visual* editor and the CLI reads back — byte for byte, through
//! the same Core, with no parallel format anywhere — and then the refusal that
//! keeps them honest when two of them write at once.
//!
//! The graphical surface is not driven from here. It needs a display and a
//! private bus, which `desktop_concurrency.rs` and `scripts/test-isolation`
//! already give it; what this adds is the CLI leg and the visual round trip.

mod support;

use noteit_core::{
    authority::perform_at,
    write::{self, NoteMutation, WriteOperation},
    NoteItCore, StorePaths,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use support::{cleanup_coordination, wait, Tui};

/// The CLI binary, beside the TUI one in the same target directory.
///
/// `cargo test -p noteit-tui` does not build another package's binary, so its
/// absence is a reason to skip out loud rather than to fail: the gate that must
/// have it is `scripts/check`, which builds the workspace first.
fn cli_binary() -> Option<PathBuf> {
    let tui = PathBuf::from(env!("CARGO_BIN_EXE_noteit-tui"));
    let candidate = tui.parent()?.join("noteit");
    candidate.is_file().then_some(candidate)
}

/// Runs the CLI against `root` and nothing else.
///
/// Every XDG base is inside the throwaway root, so the command cannot resolve
/// the person's own store however it is invoked.
fn cli(binary: &Path, root: &Path, arguments: &[&str]) -> String {
    let output = Command::new(binary)
        .args(arguments)
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("HOME", root.join("home"))
        .output()
        .expect("the CLI runs");
    assert!(
        output.status.success(),
        "noteit {arguments:?} failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("the CLI writes UTF-8")
}

/// The identifier out of a `--json` reply.
///
/// Read rather than parsed with a JSON crate on purpose: this test is here to
/// prove the surfaces agree, and the less of the CLI's own machinery it borrows
/// the more of the agreement it is actually testing.
fn note_id_in(json: &str) -> String {
    let key = "\"note_id\":\"";
    let start = json.find(key).expect("the reply names the note") + key.len();
    let rest = &json[start..];
    rest[..rest.find('"').expect("a closed string")].to_owned()
}

/// Lets the application redraw and answers the cursor reports a PTY owes it.
///
/// `drain` is what sends those replies, so a key pressed without draining in
/// between can leave the application waiting for an answer that never comes.
fn settle(tui: &mut Tui) {
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(30));
        tui.drain();
    }
}

/// Leaves the editor, the reader and then the application.
///
/// Only `Esc` and `q`, one at a time with a redraw in between. Nothing else is
/// sent blind: `d` in the editor's leave-with-changes question means discard,
/// and the very same `d` one screen later, in the reader, means move the note
/// to the trash. A test that guesses which screen it is on is a test that can
/// delete the note it was checking. Each caller therefore settles its own
/// unsaved state before calling this, and arrives here with nothing to answer.
fn leave_and_quit(tui: &mut Tui) {
    for key in [b"\x1b".as_slice(), b"\x1b", b"q"] {
        if tui.child.try_wait().unwrap().is_some() {
            break;
        }
        tui.input(key);
        settle(tui);
    }
    wait("the TUI exits", || {
        tui.drain();
        tui.child.try_wait().unwrap().is_some()
    });
    assert!(tui.child.wait().unwrap().success());
    // The terminal is measured before any cleanup, so a failure here is the
    // application's and cannot be masked by the harness.
    tui.assert_restored();
    assert!(tui.cooked());
}

fn paths_for(root: &Path, runtime: &Path) -> StorePaths {
    StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        runtime.join("note-it"),
    )
}

#[test]
fn a_note_travels_from_the_cli_through_the_visual_editor_and_back() {
    let Some(binary) = cli_binary() else {
        eprintln!("SKIP cross-surface: noteit is not built; `scripts/check` builds it");
        return;
    };
    let root = tempfile::Builder::new()
        .prefix("noteit-r5-cross-")
        .tempdir()
        .unwrap();
    let runtime = root.path().join("runtime");
    let paths = paths_for(root.path(), &runtime);

    // 1-2. The CLI creates the note and gives it a tag and a property.
    let created = cli(
        &binary,
        root.path(),
        &["criar", "conteudo inicial", "--tag", "r5", "--json"],
    );
    let id = note_id_in(&created);
    cli(
        &binary,
        root.path(),
        &["propriedades", "definir", &id, "fase=5.0D.R5"],
    );

    let before = cli(&binary, root.path(), &["ler", &id]);
    assert!(
        before.contains("conteudo inicial"),
        "the CLI wrote it: {before}"
    );
    assert!(before.contains("r5"), "and tagged it: {before}");

    // 3-5. The TUI opens the same store, edits in Visual, and saves.
    let mut tui = Tui::spawn(root.path(), |command| {
        command.env("XDG_RUNTIME_DIR", &runtime);
    });
    tui.wait_text("conteudo inicial");
    tui.input(b"\r"); // into the reader
    tui.input(b"\r"); // into the editor
    tui.wait_text("Edição");
    tui.input(b"\x1bv"); // Alt+V: the visual editor
    tui.wait_text("Visual");
    tui.input(b"\x1b[F"); // End: the caret goes to the end of the block
    tui.input(" e visual".as_bytes());
    tui.input(b"\x13"); // Ctrl+S
    wait("the visual edit reaches the store", || {
        let core = NoteItCore::open_read_only_at(paths.clone());
        core.list_notes().is_ok_and(|ids| {
            ids.iter()
                .filter_map(|id| core.read_note(id).ok())
                .any(|note| note.content.contains("e visual"))
        })
    });
    leave_and_quit(&mut tui);

    // 6. The CLI reads back exactly what the visual editor wrote, and the tag
    //    and property the visual editor never touched are still there.
    let after = cli(&binary, root.path(), &["ler", &id]);
    assert!(
        after.contains("conteudo inicial e visual"),
        "the CLI sees the visual edit verbatim: {after}"
    );
    let tags = cli(&binary, root.path(), &["tags"]);
    assert!(tags.contains("r5"), "the tag survived the edit: {tags}");
    let properties = cli(&binary, root.path(), &["propriedades"]);
    assert!(
        properties.contains("fase"),
        "and so did the property: {properties}"
    );

    // Nothing anywhere is a second format: what the CLI printed is what the
    // Core holds, and the file on disk is that same Markdown.
    let stored = std::fs::read_to_string(paths.notes_dir.join(format!("{id}.md")))
        .expect("the note is one Markdown file");
    assert!(
        stored.contains("conteudo inicial e visual"),
        "the file on disk carries it too: {stored:?}"
    );

    cleanup_coordination(&paths);
}

#[test]
fn a_stale_visual_save_is_refused_and_the_other_write_survives() {
    let Some(binary) = cli_binary() else {
        eprintln!("SKIP cross-surface conflict: noteit is not built");
        return;
    };
    let root = tempfile::Builder::new()
        .prefix("noteit-r5-conflict-")
        .tempdir()
        .unwrap();
    let runtime = root.path().join("runtime");
    let paths = paths_for(root.path(), &runtime);

    let created = cli(&binary, root.path(), &["criar", "revisao A", "--json"]);
    let id: noteit_core::Uuid = note_id_in(&created).parse().expect("a uuid");

    // The TUI opens revision A and starts editing it.
    let mut tui = Tui::spawn(root.path(), |command| {
        command.env("XDG_RUNTIME_DIR", &runtime);
    });
    tui.wait_text("revisao A");
    tui.input(b"\r");
    tui.input(b"\r");
    tui.wait_text("Edição");
    tui.input(b"\x1bv");
    tui.wait_text("Visual");
    tui.input(b"\x1b[F");
    tui.input(" mais da TUI".as_bytes());

    // Another surface writes revision B while the TUI still holds A.
    let core = NoteItCore::open_read_only_at(paths.clone());
    let revision = write::revision_of(&core.read_note(&id).expect("the note")).expect("a revision");
    perform_at(
        &paths,
        &WriteOperation::MutateNote {
            selector: id.to_string(),
            expected_revision: Some(revision),
            mutation: NoteMutation::ReplaceBody {
                body: "revisao B".into(),
            },
        },
    )
    .expect("the other surface writes");

    // The TUI's save is refused, and says so rather than winning.
    tui.input(b"\x13");
    tui.wait_text("Conflito");
    // The refusal is a question, not a notice: `r` re-reads what the store
    // actually holds into the editor, which is the answer that keeps the other
    // surface's bytes and leaves nothing of the stale draft behind.
    tui.input(b"r");
    settle(&mut tui);
    leave_and_quit(&mut tui);

    let stored = core.read_note(&id).expect("the note");
    assert_eq!(
        stored.content.trim(),
        "revisao B",
        "the other surface's bytes are exactly what survived"
    );

    let read_back = cli(&binary, root.path(), &["ler", &id.to_string()]);
    assert!(
        read_back.contains("revisao B") && !read_back.contains("mais da TUI"),
        "and the CLI agrees: {read_back}"
    );

    cleanup_coordination(&paths);
}
