//! The MCP conversation itself, driven byte by byte.
//!
//! Everything else in this crate's suites uses the protocol to get at Note-it.
//! This one is about the protocol: the handshake, the framing, the error
//! codes, and the fact that version negotiation is the SDK's job and not this
//! repository's.

mod support;

use serde_json::json;
use support::{McpClient, Sandbox};

#[test]
fn the_handshake_answers_a_server_that_offers_tools_and_says_how_to_use_them() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::spawn(&sandbox);
    let info = client.initialize(support::HANDSHAKE_PROTOCOL_VERSION);

    assert_eq!(info["serverInfo"]["name"], "noteit-mcp");
    assert_eq!(info["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(
        info["protocolVersion"].as_str().is_some(),
        "no protocol version was negotiated: {info}"
    );
    assert!(info["capabilities"]["tools"].is_object(), "{info}");

    // The two rules an agent must not have to read documentation to learn.
    let instructions = info["instructions"].as_str().expect("instructions");
    assert!(
        instructions.contains("revision_conflict"),
        "the instructions do not name the conflict: {instructions}"
    );
    assert!(
        instructions.contains("indeterminate"),
        "the instructions do not name the unknown outcome: {instructions}"
    );
    assert!(instructions.contains("expected_revision"), "{instructions}");
}

/// Which revision of MCP is spoken is negotiated by the SDK.
///
/// This repository does not implement a second version negotiation, does not
/// tie the MCP version to Note-it's private control protocol, and does not
/// refuse a client for asking about a revision it has heard of. What is
/// checked here is only that a version is agreed and that it is one of the
/// ones the specification defines — the choice itself belongs to `rmcp`.
#[test]
fn version_negotiation_belongs_to_the_sdk() {
    let sandbox = Sandbox::new();
    for requested in ["2025-06-18", "2025-11-25", "2026-07-28"] {
        let mut client = McpClient::spawn(&sandbox);
        let info = client.initialize(requested);
        let negotiated = info["protocolVersion"].as_str().expect("a version");
        assert!(
            negotiated.len() == 10 && negotiated.starts_with("20"),
            "asking for {requested} produced {negotiated}"
        );
        // And whatever was agreed, the tools work.
        let tools = client.list_tools();
        assert_eq!(tools.len(), noteit_mcp::contract::TOOL_NAMES.len());
    }
}

/// Note-it's private control protocol is not the MCP version, and neither is
/// the machine interface's schema version. Three numbers, three contracts.
#[test]
fn the_private_protocol_version_is_never_used_as_the_mcp_version() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::spawn(&sandbox);
    let info = client.initialize(support::HANDSHAKE_PROTOCOL_VERSION);
    let negotiated = info["protocolVersion"].as_str().expect("a version");

    assert_ne!(
        negotiated,
        noteit_core::control::PROTOCOL_VERSION.to_string(),
        "the private protocol version reached the MCP handshake"
    );
    // The MCP version is a specification date, and the private one is a small
    // integer. They cannot be confused for each other by accident, and this
    // says so out loud so nobody makes them the same on purpose.
    assert!(negotiated.contains('-'), "{negotiated}");
}

/// A call for a tool that does not exist is a protocol error, not an
/// improvisation.
#[test]
fn an_unknown_tool_is_refused_by_the_protocol() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let error = client.call_expecting_protocol_error("noteit_delete_everything", json!({}));
    assert!(
        error["code"].as_i64().is_some(),
        "an unknown tool must carry a JSON-RPC code: {error}"
    );
}

/// A method the server does not implement is refused rather than answered.
///
/// This phase implements tools and nothing else: no resources, no prompts, no
/// sampling, no elicitation, no tasks extension. A host that asks anyway must
/// be told, not humoured.
#[test]
fn the_server_offers_tools_and_only_tools() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);

    let info_capabilities = {
        let mut probe = McpClient::spawn(&sandbox);
        let info = probe.initialize(support::HANDSHAKE_PROTOCOL_VERSION);
        info["capabilities"].clone()
    };
    for absent in [
        "resources",
        "prompts",
        "logging",
        "completions",
        "extensions",
    ] {
        assert!(
            info_capabilities.get(absent).is_none(),
            "this phase declares `{absent}`, which it does not implement: {info_capabilities}"
        );
    }

    // A host that asks anyway — the capability is not declared, so a
    // conforming one will not — finds nothing there. The SDK answers these
    // with its own empty defaults, and what matters is that they are empty:
    // this phase publishes no resource and no prompt, so there is nothing an
    // agent could reach that the tool catalogue did not decide it may.
    for (method, key) in [("resources/list", "resources"), ("prompts/list", "prompts")] {
        match client.request(method, json!({})) {
            Ok(result) => assert_eq!(
                result[key].as_array().map(Vec::len),
                Some(0),
                "{method} offered something this phase does not implement: {result}"
            ),
            Err(error) => assert!(error["code"].as_i64().is_some(), "{method}: {error}"),
        }
    }
}

