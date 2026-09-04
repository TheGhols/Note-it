//! MCP-01 … MCP-09, MCP-30: the server as a host meets it.
//!
//! Everything in this file drives the real `noteit-mcp` binary as a child
//! process, over real pipes, against a real store in a temporary directory.
//! Nothing is mocked, because the properties being checked here — that the
//! process starts headless, that standard output carries nothing but protocol,
//! that a read touches no disk — are properties of a process and not of a
//! function.

mod support;

use serde_json::json;
use support::{create_note, fingerprint, read_revision, McpClient, Sandbox};

// ------------------------------------------------------------------ MCP-01

#[test]
fn mcp_01_the_server_starts_over_stdio_without_creating_a_store() {
    // A bare sandbox: no XDG directories at all, so anything the server
    // creates on the way up would be visible.
    let sandbox = Sandbox::bare();
    let before = fingerprint(&sandbox.root);

    let mut client = McpClient::spawn(&sandbox);
    let info = client.initialize(support::HANDSHAKE_PROTOCOL_VERSION);

    assert_eq!(info["serverInfo"]["name"], "noteit-mcp");
    assert!(
        info["capabilities"].get("tools").is_some(),
        "a tools server must say so: {info}"
    );
    // A window would need a display, and there is none; a store would need a
    // directory, and there is none of that either.
    assert_eq!(
        before,
        fingerprint(&sandbox.root),
        "starting the server created something"
    );

    let finished = client.finish();
    assert!(
        finished.trailing_stdout.trim().is_empty(),
        "bytes left on stdout: {:?}",
        finished.trailing_stdout
    );
}

// ------------------------------------------------------------------ MCP-02

#[test]
fn mcp_02_standard_output_carries_nothing_but_the_protocol() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);

    // Every kind of traffic there is: a listing, a read that fails, a write
    // that succeeds, a write that is refused. Each answer is read by
    // `read_message`, which insists that a line is a whole JSON-RPC message —
    // so a banner, a warning or a stray `println!` anywhere in this sequence
    // is a panic in the harness and not a subtle diff.
    client.list_tools();
    client.call("noteit_list", json!({}));
    client.call("noteit_read", json!({ "note_id": "deadbeef" }));
    let id = create_note(&mut client, "PRIMEIRA");
    let revision = read_revision(&mut client, &id);
    client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "SEGUNDA", "expected_revision": revision }),
    );
    client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "TERCEIRA", "expected_revision": "0".repeat(64) }),
    );

    let finished = client.finish();
    assert!(
        finished.trailing_stdout.trim().is_empty(),
        "bytes left on stdout after the last answer: {:?}",
        finished.trailing_stdout
    );
    // Diagnostics are allowed on stderr, but a note's body never is.
    for secret in ["PRIMEIRA", "SEGUNDA", "TERCEIRA"] {
        assert!(
            !finished.stderr.contains(secret),
            "a note's text reached stderr: {}",
            finished.stderr
        );
    }
}

// ------------------------------------------------------------------ MCP-03

/// Exactly the documented catalogue, and in a stable order.
///
/// Both halves matter. A missing tool is a broken contract; an *extra* one is
/// worse, because a tool nobody wrote down is a tool nobody audited.
#[test]
fn mcp_03_the_catalogue_is_exactly_the_documented_tools() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);

    let tools = client.list_tools();
    let names: Vec<String> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name").to_string())
        .collect();

    let expected: Vec<String> = noteit_mcp::contract::TOOL_NAMES
        .iter()
        .map(|name| name.to_string())
        .collect();
    assert_eq!(names, expected, "the catalogue is not what is documented");

    // Asking twice gives the same order, so a client may cache it.
    let again: Vec<String> = client
        .list_tools()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, again, "the catalogue order is not stable");

    for tool in &tools {
        assert!(
            tool["description"].as_str().is_some_and(|d| !d.is_empty()),
            "{} has no description",
            tool["name"]
        );
        assert_eq!(
            tool["inputSchema"]["type"], "object",
            "{} has no object input schema",
            tool["name"]
        );
        assert!(
            tool.get("outputSchema").is_some(),
            "{} publishes no output schema, so a client would have to read prose",
            tool["name"]
        );
    }
}

// ------------------------------------------------------------------ MCP-04

