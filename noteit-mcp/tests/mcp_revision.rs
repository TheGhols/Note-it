//! MCP-10 … MCP-20, MCP-24: R-016 through the MCP boundary.
//!
//! The Core suite proves the rule and the CLI suite proves a person can use
//! it. This one proves the thing the whole phase exists for: that an *agent*,
//! seeing nothing but tool schemas and structured results, cannot write over a
//! note it has not read.
//!
//! Every assertion about a refusal checks the bytes on disk as well as the
//! answer. "It said no" and "it changed nothing" are two different claims and
//! only the second one is the guarantee.

mod support;

use serde_json::json;
use support::{create_note, read_revision, AuthorityBehaviour, FakeAuthority, McpClient, Sandbox};

/// The eight mutation tools, each with the arguments that make it do something
/// to a note that has a task, a tag and a property in it.
///
/// One list, used by every matrix test below, so a tool added to the server
/// without being added here is caught by the catalogue test rather than
/// quietly skipping the guarantee.
fn mutation_calls(note_id: &str, task_ref: &str) -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "noteit_append",
            json!({ "note_id": note_id, "text": "ACRESCENTADO" }),
        ),
        (
            "noteit_edit",
            json!({ "note_id": note_id, "body": "SUBSTITUÍDO" }),
        ),
        ("noteit_edit", json!({ "note_id": note_id, "clear": true })),
        (
            "noteit_tag_add",
            json!({ "note_id": note_id, "tag": "Nova" }),
        ),
        (
            "noteit_tag_remove",
            json!({ "note_id": note_id, "tag": "Medicina" }),
        ),
        (
            "noteit_property_set",
            json!({ "note_id": note_id, "key": "fonte", "value": "outra" }),
        ),
        (
            "noteit_property_remove",
            json!({ "note_id": note_id, "key": "fonte" }),
        ),
        (
            "noteit_task_complete",
            json!({ "note_id": note_id, "task_ref": task_ref }),
        ),
        (
            "noteit_task_reopen",
            json!({ "note_id": note_id, "task_ref": task_ref }),
        ),
    ]
}

/// A note carrying everything the mutations need: a body, a tag, a property
/// and one task, plus the reference that names the task.
fn seeded_note(client: &mut McpClient) -> (String, String) {
    let answer = client.call(
        "noteit_create",
        json!({
            "content": "BASE\n\n- [ ] revisar noradrenalina",
            "tags": ["Medicina"],
            "properties": [{ "key": "fonte", "value": "Harrison" }],
        }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    let note_id = answer.note_id();

    let tasks = client.call("noteit_tasks_list", json!({ "state": "all" }));
    let task_ref = tasks.structured()["tasks"][0]["task_ref"]
        .as_str()
        .expect("the seeded note must have a task")
        .to_string();
    (note_id, task_ref)
}

// ------------------------------------------------------------------ MCP-10

#[test]
fn mcp_10_a_write_with_the_current_revision_commits_and_answers_the_new_one() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let r0 = read_revision(&mut client, &id);

    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "ACRESCENTADO", "expected_revision": &r0 }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert!(!answer.is_error());
    assert_eq!(answer.commit_state(), Some("committed"));
    assert_eq!(answer.structured()["changed"], true);

    let r1 = answer.revision();
    assert_ne!(r1, r0, "a committed write must move the revision");
    assert_eq!(r1, read_revision(&mut client, &id));
    assert_eq!(sandbox.body(&id), "BASE\nACRESCENTADO");

    // And the revision it handed back chains straight into the next write.
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "OUTRA VEZ", "expected_revision": &r1 }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(sandbox.body(&id), "BASE\nACRESCENTADO\nOUTRA VEZ");
}