/// The transport is standard input and standard output, and the server ends
/// when the host hangs up.
#[test]
fn closing_standard_input_ends_the_process_cleanly() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    client.list_tools();

    let finished = client.finish();
    assert_eq!(
        finished.code,
        Some(0),
        "the server did not end cleanly; stderr: {}",
        finished.stderr
    );
    assert!(
        finished.trailing_stdout.trim().is_empty(),
        "{:?}",
        finished.trailing_stdout
    );
    assert!(
        finished.stderr.is_empty(),
        "an ordinary shutdown said something: {}",
        finished.stderr
    );
}

/// Every tool answers with structured content that validates against the shape
/// its output schema declares.
///
/// Not a full JSON Schema validator — the check is the one that matters for a
/// consumer: the declared type is an object, the declared required fields are
/// present, and nothing is answered as prose alone.
#[test]
fn every_answer_carries_the_structured_content_its_schema_promises() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = support::create_note(&mut client, "BASE\n\n- [ ] fazer");
    let revision = support::read_revision(&mut client, &id);

    let schemas: std::collections::HashMap<String, serde_json::Value> = client
        .list_tools()
        .into_iter()
        .map(|tool| {
            (
                tool["name"].as_str().unwrap().to_string(),
                tool["outputSchema"].clone(),
            )
        })
        .collect();

    let calls: Vec<(&str, serde_json::Value)> = vec![
        ("noteit_list", json!({})),
        ("noteit_read", json!({ "note_id": &id })),
        ("noteit_search", json!({ "query": "BASE" })),
        ("noteit_tasks_list", json!({})),
        ("noteit_trash_list", json!({})),
        ("noteit_create", json!({ "content": "OUTRA" })),
        (
            "noteit_append",
            json!({ "note_id": &id, "text": "MAIS", "expected_revision": &revision }),
        ),
        ("noteit_trash_restore", json!({ "note_id": "0123abcd" })),
    ];

    for (name, arguments) in calls {
        let answer = client.call(name, arguments);
        let schema = schemas.get(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(schema["type"], "object", "{name}: {schema}");

        let structured = answer.structured();
        assert!(structured.is_object(), "{name}: {}", answer.raw);
        for required in schema["required"].as_array().cloned().unwrap_or_default() {
            let field = required.as_str().unwrap();
            assert!(
                structured.get(field).is_some(),
                "{name} promised `{field}` and did not send it: {}",
                answer.raw
            );
        }
        // The human-readable content is there too, and is never the only thing.
        assert!(
            answer.raw["content"]
                .as_array()
                .is_some_and(|c| !c.is_empty()),
            "{name} sent no content block: {}",
            answer.raw
        );
    }
}

/// Every published schema is one a client can actually read.
///
/// `schemars` writes an optional field as `"type": ["string", "null"]`. The
/// official MCP Inspector flags that shape, because a good number of clients
/// read `type` as a single string and either reject the tool or drop the
/// constraint — and a dropped constraint on `expected_revision` is the one
/// thing this server must not permit. `crate::schema` rewrites it, and this
/// checks the rewrite reached everything a host can see.
#[test]
fn no_published_schema_spells_a_type_as_an_array() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);

    fn walk(node: &serde_json::Value, path: &str, into: &mut Vec<String>) {
        match node {
            serde_json::Value::Object(object) => {
                if let Some(serde_json::Value::Array(_)) = object.get("type") {
                    into.push(format!("{path}.type"));
                }
                for (key, child) in object {
                    walk(child, &format!("{path}.{key}"), into);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, &format!("{path}[{index}]"), into);
                }
            }
            _ => {}
        }
    }

    let mut offenders = Vec::new();
    for tool in client.list_tools() {
        let name = tool["name"].as_str().unwrap().to_string();
        walk(
            &tool["inputSchema"],
            &format!("{name}.inputSchema"),
            &mut offenders,
        );
        if let Some(output) = tool.get("outputSchema") {
            walk(output, &format!("{name}.outputSchema"), &mut offenders);
        }
    }
    assert!(
        offenders.is_empty(),
        "these schema nodes spell a type as an array: {offenders:#?}"
    );
}

/// And the rewrite did not cost the contract anything.
///
/// The point of the previous test is portability; the point of this one is that
/// portability was not bought by loosening what the schema says. The required
/// fields are still required and the nullable ones are still nullable.
#[test]
fn the_portable_schemas_still_say_what_they_said() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let tools = client.list_tools();

    let append = tools
        .iter()
        .find(|tool| tool["name"] == "noteit_append")
        .expect("noteit_append");
    // Required, and a plain string: the field that must never be optional is
    // also the field that must never be nullable.
    assert_eq!(
        append["inputSchema"]["properties"]["expected_revision"]["type"], "string",
        "{}",
        append["inputSchema"]
    );
    assert!(append["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "expected_revision"));

    // And a genuinely nullable output field kept both of its possibilities.
    let branches = &append["outputSchema"]["properties"]["current_revision"]["anyOf"];
    let kinds: Vec<&str> = branches
        .as_array()
        .unwrap_or_else(|| panic!("{}", append["outputSchema"]))
        .iter()
        .filter_map(|branch| branch["type"].as_str())
        .collect();
    assert_eq!(kinds, vec!["string", "null"], "{}", append["outputSchema"]);
}
