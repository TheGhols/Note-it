//! R-016 at the process boundary: real `noteit` invocations, real files.
//!
//! The Core suite proves the rule. This one proves that a client which only
//! ever sees the published interface — a command line, a JSON document, an exit
//! code — can actually use it, and that two separate processes racing over one
//! store get the same guarantee.

// The shared harness is compiled into every integration binary and each one
// uses the part it needs; this suite drives the CLI as a process and does not
// reach for the fake authority or the seeding helpers.
#[allow(dead_code)]
mod support;

use serde_json::Value;
use support::Sandbox;

/// Runs a command that must produce one JSON document, wherever the contract
/// puts it, and returns the parsed envelope with the exit code.
fn json(sandbox: &Sandbox, args: &[&str]) -> (i32, Value) {
    let (status, stdout, stderr) = sandbox.run(args);
    let text = if stdout.trim().is_empty() {
        stderr
    } else {
        // Success documents go to stdout and stderr stays empty.
        assert!(
            stderr.is_empty(),
            "a successful machine command must leave stderr empty, got: {stderr}"
        );
        stdout
    };
    let value: Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("one JSON document: {e}\n{text}"));
    (status, value)
}

fn create(sandbox: &Sandbox, body: &str) -> String {
    let (status, value) = json(sandbox, &["criar", body, "--json"]);
    assert_eq!(status, 0);
    value["data"]["write"]["note_id"]
        .as_str()
        .expect("note_id")
        .to_string()
}

fn read_revision(sandbox: &Sandbox, id: &str) -> String {
    let (status, value) = json(sandbox, &["ler", id, "--json"]);
    assert_eq!(status, 0);
    value["data"]["note"]["revision"]
        .as_str()
        .expect("a read must publish the revision it describes")
        .to_string()
}

fn body_on_disk(sandbox: &Sandbox, id: &str) -> String {
    let path = sandbox.store_paths().notes_dir.join(format!("{id}.md"));
    let raw = std::fs::read_to_string(path).expect("read the note file");
    noteit_core::model::NoteDocument::parse(&raw)
        .expect("parse")
        .content
}

// --------------------------------------------------------------------- R016-Q

#[test]
fn r016_q_a_read_publishes_a_revision_in_the_documented_form() {
    let sandbox = Sandbox::new();
    let id = create(&sandbox, "BASE");
    let revision = read_revision(&sandbox, &id);

    assert_eq!(revision.len(), 64, "sixty-four characters: {revision}");
    assert!(
        revision
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "lowercase hexadecimal only: {revision}"
    );

    // Reading twice without a change is the same version.
    assert_eq!(revision, read_revision(&sandbox, &id));
}

#[test]
fn r016_q_a_committed_conditional_write_reports_the_new_revision() {
    let sandbox = Sandbox::new();
    let id = create(&sandbox, "BASE");
    let r0 = read_revision(&sandbox, &id);

    let (status, value) = json(
        &sandbox,
        &["adicionar", &id, "ADDED", "--if-revision", &r0, "--json"],
    );
    assert_eq!(status, 0);
    assert_eq!(value["status"], "ok");
    assert_eq!(value["data"]["write"]["commit_state"], "committed");
    let r1 = value["data"]["write"]["revision"]
        .as_str()
        .expect("a committed write says where it landed");
    assert_ne!(r1, r0);
    assert_eq!(r1, read_revision(&sandbox, &id));
    assert_eq!(body_on_disk(&sandbox, &id), "BASE\nADDED");
}

#[test]
fn r016_q_a_conflict_is_a_typed_error_with_both_revisions_as_data() {
    let sandbox = Sandbox::new();
    let id = create(&sandbox, "SHARED-BASE");
    let r0 = read_revision(&sandbox, &id);

    // Somebody else writes.
    let (status, _) = json(
        &sandbox,
        &["adicionar", &id, "USER-TYPED-THIS-MEANWHILE", "--json"],
    );
    assert_eq!(status, 0);
    let r1 = read_revision(&sandbox, &id);

    let (status, stdout, stderr) = sandbox.run(&[
        "editar",
        &id,
        "SHARED-BASE\nAGENT-CONCLUSION",
        "--if-revision",
        &r0,
        "--json",
    ]);
    assert_ne!(status, 0, "a conflict is a failure exit");
    assert!(
        stdout.trim().is_empty(),
        "the error envelope belongs on stderr, stdout was: {stdout}"
    );
    let value: Value = serde_json::from_str(stderr.trim()).expect("one JSON document");

    assert_eq!(value["status"], "error");
    assert_eq!(value["command"], "edit");
    assert!(value["data"].is_null());
    assert_eq!(value["error"]["code"], "revision_conflict");
    assert_eq!(value["error"]["commit_state"], "not_committed");
    // Structured, so no agent has to read a sentence to find the new version.
    assert_eq!(value["error"]["expected_revision"], r0.as_str());
    assert_eq!(value["error"]["current_revision"], r1.as_str());

    // And the earlier write is still there.
    assert_eq!(
        body_on_disk(&sandbox, &id),
        "SHARED-BASE\nUSER-TYPED-THIS-MEANWHILE"
    );
}

#[test]
fn r016_q_a_conditional_no_op_reports_not_needed_and_the_current_revision() {
    let sandbox = Sandbox::new();
    let id = create(&sandbox, "BASE");
    json(&sandbox, &["tags", "adicionar", &id, "medicina", "--json"]);
    let r0 = read_revision(&sandbox, &id);

    let (status, value) = json(
        &sandbox,
        &[
            "tags",
            "adicionar",
            &id,
            "medicina",
            "--if-revision",
            &r0,
            "--json",
        ],
    );
    assert_eq!(status, 0);
    assert_eq!(value["status"], "ok");
    assert_eq!(value["data"]["write"]["changed"], false);
    assert_eq!(value["data"]["write"]["commit_state"], "not_needed");
    assert_eq!(value["data"]["write"]["revision"], r0.as_str());
}