/// A conditional write that changes nothing is a success, not a conflict.
#[test]
fn mcp_10_a_no_op_is_a_success_that_changed_nothing() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let (id, _) = seeded_note(&mut client);
    let revision = read_revision(&mut client, &id);
    let before = sandbox.note_bytes(&id);

    let answer = client.call(
        "noteit_tag_add",
        json!({ "note_id": &id, "tag": "Medicina", "expected_revision": &revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert!(!answer.is_error());
    assert_eq!(answer.commit_state(), Some("not_needed"));
    assert_eq!(answer.structured()["changed"], false);
    assert_eq!(
        answer.revision(),
        revision,
        "a no-op answers the version the note already had"
    );
    assert_eq!(before, sandbox.note_bytes(&id), "a no-op rewrote the file");
}

// ------------------------------------------------------------------ MCP-11
//
// The missing-revision case lives in `mcp_surface.rs`, where the schema that
// refuses it is also checked. What is proved here is the other half: that no
// spelling of "absent" gets through as an unconditional write.

#[test]
fn mcp_11_no_spelling_of_an_absent_revision_becomes_an_unconditional_write() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let before = sandbox.note_bytes(&id);

    for absent in [json!(null), json!(""), json!(false), json!(0)] {
        let answer = client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": "NÃO", "expected_revision": absent }),
        );
        assert!(
            answer.is_error(),
            "`{absent}` was accepted as a precondition: {}",
            answer.raw
        );
        assert_eq!(
            before,
            sandbox.note_bytes(&id),
            "`{absent}` reached the store"
        );
    }
}

// ------------------------------------------------------------------ MCP-12

#[test]
fn mcp_12_a_malformed_revision_is_refused_and_changes_nothing() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let before = sandbox.note_bytes(&id);

    let malformed = [
        ("too short", "abc123"),
        ("too long", &"a".repeat(65)),
        ("uppercase", &"A".repeat(64)),
        ("not hexadecimal", &"z".repeat(64)),
        ("a path", "../../etc/passwd"),
        ("empty", ""),
        ("hex with spaces", &format!("{} ", "a".repeat(63))),
    ];

    for (why, revision) in malformed {
        let answer = client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": "NÃO", "expected_revision": revision }),
        );
        assert!(answer.is_error(), "{why} was accepted: {}", answer.raw);
        assert_eq!(answer.status(), "error", "{why}: {}", answer.raw);
        assert_eq!(
            answer.code(),
            Some("invalid_input"),
            "{why} was not an invalid_input: {}",
            answer.raw
        );
        assert_eq!(
            answer.commit_state(),
            Some("not_committed"),
            "{why} left the commit state ambiguous: {}",
            answer.raw
        );
        assert_eq!(before, sandbox.note_bytes(&id), "{why} changed the file");
    }
}

// ------------------------------------------------------------------ MCP-13

#[test]
fn mcp_13_a_stale_revision_conflicts_and_the_file_is_byte_identical() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE COMPARTILHADA");
    let stale = read_revision(&mut client, &id);

    // Somebody else writes. From the agent's side this is simply "the note
    // moved on"; it does not matter who did it.
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "A PESSOA ESCREVEU ISTO", "expected_revision": &stale }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    let current = answer.revision();
    let before = sandbox.note_bytes(&id);

    let answer = client.call(
        "noteit_edit",
        json!({ "note_id": &id, "body": "O AGENTE DECIDIU ISTO", "expected_revision": &stale }),
    );

    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(answer.status(), "error");
    assert_eq!(answer.code(), Some("revision_conflict"));
    assert_eq!(answer.commit_state(), Some("not_committed"));
    assert_eq!(answer.str_field("expected_revision"), Some(stale.as_str()));
    assert_eq!(answer.str_field("current_revision"), Some(current.as_str()));
    assert_eq!(answer.str_field("note_id"), Some(id.as_str()));

    // A conflict must not hand back a token the client could chain from: the
    // rule is "read again", and a `revision` field here would make it "retry".
    assert!(
        answer.structured().get("revision").is_none(),
        "a conflict published a usable precondition: {}",
        answer.raw
    );
    // Nor the content, which a client that has not looked at it must not have.
    let text = serde_json::to_string(&answer.raw).unwrap();
    assert!(
        !text.contains("A PESSOA ESCREVEU ISTO"),
        "a conflict handed back the content the client has not read: {text}"
    );

    assert_eq!(
        before,
        sandbox.note_bytes(&id),
        "a conflict changed the file"
    );
    // Nothing left behind either: no new backup and no surviving temporary.
    let notes_dir = sandbox.store_paths().notes_dir.clone();
    let leftovers: Vec<_> = std::fs::read_dir(&notes_dir)
        .expect("read the notes directory")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| !name.ends_with(".md"))
        .collect();
    assert!(leftovers.is_empty(), "a conflict left {leftovers:?} behind");
}