/// The property this whole server exists for, read off the published schema.
///
/// Not "the code checks it" — the *schema* says the field is required, so a
/// request without it never reaches this crate's code at all.
#[test]
fn mcp_04_every_mutation_of_an_existing_note_requires_a_revision() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let tools = client.list_tools();

    // Every tool that changes a note that already exists. `noteit_create`
    // makes one and cannot have read an earlier version; `noteit_trash_restore`
    // moves a file and is not an edit.
    let mutations = [
        "noteit_append",
        "noteit_edit",
        "noteit_tag_add",
        "noteit_tag_remove",
        "noteit_property_set",
        "noteit_property_remove",
        "noteit_task_complete",
        "noteit_task_reopen",
    ];

    for name in mutations {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("{name} is missing from the catalogue"));
        let schema = &tool["inputSchema"];
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap_or_else(|| panic!("{name} declares no required fields"))
            .iter()
            .filter_map(|value| value.as_str())
            .collect();

        assert!(
            required.contains(&"expected_revision"),
            "{name} does not require expected_revision: {schema}"
        );
        assert!(
            required.contains(&"note_id"),
            "{name} does not require note_id: {schema}"
        );
        assert_eq!(
            schema["properties"]["expected_revision"]["type"], "string",
            "{name}'s expected_revision is not a string: {schema}"
        );
    }

    // And the two that must not ask for one.
    for name in ["noteit_create", "noteit_trash_restore"] {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        assert!(
            tool["inputSchema"]["properties"]
                .get("expected_revision")
                .is_none(),
            "{name} invented a precondition it cannot honour: {}",
            tool["inputSchema"]
        );
    }
}

/// The schema is not the only guard: a request that reaches the server without
/// the field is refused before anything is written.
#[test]
fn mcp_04_a_request_without_the_field_is_refused_by_the_protocol() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let before = sandbox.note_bytes(&id);

    // The arguments are refused while being deserialized, which is earlier
    // than anything in this repository runs: the tool body is never entered,
    // no store is opened and no lease is taken. MCP calls that a protocol
    // error rather than a tool failure — a request whose shape is wrong was
    // never a call — so it is JSON-RPC `-32602` and carries no result at all.
    //
    // Since Phase 4.2R.R1 it does not name the missing field either. The field
    // name was `serde_json`'s to give, and `serde_json` gives it inside a
    // sentence that also quotes whatever the client sent; the schema in
    // `tools/list` is where the required fields are published. See
    // `mcp_argument_boundary.rs` and ADR-055.
    client.call_refused_by_the_argument_boundary(
        "noteit_append",
        json!({ "note_id": &id, "text": "SEM REVISÃO" }),
    );
    assert_eq!(
        before,
        sandbox.note_bytes(&id),
        "a refused request changed the file"
    );

    // The same for every other mutation, so this is a property of the
    // boundary and not a fact about `noteit_append`.
    for (name, arguments) in [
        ("noteit_edit", json!({ "note_id": &id, "body": "X" })),
        ("noteit_tag_add", json!({ "note_id": &id, "tag": "x" })),
        ("noteit_tag_remove", json!({ "note_id": &id, "tag": "x" })),
        (
            "noteit_property_set",
            json!({ "note_id": &id, "key": "k", "value": "v" }),
        ),
        (
            "noteit_property_remove",
            json!({ "note_id": &id, "key": "k" }),
        ),
        (
            "noteit_task_complete",
            json!({ "note_id": &id, "task_ref": "abcd1234" }),
        ),
        (
            "noteit_task_reopen",
            json!({ "note_id": &id, "task_ref": "abcd1234" }),
        ),
    ] {
        client.call_refused_by_the_argument_boundary(name, arguments);
        assert_eq!(
            before,
            sandbox.note_bytes(&id),
            "{name} changed the file without a precondition"
        );
    }
}

// ------------------------------------------------------------------ MCP-05

/// A read prepares nothing.
///
/// The store here does not exist: no data directory, no config, no state, no
/// runtime directory. Every read-only tool has to answer anyway and leave the
/// world byte-for-byte as it found it.
#[test]
fn mcp_05_read_only_tools_create_nothing_at_all() {
    let sandbox = Sandbox::bare();
    let before = fingerprint(&sandbox.root);
    assert!(before.is_empty(), "the sandbox was not empty to begin with");

    let mut client = McpClient::start(&sandbox);
    for (name, arguments) in [
        ("noteit_list", json!({})),
        ("noteit_read", json!({ "note_id": "0123abcd" })),
        ("noteit_search", json!({ "query": "qualquer" })),
        ("noteit_tasks_list", json!({})),
        ("noteit_trash_list", json!({})),
    ] {
        let answer = client.call(name, arguments);
        assert!(
            answer.structured().get("status").is_some(),
            "{name} answered without a status: {}",
            answer.raw
        );
    }
    drop(client);

    assert_eq!(
        before,
        fingerprint(&sandbox.root),
        "a read-only tool created something: {:?}",
        fingerprint(&sandbox.root)
    );
}