// --------------------------------------------------------------------- R016-E

#[test]
fn r016_e_a_malformed_revision_is_a_usage_error_and_never_an_unconditional_write() {
    // The bypass this closes: if a token that is not a revision were treated as
    // "no precondition", a client with a corrupted value would silently get the
    // last-writer-wins behaviour it was trying to avoid.
    let sandbox = Sandbox::new();
    let id = create(&sandbox, "BASE");
    let valid = read_revision(&sandbox, &id);

    let malformed = [
        ("empty", String::new()),
        ("short", "abc".to_string()),
        ("long", "a".repeat(65)),
        ("not hex", "z".repeat(64)),
        ("uppercase", valid.to_uppercase()),
        ("path", "../../etc/passwd".to_string()),
        ("huge", "a".repeat(100_000)),
    ];

    for (name, token) in malformed {
        let (status, stdout, stderr) = sandbox.run(&[
            "editar",
            &id,
            "OVERWRITTEN",
            "--if-revision",
            &token,
            "--json",
        ]);
        assert_ne!(status, 0, "{name} must not succeed");
        let text = if stdout.trim().is_empty() {
            &stderr
        } else {
            &stdout
        };
        let value: Value = serde_json::from_str(text.trim()).expect("one JSON document");
        assert_eq!(value["status"], "error", "{name}");
        assert_eq!(value["error"]["code"], "usage_error", "{name}");
        assert_eq!(value["error"]["commit_state"], "not_committed", "{name}");
        assert_eq!(
            body_on_disk(&sandbox, &id),
            "BASE",
            "{name} must not have written anything"
        );
    }
}

// --------------------------------------------------------------------- R016-P

#[test]
fn r016_p_two_real_processes_racing_over_one_store_get_the_guarantee() {
    // Not library calls: three separate `noteit` executions against one store
    // on disk, exactly as a person and an agent would reach it.
    let sandbox = Sandbox::new();
    let id = create(&sandbox, "SHARED-BASE");

    // Process A reads and keeps the revision.
    let r0 = read_revision(&sandbox, &id);

    // Process B writes, coordinated, and commits.
    let (status, value) = json(
        &sandbox,
        &["adicionar", &id, "USER-TYPED-THIS-MEANWHILE", "--json"],
    );
    assert_eq!(status, 0);
    assert_eq!(value["data"]["write"]["commit_state"], "committed");

    // Process A writes what it built from the base it read.
    let (status, stdout, stderr) = sandbox.run(&[
        "editar",
        &id,
        "SHARED-BASE\nAGENT-CONCLUSION",
        "--if-revision",
        &r0,
        "--json",
    ]);
    assert_ne!(status, 0);
    let text = if stdout.trim().is_empty() {
        &stderr
    } else {
        &stdout
    };
    let value: Value = serde_json::from_str(text.trim()).expect("one JSON document");
    assert_eq!(value["error"]["code"], "revision_conflict");

    // The physical file is the evidence.
    assert_eq!(
        body_on_disk(&sandbox, &id),
        "SHARED-BASE\nUSER-TYPED-THIS-MEANWHILE",
        "the other process's committed write survived"
    );

    // Process A re-reads, reconciles, and writes consciously.
    let r1 = read_revision(&sandbox, &id);
    let (status, value) = json(
        &sandbox,
        &[
            "editar",
            &id,
            "SHARED-BASE\nUSER-TYPED-THIS-MEANWHILE\nAGENT-CONCLUSION",
            "--if-revision",
            &r1,
            "--json",
        ],
    );
    assert_eq!(status, 0);
    assert_eq!(value["data"]["write"]["commit_state"], "committed");
    assert_eq!(
        body_on_disk(&sandbox, &id),
        "SHARED-BASE\nUSER-TYPED-THIS-MEANWHILE\nAGENT-CONCLUSION"
    );
}

// ------------------------------------------------------------ human interface

#[test]
fn the_human_commands_still_work_with_no_precondition_at_all() {
    // Backward compatibility, stated as a test: every mutation keeps its old
    // spelling and its old last-writer-wins meaning when nobody asks otherwise.
    let sandbox = Sandbox::new();
    let id = create(&sandbox, "BASE");

    for args in [
        vec!["adicionar", &id, "MAIS"],
        vec!["editar", &id, "OUTRO CORPO"],
        vec!["tags", "adicionar", &id, "medicina"],
        vec!["tags", "remover", &id, "medicina"],
        vec!["propriedades", "definir", &id, "estado=ativo"],
        vec!["propriedades", "remover", &id, "estado"],
    ] {
        let (status, _stdout, stderr) = sandbox.run(&args);
        assert_eq!(status, 0, "{args:?} failed: {stderr}");
    }
}

#[test]
fn a_human_conflict_is_a_plain_sentence_with_no_escape_sequences() {
    let sandbox = Sandbox::new();
    let id = create(&sandbox, "BASE");
    let r0 = read_revision(&sandbox, &id);
    sandbox.run(&["adicionar", &id, "OUTRO"]);

    let (status, stdout, stderr) = sandbox.run(&["editar", &id, "NOVO", "--if-revision", &r0]);
    assert_ne!(status, 0);
    assert!(stdout.is_empty(), "the failure belongs on stderr");
    assert!(
        stderr.contains("mudou desde a leitura"),
        "the person is told what happened: {stderr}"
    );
    assert!(
        stderr.contains("leia a nota de novo"),
        "and what to do about it: {stderr}"
    );
    assert!(
        !stderr.contains('\u{1b}'),
        "no terminal escape may reach the message"
    );
}