/// Every mutation, one at a time, refuses a stale revision and changes nothing.
///
/// This is the matrix. Its companion in `mcp_mutation_matrix.rs` is the one
/// that makes it a compile error to add a `NoteMutation` variant and forget it.
#[test]
fn mcp_13_every_mutation_refuses_a_stale_revision() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let (id, task_ref) = seeded_note(&mut client);
    let stale = read_revision(&mut client, &id);

    // Move the note on, so `stale` names a version that no longer exists.
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "ALGUÉM MAIS", "expected_revision": &stale }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    let before = sandbox.note_bytes(&id);
    let updated_at_before = client.call("noteit_read", json!({ "note_id": &id }));
    let updated_at_before = updated_at_before.structured()["note"]["updated_at"].clone();

    for (name, mut arguments) in mutation_calls(&id, &task_ref) {
        arguments["expected_revision"] = json!(&stale);
        let answer = client.call(name, arguments.clone());

        assert!(
            answer.is_error(),
            "{name} accepted a stale base: {}",
            answer.raw
        );
        assert_eq!(
            answer.code(),
            Some("revision_conflict"),
            "{name} refused for the wrong reason: {}",
            answer.raw
        );
        assert_eq!(
            answer.commit_state(),
            Some("not_committed"),
            "{name} did not say the store was untouched: {}",
            answer.raw
        );
        assert_eq!(
            before,
            sandbox.note_bytes(&id),
            "{name} changed the file on a stale base"
        );
    }

    let after = client.call("noteit_read", json!({ "note_id": &id }));
    assert_eq!(
        updated_at_before,
        after.structured()["note"]["updated_at"],
        "a refused mutation moved updated_at"
    );
}

// ------------------------------------------------------------------ MCP-14

/// Two agents, one base: one commits and one conflicts. Never both.
#[test]
fn mcp_14_two_clients_racing_from_one_base_produce_one_commit_and_one_conflict() {
    let sandbox = Sandbox::new();
    let mut writer = McpClient::start(&sandbox);
    let id = create_note(&mut writer, "BASE COMPARTILHADA");

    // Two separate server processes, each with its own connection, exactly as
    // two hosts on this machine would be.
    let mut a = McpClient::start(&sandbox);
    let mut b = McpClient::start(&sandbox);

    let base_a = read_revision(&mut a, &id);
    let base_b = read_revision(&mut b, &id);
    assert_eq!(base_a, base_b, "both read the same version");

    let answer_a = a.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "DE A", "expected_revision": &base_a }),
    );
    let answer_b = b.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "DE B", "expected_revision": &base_b }),
    );

    let committed = [&answer_a, &answer_b]
        .iter()
        .filter(|answer| answer.status() == "ok")
        .count();
    let conflicted = [&answer_a, &answer_b]
        .iter()
        .filter(|answer| answer.code() == Some("revision_conflict"))
        .count();
    assert_eq!(
        (committed, conflicted),
        (1, 1),
        "A: {}\nB: {}",
        answer_a.raw,
        answer_b.raw
    );

    // And the winner's text is the only one there. Not "both", which is what a
    // last-writer-wins interface would have produced with one of them lost.
    let body = sandbox.body(&id);
    let has_a = body.contains("DE A");
    let has_b = body.contains("DE B");
    assert!(
        has_a ^ has_b,
        "exactly one of the two writes must be in the note: {body:?}"
    );
    assert_eq!(body.lines().count(), 2, "{body:?}");
}

