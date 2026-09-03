//! 4.1R1 / AUD-03: the decisions this contract made, pinned so a change in the
//! Core has to meet them.
//!
//! The MCP surface deliberately publishes less than the Core knows. Each
//! omission below is a decision, and a decision is only worth anything if
//! somebody is made to revisit it when its premise moves. The mechanism is an
//! exhaustive `match` over the Core's own enum, with **no** wildcard arm: a new
//! variant does not compile here, and the compiler names it.
//!
//! ## Why this is a test and not a function in the crate
//!
//! It used to be a function in `domain.rs`, written with `matches!` and marked
//! `#[allow(dead_code)]`, whose comment claimed that a new `WriteOutcomeKind`
//! "makes somebody look at this boundary". It did not. `matches!` is an
//! expression that answers `false` for a pattern it does not list, so a new
//! variant compiled perfectly and the function — which nothing called — went
//! on returning `true` for the ten it knew and `false` for the eleventh.
//!
//! An exhaustive `match` really does fail to compile, so the guarantee is now
//! real. Putting it here rather than in the crate keeps the shipped binary free
//! of code that exists only to be compiled, and loses nothing: a test is
//! compiled by `cargo test`, which is a gate in `scripts/check` and a step in
//! CI, so the compile error arrives at exactly the same moment it would have.

mod support;

use noteit_core::write::WriteOutcomeKind;
use serde_json::json;
use support::{create_note, read_revision, McpClient, Sandbox};

/// What the MCP contract decided to do about each outcome the Core can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    /// The agent asked for this by calling a specific tool, so naming it again
    /// in the answer tells it nothing it did not already know.
    ImpliedByTheToolThatWasCalled,
}

/// Every `WriteOutcomeKind`, and what this boundary does with it.
///
/// Exhaustive, deliberately without a `_` arm. Adding a variant to the Core is
/// a compile error here, and closing it means deciding — in this file, in
/// writing — whether the new outcome is still implied by the tool that was
/// called or whether it is something an agent has to be told.
fn decision_for(kind: WriteOutcomeKind) -> Decision {
    match kind {
        WriteOutcomeKind::NoteCreated
        | WriteOutcomeKind::ContentAppended
        | WriteOutcomeKind::ContentReplaced
        | WriteOutcomeKind::ContentCleared
        | WriteOutcomeKind::TagAdded
        | WriteOutcomeKind::TagRemoved
        | WriteOutcomeKind::PropertySet
        | WriteOutcomeKind::PropertyRemoved
        | WriteOutcomeKind::TaskCompleted
        | WriteOutcomeKind::TaskReopened
        | WriteOutcomeKind::NoteRestored => Decision::ImpliedByTheToolThatWasCalled,
    }
}

/// The eleven outcomes the Core names today, each one decided.
///
/// The list is not the guarantee — `decision_for` is, because it does not
/// compile with a variant missing. This checks the other direction: that the
/// count still matches what the documentation describes, and that every entry
/// really does reach the `match`.
#[test]
fn every_outcome_kind_the_core_can_name_has_a_decision() {
    let kinds = [
        WriteOutcomeKind::NoteCreated,
        WriteOutcomeKind::ContentAppended,
        WriteOutcomeKind::ContentReplaced,
        WriteOutcomeKind::ContentCleared,
        WriteOutcomeKind::TagAdded,
        WriteOutcomeKind::TagRemoved,
        WriteOutcomeKind::PropertySet,
        WriteOutcomeKind::PropertyRemoved,
        WriteOutcomeKind::TaskCompleted,
        WriteOutcomeKind::TaskReopened,
        WriteOutcomeKind::NoteRestored,
    ];
    assert_eq!(
        kinds.len(),
        11,
        "the Core names a different number of outcomes; the decision above has to be looked at"
    );
    for kind in kinds {
        assert_eq!(decision_for(kind), Decision::ImpliedByTheToolThatWasCalled);
    }
}

/// And the decision is what the wire actually shows.
///
/// A pinned decision that the code had quietly stopped honouring would be worse
/// than none, so this reads a real answer off a real server: no `kind`, and the
/// fields that *are* published all present.
#[test]
fn a_write_answer_carries_no_outcome_kind_and_says_everything_else() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let revision = read_revision(&mut client, &id);

    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "MAIS", "expected_revision": revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);

    let structured = answer.structured();
    assert!(
        structured.get("kind").is_none(),
        "the contract publishes `kind` after all; the decision in this file is out of date: {}",
        answer.raw
    );
    for field in ["status", "commit_state", "note_id", "changed", "revision"] {
        assert!(
            structured.get(field).is_some(),
            "a committed write did not publish `{field}`: {}",
            answer.raw
        );
    }

    // And the published output schema agrees, so a client generating types from
    // it sees the same surface this test does.
    let tool = client
        .list_tools()
        .into_iter()
        .find(|tool| tool["name"] == "noteit_append")
        .expect("noteit_append");
    assert!(
        tool["outputSchema"]["properties"].get("kind").is_none(),
        "the output schema declares `kind`: {}",
        tool["outputSchema"]
    );
}