// ------------------------------------------------------------------ MCP-06

#[test]
fn mcp_06_list_answers_a_synthetic_store() {
    let sandbox = Sandbox::new();
    let first = sandbox.seed("PRIMEIRA NOTA\ncorpo");
    let second = sandbox.seed("SEGUNDA NOTA");
    let mut client = McpClient::start(&sandbox);

    let answer = client.call("noteit_list", json!({}));
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert!(!answer.is_error());
    let notes = answer.structured()["notes"].as_array().expect("notes");
    assert_eq!(notes.len(), 2, "{}", answer.raw);
    assert_eq!(answer.structured()["count"], 2);

    let ids: Vec<&str> = notes
        .iter()
        .map(|note| note["note_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&first.to_string().as_str()));
    assert!(ids.contains(&second.to_string().as_str()));
    // Full UUIDs, never the eight-character prefix a person reads.
    for id in ids {
        assert_eq!(
            id.len(),
            36,
            "a listing must publish full identifiers: {id}"
        );
    }
    // A listing is a listing and never a version: a summary carries no
    // revision, because nobody may build a write from a snippet.
    assert!(
        notes[0].get("revision").is_none(),
        "a summary must not look like a base for a write: {}",
        notes[0]
    );
}

#[test]
fn mcp_06_a_listing_can_be_filtered_and_bounded() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let tagged = create_note(&mut client, "COM TAG");
    let revision = read_revision(&mut client, &tagged);
    let answer = client.call(
        "noteit_tag_add",
        json!({ "note_id": &tagged, "tag": "Medicina", "expected_revision": revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    create_note(&mut client, "SEM TAG");

    let answer = client.call("noteit_list", json!({ "tags": ["medicina"] }));
    assert_eq!(answer.structured()["count"], 1, "{}", answer.raw);
    assert_eq!(answer.structured()["notes"][0]["note_id"], tagged);

    let answer = client.call("noteit_list", json!({ "limit": 1 }));
    assert_eq!(answer.structured()["count"], 1, "{}", answer.raw);
}

// ------------------------------------------------------------------ MCP-07

#[test]
fn mcp_07_read_answers_the_content_and_the_revision() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "TÍTULO\n\ncorpo com acento e emoji 🩺");

    let answer = client.call("noteit_read", json!({ "note_id": &id }));
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    let note = &answer.structured()["note"];
    assert_eq!(note["note_id"], id);
    assert_eq!(note["content"], "TÍTULO\n\ncorpo com acento e emoji 🩺");
    assert_eq!(note["label"], "TÍTULO");
    assert!(note["created_at"].is_string());

    let revision = note["revision"].as_str().expect("a revision");
    assert_eq!(revision.len(), 64, "{revision}");
    assert!(
        revision
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "lowercase hexadecimal only: {revision}"
    );
    // Reading twice without a change is the same version.
    assert_eq!(revision, read_revision(&mut client, &id));

    // The eight-character prefix a person would type resolves to the same note.
    let short = &id[..8];
    let answer = client.call("noteit_read", json!({ "note_id": short }));
    assert_eq!(answer.structured()["note"]["note_id"], id, "{}", answer.raw);
}

#[test]
fn mcp_07_reading_a_note_that_is_not_there_is_a_typed_refusal() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);

    let answer = client.call("noteit_read", json!({ "note_id": "0123abcd" }));
    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(answer.status(), "error");
    assert_eq!(answer.code(), Some("not_found"));
    assert!(answer.structured()["note"].is_null() || answer.structured().get("note").is_none());
}

// ------------------------------------------------------------------ MCP-08

#[test]
fn mcp_08_search_finds_without_touching_a_file() {
    let sandbox = Sandbox::new();
    sandbox.seed("Choque séptico\nnoradrenalina primeiro");
    sandbox.seed("Outra nota\nsem relação");
    let before = fingerprint(&sandbox.store_paths().notes_dir);

    let mut client = McpClient::start(&sandbox);
    let answer = client.call("noteit_search", json!({ "query": "noradrenalina" }));
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(answer.structured()["count"], 1, "{}", answer.raw);
    assert_eq!(answer.structured()["query"], "noradrenalina");
    assert_eq!(answer.structured()["results"][0]["match_count"], 1);

    // Accents fold, exactly as the rest of Note-it searches.
    let answer = client.call("noteit_search", json!({ "query": "septico" }));
    assert_eq!(answer.structured()["count"], 1, "{}", answer.raw);

    // An empty query lists rather than matching nothing.
    let answer = client.call("noteit_search", json!({ "query": "" }));
    assert_eq!(answer.structured()["count"], 2, "{}", answer.raw);

    drop(client);
    assert_eq!(
        before,
        fingerprint(&sandbox.store_paths().notes_dir),
        "a search changed a note"
    );
}