// ------------------------------------------------------------------ MCP-15

/// An append is never repeated on the client's behalf.
///
/// The failure this prevents is a paragraph landing in a note twice, so the
/// evidence has to be the note itself and the number of requests the authority
/// actually saw.
#[test]
fn mcp_15_an_append_is_never_retried_silently() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let revision = read_revision(&mut client, &id);

    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "UMA VEZ", "expected_revision": &revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(sandbox.body(&id), "BASE\nUMA VEZ");
    assert_eq!(
        sandbox.body(&id).matches("UMA VEZ").count(),
        1,
        "the paragraph is in the note more than once"
    );

    // Sending the same append again with the now-stale revision — which is
    // what a client that did not read the answer would do — conflicts rather
    // than duplicating.
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "UMA VEZ", "expected_revision": &revision }),
    );
    assert_eq!(answer.code(), Some("revision_conflict"), "{}", answer.raw);
    assert_eq!(sandbox.body(&id).matches("UMA VEZ").count(), 1);
}

// ------------------------------------------------------- MCP-16 … MCP-20

/// Each mutation, with the *current* revision, commits and moves the version
/// on. The stale halves are the matrix above; these are the positive cases,
/// one tool at a time, so a broken mapping is named rather than inferred.
#[test]
fn mcp_16_to_20_every_mutation_commits_on_the_current_revision() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let (id, task_ref) = seeded_note(&mut client);

    // Ordered so each one has something to act on: complete before reopen,
    // set before remove, and the two body rewrites before the tag work.
    let steps: Vec<(&str, serde_json::Value)> = vec![
        (
            "noteit_append",
            json!({ "note_id": &id, "text": "ACRESCENTADO" }),
        ),
        (
            "noteit_task_complete",
            json!({ "note_id": &id, "task_ref": &task_ref }),
        ),
        (
            "noteit_tag_add",
            json!({ "note_id": &id, "tag": "Cardiologia" }),
        ),
        (
            "noteit_tag_remove",
            json!({ "note_id": &id, "tag": "Medicina" }),
        ),
        (
            "noteit_property_set",
            json!({ "note_id": &id, "key": "fonte", "value": "Braunwald" }),
        ),
        (
            "noteit_property_remove",
            json!({ "note_id": &id, "key": "fonte" }),
        ),
        (
            "noteit_edit",
            json!({ "note_id": &id, "body": "TUDO NOVO\n\n- [x] feito" }),
        ),
        ("noteit_edit", json!({ "note_id": &id, "clear": true })),
    ];

    let mut revision = read_revision(&mut client, &id);
    for (name, mut arguments) in steps {
        arguments["expected_revision"] = json!(&revision);
        let answer = client.call(name, arguments);
        assert_eq!(answer.status(), "ok", "{name}: {}", answer.raw);
        assert_eq!(
            answer.commit_state(),
            Some("committed"),
            "{name} did not commit: {}",
            answer.raw
        );
        let next = answer.revision();
        assert_ne!(next, revision, "{name} did not move the revision");
        assert_eq!(
            next,
            read_revision(&mut client, &id),
            "{name} reported a revision the note does not have"
        );
        revision = next;
    }
}

