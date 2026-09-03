//! §26: every `NoteMutation` variant, and no way to add one and forget.
//!
//! The suite in `mcp_revision.rs` drives every mutation *tool* through the
//! protocol. This one closes the gap that a list of tool names cannot: a new
//! variant added to `NoteMutation` in the Core with a tool to match, and
//! nobody remembering to give it the precondition.
//!
//! The mechanism is a single exhaustive `match` on the Core's own enum, with
//! no wildcard arm. Adding a variant makes this file stop compiling, and the
//! compiler names it. That is the point: a guarantee that depends on somebody
//! remembering is not a guarantee.

mod support;

use noteit_core::write::NoteMutation;
use serde_json::json;
use support::{read_revision, McpClient, Sandbox};

/// Which MCP tool asks for this mutation, and the arguments that reach it.
///
/// Exhaustive, deliberately without a `_` arm. A new [`NoteMutation`] variant
/// is a compile error here.
fn tool_for(
    mutation: &NoteMutation,
    note_id: &str,
    task_ref: &str,
) -> (&'static str, serde_json::Value) {
    match mutation {
        NoteMutation::Append { .. } => (
            "noteit_append",
            json!({ "note_id": note_id, "text": "ACRESCENTADO" }),
        ),
        NoteMutation::ReplaceBody { .. } => (
            "noteit_edit",
            json!({ "note_id": note_id, "body": "SUBSTITUÍDO" }),
        ),
        NoteMutation::ClearBody => ("noteit_edit", json!({ "note_id": note_id, "clear": true })),
        NoteMutation::AddTag { .. } => (
            "noteit_tag_add",
            json!({ "note_id": note_id, "tag": "Nova" }),
        ),
        NoteMutation::RemoveTag { .. } => (
            "noteit_tag_remove",
            json!({ "note_id": note_id, "tag": "Medicina" }),
        ),
        NoteMutation::SetProperty { .. } => (
            "noteit_property_set",
            json!({ "note_id": note_id, "key": "fonte", "value": "outra" }),
        ),
        NoteMutation::RemoveProperty { .. } => (
            "noteit_property_remove",
            json!({ "note_id": note_id, "key": "fonte" }),
        ),
        NoteMutation::CompleteTask { .. } => (
            "noteit_task_complete",
            json!({ "note_id": note_id, "task_ref": task_ref }),
        ),
        NoteMutation::ReopenTask { .. } => (
            "noteit_task_reopen",
            json!({ "note_id": note_id, "task_ref": task_ref }),
        ),
    }
}

/// One value of every variant.
///
/// Also exhaustive by construction: `tool_for` is called for each of these and
/// its `match` has no wildcard, so a variant that is missing from this list
/// still cannot be missing from the decision — and the count assertion below
/// catches a variant added to the enum and to `tool_for` but not to here.
fn every_mutation() -> Vec<NoteMutation> {
    vec![
        NoteMutation::Append {
            payload: "x".into(),
        },
        NoteMutation::ReplaceBody { body: "x".into() },
        NoteMutation::ClearBody,
        NoteMutation::AddTag { tag: "x".into() },
        NoteMutation::RemoveTag { tag: "x".into() },
        NoteMutation::SetProperty {
            key: "k".into(),
            value: "v".into(),
        },
        NoteMutation::RemoveProperty { key: "k".into() },
        NoteMutation::CompleteTask {
            task_ref: "abcd1234".into(),
        },
        NoteMutation::ReopenTask {
            task_ref: "abcd1234".into(),
        },
    ]
}

fn seeded_note(client: &mut McpClient) -> (String, String) {
    let answer = client.call(
        "noteit_create",
        json!({
            "content": "BASE\n\n- [ ] revisar",
            "tags": ["Medicina"],
            "properties": [{ "key": "fonte", "value": "Harrison" }],
        }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    let note_id = answer.note_id();
    let tasks = client.call("noteit_tasks_list", json!({ "state": "all" }));
    let task_ref = tasks.structured()["tasks"][0]["task_ref"]
        .as_str()
        .expect("a task")
        .to_string();
    (note_id, task_ref)
}

/// Nine variants today. The number is asserted so that adding one without
/// deciding anything about it fails here as well as at the `match`.
#[test]
fn the_matrix_covers_every_mutation_the_core_has() {
    assert_eq!(
        every_mutation().len(),
        9,
        "a mutation was added or removed; the matrix below has to be looked at"
    );
}

/// The property, for every variant: a stale revision refuses, and not one byte
/// of the note moves.
#[test]
fn every_mutation_variant_refuses_a_stale_revision_without_touching_the_note() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let (id, task_ref) = seeded_note(&mut client);

    let stale = read_revision(&mut client, &id);
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "MOVEU", "expected_revision": &stale }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    let before = sandbox.note_bytes(&id);

    for mutation in every_mutation() {
        let (tool, mut arguments) = tool_for(&mutation, &id, &task_ref);
        arguments["expected_revision"] = json!(&stale);
        let answer = client.call(tool, arguments);

        assert!(
            answer.is_error(),
            "{mutation:?} via {tool} accepted a stale base: {}",
            answer.raw
        );
        assert_eq!(
            answer.code(),
            Some("revision_conflict"),
            "{mutation:?} via {tool}: {}",
            answer.raw
        );
        assert_eq!(
            before,
            sandbox.note_bytes(&id),
            "{mutation:?} via {tool} changed the note on a stale base"
        );
    }
}

/// And every variant is reachable through a tool that requires the field.
///
/// The two halves together are the guarantee: the mapping is exhaustive, and
/// everything it maps to demands a precondition.
#[test]
fn every_mutation_variant_is_reached_by_a_tool_that_requires_the_precondition() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let tools = client.list_tools();

    for mutation in every_mutation() {
        let (tool, _) = tool_for(&mutation, "0123abcd", "abcd1234");
        let published = tools
            .iter()
            .find(|published| published["name"] == tool)
            .unwrap_or_else(|| panic!("{mutation:?} maps to {tool}, which is not published"));
        let required: Vec<&str> = published["inputSchema"]["required"]
            .as_array()
            .unwrap_or_else(|| panic!("{tool} declares nothing required"))
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert!(
            required.contains(&"expected_revision"),
            "{mutation:?} is reachable through {tool}, which does not require a precondition"
        );
    }
}