// ------------------------------------------------------------------ MCP-09

#[test]
fn mcp_09_create_answers_a_uuid_and_a_revision() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);

    let answer = client.call(
        "noteit_create",
        json!({
            "content": "NOVA",
            "tags": ["Medicina"],
            "properties": [{ "key": "fonte", "value": "Harrison" }],
        }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert!(!answer.is_error());
    assert_eq!(answer.commit_state(), Some("committed"));
    assert_eq!(answer.structured()["changed"], true);

    let id = answer.note_id();
    noteit_core::Uuid::parse_str(&id).expect("a create must answer a full UUID");
    let revision = answer.revision();
    assert_eq!(revision.len(), 64);

    // The revision a create hands back is usable as a precondition with no
    // extra read, which is the whole point of publishing it.
    assert_eq!(revision, read_revision(&mut client, &id));
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "SEGUIDA", "expected_revision": revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(sandbox.body(&id), "NOVA\nSEGUIDA");
}

// ------------------------------------------------------------------ MCP-30

/// The catalogue offers no way out of the domain.
///
/// Not a review of the names — a mechanical check that nothing in the
/// published surface takes a path, a command or a filename, and that no tool
/// is called any of the things a general-purpose escape hatch is called.
#[test]
fn mcp_30_no_tool_offers_a_filesystem_or_a_shell() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let tools = client.list_tools();

    const FORBIDDEN_NAMES: &[&str] = &[
        "read_file",
        "write_file",
        "list_directory",
        "delete_file",
        "shell",
        "exec",
        "bash",
        "open_path",
        "run_noteit",
        "raw_command",
        "execute_shell",
        "edit_markdown",
    ];
    // Any argument that could name something outside the domain.
    const FORBIDDEN_ARGUMENTS: &[&str] = &[
        "path",
        "file",
        "filename",
        "file_path",
        "directory",
        "dir",
        "command",
        "cmd",
        "shell",
        "argv",
        "store",
        "socket",
        "runtime",
        "url",
        "host",
        "port",
    ];

    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        assert!(
            name.starts_with("noteit_"),
            "a tool outside the domain namespace: {name}"
        );
        for forbidden in FORBIDDEN_NAMES {
            assert!(
                !name.contains(forbidden),
                "{name} names a general-purpose escape hatch"
            );
        }
        let properties = tool["inputSchema"]["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for argument in properties.keys() {
            assert!(
                !FORBIDDEN_ARGUMENTS.contains(&argument.as_str()),
                "{name} takes `{argument}`, which is not a Note-it concept"
            );
        }
    }

    // And a tool that does not exist is refused rather than improvised.
    let error = client.call_expecting_protocol_error("read_file", json!({ "path": "/etc/passwd" }));
    assert!(
        error["code"].as_i64().is_some(),
        "an unknown tool must be a protocol error: {error}"
    );
}

/// The private control protocol is not published here.
///
/// Two boundaries exist and only one of them is MCP. Nothing a host can see
/// may name the socket, the lease, the runtime directory or which of the two
/// write paths a change took — those belong to a conversation between two
/// Note-it processes.
#[test]
fn mcp_30_the_private_protocol_never_appears_in_the_public_surface() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let revision = read_revision(&mut client, &id);
    let write = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "MAIS", "expected_revision": revision }),
    );

    let catalogue = serde_json::to_string(&client.list_tools()).unwrap();
    let answer = serde_json::to_string(&write.raw).unwrap();
    for surface in [&catalogue, &answer] {
        for leak in [
            "writer.lock",
            "control.sock",
            "protocol_version",
            "request_id",
            "write_path",
            "WritePath",
            "lease",
            "XDG_RUNTIME_DIR",
        ] {
            assert!(
                !surface.contains(leak),
                "`{leak}` reached the MCP surface: {surface}"
            );
        }
    }
    // Not even the store's own location.
    let root = sandbox.root.display().to_string();
    assert!(
        !answer.contains(&root),
        "a write answer published a filesystem path: {answer}"
    );
}