/// Reopening a task is its own case, because it is the one mutation whose
/// stale half is easiest to leave out of a matrix by accident.
#[test]
fn mcp_20_reopening_a_task_preserves_the_precondition() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let (id, task_ref) = seeded_note(&mut client);

    let revision = read_revision(&mut client, &id);
    let answer = client.call(
        "noteit_task_complete",
        json!({ "note_id": &id, "task_ref": &task_ref, "expected_revision": &revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert!(sandbox.body(&id).contains("- [x]"), "{}", sandbox.body(&id));

    // The reference names the task *as it was*, and completing it changed the
    // task — so the old reference is stale and refused rather than guessed at.
    let after_complete = answer.revision();
    let answer = client.call(
        "noteit_task_reopen",
        json!({ "note_id": &id, "task_ref": &task_ref, "expected_revision": &after_complete }),
    );
    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(answer.code(), Some("stale_task_ref"), "{}", answer.raw);

    // With the reference the note now has, and the current revision, it works.
    let tasks = client.call("noteit_tasks_list", json!({ "state": "completed" }));
    let fresh_ref = tasks.structured()["tasks"][0]["task_ref"]
        .as_str()
        .expect("the completed task")
        .to_string();
    let answer = client.call(
        "noteit_task_reopen",
        json!({ "note_id": &id, "task_ref": fresh_ref, "expected_revision": &after_complete }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert!(sandbox.body(&id).contains("- [ ]"), "{}", sandbox.body(&id));

    // And a stale revision refuses it even with a good reference.
    let tasks = client.call("noteit_tasks_list", json!({ "state": "pending" }));
    let newest_ref = tasks.structured()["tasks"][0]["task_ref"]
        .as_str()
        .unwrap()
        .to_string();
    let before = sandbox.note_bytes(&id);
    let answer = client.call(
        "noteit_task_complete",
        json!({ "note_id": &id, "task_ref": newest_ref, "expected_revision": &after_complete }),
    );
    assert_eq!(answer.code(), Some("revision_conflict"), "{}", answer.raw);
    assert_eq!(before, sandbox.note_bytes(&id));
}

// ------------------------------------------------------------------ MCP-24

/// The answer was lost, so the result is genuinely unknown — and is said so.
///
/// A real authority holds the lease and hangs up after reading the request.
/// From here there is no way to tell whether it committed first, and inventing
/// an answer either way is how a client either loses a change or duplicates
/// one.
#[test]
fn mcp_24_a_lost_answer_is_unknown_and_is_never_repeated() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE");
    let id = id.to_string();
    let before = sandbox.note_bytes(&id);

    // Read the revision before the authority takes the lease: a read needs no
    // lease at all, which is itself worth having proved here.
    let mut client = McpClient::start(&sandbox);
    let revision = read_revision(&mut client, &id);

    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::HangUpAfterRequest);
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "PODE TER SIDO GRAVADO", "expected_revision": &revision }),
    );

    assert_eq!(
        answer.status(),
        "indeterminate",
        "a lost answer must not be reported as a failure: {}",
        answer.raw
    );
    assert_eq!(answer.code(), Some("indeterminate"), "{}", answer.raw);
    assert_eq!(
        answer.commit_state(),
        Some("unknown"),
        "the one thing this case must never say is `not_committed`: {}",
        answer.raw
    );
    // Flagged, so no host mistakes it for a completed write.
    assert!(answer.is_error(), "{}", answer.raw);

    // Exactly one request went out. Not two: an automatic retry here is how
    // the same paragraph lands in the note twice.
    assert_eq!(
        authority.handled(),
        1,
        "the request was sent more than once"
    );
    // And this server wrote nothing itself: the authority holds the lease, so
    // the only thing that could have changed the file is the authority, which
    // in this case did not.
    assert_eq!(before, sandbox.note_bytes(&id));
}

/// An authority that cannot be reached at all fails closed: nothing written,
/// and said plainly rather than worked around.
#[test]
fn mcp_24_an_unreachable_authority_writes_nothing() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let before = sandbox.note_bytes(&id);
    let mut client = McpClient::start(&sandbox);
    let revision = read_revision(&mut client, &id);

    // The lease is held and there is no socket to ask: the shape of a Note-it
    // that is starting up, or one that died holding the store.
    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare");
    let lease = noteit_core::coordination::WriterLease::try_acquire_prepared(&coordination)
        .expect("prepare")
        .expect("take the lease");

    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "NUNCA", "expected_revision": &revision }),
    );
    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(
        answer.code(),
        Some("authority_unavailable"),
        "{}",
        answer.raw
    );
    assert_eq!(
        answer.commit_state(),
        Some("not_committed"),
        "{}",
        answer.raw
    );
    assert_eq!(before, sandbox.note_bytes(&id), "a fail-closed path wrote");
    drop(lease);
}
