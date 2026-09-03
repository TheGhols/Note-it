//! §26: every `NoteMutation` variant, and no way to add one and forget.
//!
//! The suite in `mcp_revision.rs` drives every mutation *tool* through the
//! protocol. This one closes the gap that a list of tool names cannot: a new
//! variant added to `NoteMutation` in the Core with a tool to match, and
//! nobody remembering to give it the precondition.
//!
//! ## Why the matrix is declared, and not written twice
//!
//! The first version of this file had two artefacts: an exhaustive `match`
//! from variant to tool, and a hand-written list of sample values to iterate.
//! The `match` did force a decision about every new variant — it does not
//! compile without one — but the *list* did not follow. A variant added to the
//! Core, given an arm in the `match` to make it compile, and forgotten in the
//! list would have been decided and never exercised, and the count assertion
//! guarding the list would have gone on reading nine.
//!
//! So there is one declaration now. [`mutation_matrix!`] takes one row per
//! variant and generates both the sample value and the `match` arm from it.
//! The `match` still has no wildcard, so a new variant fails to compile and the
//! compiler names it; and because the row that satisfies the compiler is the
//! same row that produces the value, there is no way to satisfy it and still
//! skip the variant. Each row also checks at run time that its sample really is
//! the variant the row names, so a copy-paste that tests one variant twice and
//! another never is caught rather than hidden.

mod support;

use noteit_core::write::NoteMutation;
use serde_json::json;
use support::{read_revision, McpClient, Sandbox};

/// The matrix itself: one row per `NoteMutation` variant.
///
/// A row is `Variant: <sample value> => "<tool>", <arguments>`, where the
/// arguments are a closure of `(note_id, task_ref)` so that a row can name the
/// note it acts on. From the rows the macro builds:
///
/// - [`every_mutation`], the values the tests below iterate;
/// - [`tool_for`], an exhaustive `match` with **no** wildcard arm.
///
/// Both come from the same rows, so they cannot drift apart. See the module
/// documentation for why that matters.
macro_rules! mutation_matrix {
    ($( $variant:ident : $sample:expr => $tool:literal , $arguments:expr );+ $(;)?) => {
        /// One value of every variant, in the order the rows declare them.
        ///
        /// Complete because [`tool_for`] is: a variant with no row does not
        /// compile there, and a row that compiles produces a value here.
        fn every_mutation() -> Vec<NoteMutation> {
            vec![
                $({
                    let sample = $sample;
                    // A row whose value is not the variant it names would test
                    // one variant twice and another never — silently, since
                    // both the count and the `match` would still be satisfied.
                    assert!(
                        matches!(sample, NoteMutation::$variant { .. }),
                        concat!(
                            "the matrix row for ",
                            stringify!($variant),
                            " carries a value of a different variant",
                        )
                    );
                    sample
                }),+
            ]
        }

        /// Which MCP tool asks for this mutation, and the arguments it takes.
        ///
        /// Exhaustive, deliberately without a `_` arm. A new [`NoteMutation`]
        /// variant is a compile error here, and the compiler names it.
        fn tool_for(
            mutation: &NoteMutation,
            note_id: &str,
            task_ref: &str,
        ) -> (&'static str, serde_json::Value) {
            match mutation {
                $( NoteMutation::$variant { .. } => ($tool, ($arguments)(note_id, task_ref)) ),+
            }
        }

        /// The variants the matrix names, for the tests that report on it.
        fn matrix_variants() -> Vec<&'static str> {
            vec![ $( stringify!($variant) ),+ ]
        }
    };
}

mutation_matrix! {
    Append:
        NoteMutation::Append { payload: "x".into() }
        => "noteit_append",
           |note_id: &str, _task_ref: &str| json!({ "note_id": note_id, "text": "ACRESCENTADO" });

    ReplaceBody:
        NoteMutation::ReplaceBody { body: "x".into() }
        => "noteit_edit",
           |note_id: &str, _task_ref: &str| json!({ "note_id": note_id, "body": "SUBSTITUÍDO" });

    ClearBody:
        NoteMutation::ClearBody
        => "noteit_edit",
           |note_id: &str, _task_ref: &str| json!({ "note_id": note_id, "clear": true });

    AddTag:
        NoteMutation::AddTag { tag: "x".into() }
        => "noteit_tag_add",
           |note_id: &str, _task_ref: &str| json!({ "note_id": note_id, "tag": "Nova" });

    RemoveTag:
        NoteMutation::RemoveTag { tag: "x".into() }
        => "noteit_tag_remove",
           |note_id: &str, _task_ref: &str| json!({ "note_id": note_id, "tag": "Medicina" });

    SetProperty:
        NoteMutation::SetProperty { key: "k".into(), value: "v".into() }
        => "noteit_property_set",
           |note_id: &str, _task_ref: &str| json!({ "note_id": note_id, "key": "fonte", "value": "outra" });

    RemoveProperty:
        NoteMutation::RemoveProperty { key: "k".into() }
        => "noteit_property_remove",
           |note_id: &str, _task_ref: &str| json!({ "note_id": note_id, "key": "fonte" });

    CompleteTask:
        NoteMutation::CompleteTask { task_ref: "abcd1234".into() }
        => "noteit_task_complete",
           |note_id: &str, task_ref: &str| json!({ "note_id": note_id, "task_ref": task_ref });

    ReopenTask:
        NoteMutation::ReopenTask { task_ref: "abcd1234".into() }
        => "noteit_task_reopen",
           |note_id: &str, task_ref: &str| json!({ "note_id": note_id, "task_ref": task_ref });
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

/// The matrix names nine variants, each exactly once.
///
/// This is **not** the exhaustiveness proof — that is the `match` inside
/// `mutation_matrix!`, which does not compile if a variant has no row. What is
/// checked here is the other direction, which a compiler cannot check: that no
/// variant was given two rows, and that the number still matches the nine the
/// documentation and `docs/mcp.md` describe. A tenth appearing here is a
/// prompt to update those, not a failure of the guarantee.
#[test]
fn the_matrix_names_every_variant_exactly_once() {
    let named = matrix_variants();
    let mut unique = named.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        named.len(),
        unique.len(),
        "a variant has more than one row in the matrix: {named:?}"
    );
    assert_eq!(
        every_mutation().len(),
        named.len(),
        "the generated list and the generated match disagree about the row count"
    );
    assert_eq!(
        named.len(),
        9,
        "the matrix now names {} variants; the documentation says nine and has to be updated: {named:?}",
        named.len()
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
