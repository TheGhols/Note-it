//! §4.2R.R1: what a client sends cannot decide what a refusal weighs.
//!
//! Phase 4.2R closed every sentence the *domain* writes: a `message` became a
//! `&'static str` chosen by a `code`, so a runtime-built string could not
//! reach the wire (ADR-054). It closed nothing one layer earlier. Arguments
//! are deserialised into a tool's input type **before** any handler in this
//! crate runs, and the SDK's extractor answered a failure with
//! `serde_json`'s own sentence — which quotes the value it did not understand,
//! in full.
//!
//! Measured against the baseline, on a real process over real pipes:
//!
//! ```text
//! tool               field          sent                    answered
//! noteit_list        limit          300 KiB string          307 361 bytes
//! noteit_tasks_list  state          300 KiB variant         307 387 bytes
//! noteit_context     include_tasks  300 KiB string          307 367 bytes
//! noteit_edit        clear          300 KiB string          307 367 bytes
//! noteit_list        tags           300 KiB string          307 368 bytes
//! (JSON-RPC)         method         300 KiB name            307 261 bytes
//! ```
//!
//! Every one carried the canary back. The last is the same class one layer up
//! again: the SDK's default `on_custom_request` answers an unroutable request
//! with the method name the client chose.
//!
//! ## The property this suite pins
//!
//! > No text derived from a client's arguments reaches the wire, and the size
//! > of a refusal is not something a client can move.
//!
//! The second half is why the tests below assert **equality** and not only a
//! ceiling. A bound of "under a kilobyte" would be satisfied by a refusal that
//! echoed the first five hundred bytes of the argument, which is still a
//! channel and still amplification. A refusal that is byte-for-byte the same
//! whether the argument was one kilobyte or one megabyte has no channel in it
//! at all.
//!
//! ## Why the refusal is a JSON-RPC error and not a tool result
//!
//! MCP classifies invalid arguments as a protocol error: the request's shape
//! was wrong, so it was never a call. The SDK routes it to the tool-result
//! channel instead, but only by sniffing its own message for the literal
//! prefix `failed to deserialize parameters:` — which this server no longer
//! produces, deliberately. The answer is `-32602`, it carries no
//! `structuredContent`, and that is the more consistent contract: a tool
//! result always carries the structured shape `docs/mcp.md` documents, and a
//! request that never reached a tool is not a tool result.
//!
//! See `noteit-mcp/src/params.rs`, `scripts/check-mcp-boundary` and ADR-055.

mod support;

use noteit_mcp::contract::TOOL_NAMES;
use noteit_mcp::params::INVALID_ARGUMENTS;
use serde_json::{json, Value};
use support::{create_note, fingerprint, read_revision, McpClient, Sandbox};

/// A string nothing else in this repository says, so finding it in an answer
/// means it came from the request and from nowhere else.
const CANARY: &str = "CANARIO-ARG-4E2R-R1-8H";

/// The size the phase attacks with: three hundred kibibytes, big enough for
/// amplification to be unmistakable and small enough to stay a test.
const HOSTILE_BYTES: usize = 300 * 1024;

/// The most a refusal may weigh on the wire, envelope included.
///
/// Generous on purpose — the measurements below come in at 112 and 113 bytes,
/// and the point of the number is to be a ceiling nothing derived from an
/// argument could ever fit under, not to pin the exact byte count. The
/// equality tests do the pinning.
const REFUSAL_CEILING_BYTES: usize = 512;

/// A hostile payload of `bytes` with the canary at both ends.
///
/// At both ends so that a refusal which echoed either the head or the tail of
/// an argument is caught; a canary only at the front would be missed by a
/// server that quoted the last kilobyte.
fn hostile(bytes: usize) -> String {
    let filling = "Z".repeat(bytes - 2 * CANARY.len());
    format!("{CANARY}{filling}{CANARY}")
}

/// Every shape of wrong argument this server publishes a field for.
///
/// One row per *kind* of type mismatch rather than one per field, because the
/// property is about the deserialiser and not about any one tool: a number
/// given a string, an enum given an unknown variant, a boolean given a string,
/// a list given a scalar, and a list of objects given a list of scalars.
///
/// `note_id` and `expected_revision` are filled in with values that are
/// themselves valid, so that the row under test is the only thing wrong with
/// the request.
fn wrong_shapes(note_id: &str, revision: &str, payload: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("noteit_list", json!({ "limit": payload })),
        (
            "noteit_search",
            json!({ "query": "agulha", "limit": payload }),
        ),
        ("noteit_tasks_list", json!({ "limit": payload })),
        (
            "noteit_context",
            json!({ "query": "agulha", "limit": payload }),
        ),
        ("noteit_tasks_list", json!({ "state": payload })),
        (
            "noteit_context",
            json!({ "query": "agulha", "include_tasks": payload }),
        ),
        (
            "noteit_edit",
            json!({
                "note_id": note_id,
                "clear": payload,
                "expected_revision": revision,
            }),
        ),
        ("noteit_list", json!({ "tags": payload })),
        ("noteit_create", json!({ "content": "x", "tags": payload })),
        ("noteit_list", json!({ "properties": payload })),
        (
            "noteit_create",
            json!({ "content": "x", "properties": [payload] }),
        ),
        ("noteit_read", json!({ "note_id": 12_345 })),
        ("noteit_list", json!({ "limit": [payload] })),
        (
            "noteit_trash_restore",
            json!({ "note_id": { "buried": payload } }),
        ),
    ]
}

/// A sandbox with one note in it, and the two valid values a mutation needs.
fn seeded() -> (Sandbox, McpClient, String, String) {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let note_id = create_note(&mut client, "nota de base com agulha");
    let revision = read_revision(&mut client, &note_id);
    (sandbox, client, note_id, revision)
}

// ============================================ the argument boundary (4.2R-004)

/// R1-01. Three hundred kilobytes of wrong argument answer in a hundred bytes,
/// and none of them are the client's.
///
/// The measurement is the one the operating system moved: the whole JSON-RPC
/// line, counted off the pipe, not a length recomputed from a parsed value.
#[test]
fn r1_01_a_hostile_argument_is_refused_without_being_repeated() {
    let (sandbox, mut client, note_id, revision) = seeded();
    let before = fingerprint(&sandbox.root);
    let payload = hostile(HOSTILE_BYTES);

    for (tool, arguments) in wrong_shapes(&note_id, &revision, &payload) {
        let sent = serde_json::to_string(&arguments).expect("serialise").len();
        let (wire, refusal) = client.call_refused_by_the_argument_boundary(tool, arguments);
        let rendered = refusal.to_string();

        assert!(
            !rendered.contains(CANARY),
            "{tool} published the canary in {wire} bytes: {rendered}"
        );
        assert!(
            !rendered.contains("ZZZZZZZZ"),
            "{tool} published the filling: {rendered}"
        );
        assert!(
            wire <= REFUSAL_CEILING_BYTES,
            "{tool} answered {wire} bytes to a {sent} byte request"
        );
        assert_eq!(refusal["message"], json!(INVALID_ARGUMENTS), "{tool}");
        assert!(
            refusal.get("data").is_none(),
            "{tool} attached a data member: {rendered}"
        );
    }

    // A request that was refused before the tool body ran cannot have reached
    // the store, and this is the proof rather than the reasoning.
    assert_eq!(
        before,
        fingerprint(&sandbox.root),
        "a refused request changed the sandbox"
    );

    // And the process is still there, answering.
    let after = client.call("noteit_list", json!({}));
    assert_eq!(after.status(), "ok", "{}", after.raw);
}

/// R1-02. The refusal does not grow with the argument. At all.
///
/// The strongest form of the property: same tool, same field, four sizes
/// spanning three orders of magnitude, and one byte count. A refusal that
/// echoed any part of the argument — a prefix, a length, a truncation — could
/// not produce the same number four times.
#[test]
fn r1_02_the_size_of_a_refusal_is_not_the_clients_to_choose() {
    let (_sandbox, mut client, _note_id, _revision) = seeded();

    let mut measured: Vec<(usize, usize)> = Vec::new();
    for bytes in [1024, 64 * 1024, HOSTILE_BYTES, 1024 * 1024] {
        let (wire, _) = client.call_refused_by_the_argument_boundary(
            "noteit_list",
            json!({ "limit": hostile(bytes) }),
        );
        measured.push((bytes, wire));
    }

    let (_, first) = measured[0];
    for (sent, wire) in &measured {
        assert_eq!(
            *wire, first,
            "a {sent} byte argument answered in {wire} bytes and a 1024 byte one in {first}: \
             the refusal carries something of the argument's"
        );
    }
}

/// R1-03. A small wrong argument and a huge one are answered identically.
///
/// The other direction of the same property. If a refusal said anything at
/// all about what was wrong — the field, the expected type, the length — these
/// two answers would differ, because these two requests differ in every one of
/// those. They are the same bytes.
#[test]
fn r1_03_a_small_mistake_and_a_large_one_are_told_apart_by_nothing() {
    let (_sandbox, mut client, _note_id, _revision) = seeded();

    let (small_wire, small) =
        client.call_refused_by_the_argument_boundary("noteit_list", json!({ "limit": "10" }));
    let (large_wire, large) = client.call_refused_by_the_argument_boundary(
        "noteit_tasks_list",
        json!({ "state": hostile(HOSTILE_BYTES) }),
    );

    assert_eq!(small["message"], large["message"]);
    assert_eq!(small["code"], large["code"]);
    assert_eq!(
        small_wire, large_wire,
        "a two-character mistake answered in {small_wire} bytes and a 300 KiB one in {large_wire}"
    );
}

/// R1-04. An argument of the right type is still the tool's to refuse.
///
/// The boundary must not have swallowed the domain. A three-hundred-kilobyte
/// `note_id` *is* a string, so it deserialises, reaches the handler, and comes
/// back as this server's own structured refusal — with the constant sentence
/// Phase 4.2R put there and nothing of the selector in it.
#[test]
fn r1_04_a_well_typed_argument_still_reaches_the_tool_that_refuses_it() {
    let (_sandbox, mut client, _note_id, _revision) = seeded();
    let payload = hostile(HOSTILE_BYTES);

    let (answer, wire) = client.call_on_the_wire("noteit_read", json!({ "note_id": &payload }));
    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(answer.status(), "error", "{}", answer.raw);
    assert!(
        answer.structured().get("code").is_some(),
        "a well-typed argument must be refused by the tool, with a code: {}",
        answer.raw
    );
    assert!(
        !answer.raw.to_string().contains(CANARY),
        "the selector came back: {}",
        answer.raw
    );
    assert!(
        wire <= 1024,
        "a well-typed hostile selector answered {wire} bytes"
    );
}

// ================================================ the method boundary (4.2R.R1)

/// R1-05. A method this server does not have is refused without being named.
///
/// One layer above the arguments: the SDK's default answer to an unroutable
/// request is the method string the client sent, at the length the client
/// chose. `tools/call` with an `arguments` member that is not an object lands
/// here too — the SDK could not see it as a tool call at all.
#[test]
fn r1_05_an_unroutable_request_does_not_echo_what_it_was_called() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let payload = hostile(HOSTILE_BYTES);

    for (label, method, params) in [
        ("a method of 300 KiB", payload.clone(), json!({})),
        (
            "a tools/call whose arguments are not an object",
            "tools/call".to_string(),
            json!({ "name": "noteit_list", "arguments": payload.clone() }),
        ),
    ] {
        let id = client.send_request(&method, params);
        let (wire, answer) = client.await_response_on_the_wire(id);
        let error = answer.expect_err(&format!("{label} must be refused"));
        assert!(
            !error.to_string().contains(CANARY),
            "{label} echoed the request: {error}"
        );
        assert!(
            wire <= REFUSAL_CEILING_BYTES,
            "{label} answered {wire} bytes"
        );
        assert_eq!(error["code"].as_i64(), Some(-32601), "{label}: {error}");
    }

    // A name that does not route is refused by the router with its own fixed
    // sentence, which was never the problem and must not become one.
    let error = client
        .call_expecting_protocol_error("noteit_delete_everything", json!({ "victim": &payload }));
    assert!(
        !error.to_string().contains(CANARY),
        "an unknown tool echoed its arguments: {error}"
    );
}

// ============================================== nothing else moved (§11)

/// R1-06. The catalogue is the same catalogue, and every schema still says
/// what it said.
///
/// Swapping the extractor changes what a *refusal* says. A host reading
/// `tools/list` must not be able to tell that anything happened, so the
/// published schema is checked here on the wire as well as against the
/// generator in `noteit_mcp::params`'s own tests.
#[test]
fn r1_06_the_sixteen_tools_and_their_schemas_are_unchanged() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let tools = client.list_tools();

    assert_eq!(tools.len(), 16, "the catalogue changed size");
    let mut published: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();
    published.sort_unstable();
    let mut expected: Vec<&str> = TOOL_NAMES.to_vec();
    expected.sort_unstable();
    assert_eq!(published, expected, "the catalogue changed contents");

    // The fields a wrapper could have eaten: one per shape the boundary
    // touches, checked as the type the schema declares.
    for (tool, field, kind) in [
        ("noteit_list", "limit", "integer"),
        ("noteit_list", "tags", "array"),
        ("noteit_list", "properties", "array"),
        ("noteit_tasks_list", "state", "string"),
        ("noteit_context", "include_tasks", "boolean"),
        ("noteit_context", "query", "string"),
        ("noteit_edit", "clear", "boolean"),
        ("noteit_edit", "expected_revision", "string"),
        ("noteit_read", "note_id", "string"),
    ] {
        let published = tools
            .iter()
            .find(|published| published["name"] == tool)
            .unwrap_or_else(|| panic!("{tool} is not published"));
        let schema = &published["inputSchema"]["properties"][field];
        assert!(
            !schema.is_null(),
            "{tool} no longer publishes `{field}`: {published}"
        );
        assert_eq!(
            declared_type(schema, &published["inputSchema"]).as_deref(),
            Some(kind),
            "{tool}.{field} is published as something other than {kind}: {schema}"
        );
    }

    // And every mutation still declares its precondition required, which is
    // the guarantee the whole boundary exists for.
    for tool in tools.iter().filter(|tool| {
        tool["inputSchema"]["properties"]
            .get("expected_revision")
            .is_some()
    }) {
        let required: Vec<&str> = tool["inputSchema"]["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(
            required.contains(&"expected_revision"),
            "{} publishes expected_revision without requiring it",
            tool["name"]
        );
    }
}

/// R1-07. Valid calls answer exactly as before, and writing still needs a
/// revision that names a state the caller knows.
///
/// The authorisation behaviour is the thing this phase must not have moved.
/// The whole chain is walked: no revision refuses, a stale revision conflicts,
/// the revision from a read commits, and the revision that write returned
/// chains into the next one.
#[test]
fn r1_07_valid_work_and_the_revision_rules_are_untouched() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let note_id = create_note(&mut client, "conteúdo inicial\n\n- [ ] uma tarefa\n");

    for (tool, arguments) in [
        ("noteit_list", json!({})),
        ("noteit_search", json!({ "query": "inicial" })),
        ("noteit_tasks_list", json!({})),
        ("noteit_context", json!({ "query": "inicial" })),
        ("noteit_trash_list", json!({})),
        ("noteit_read", json!({ "note_id": &note_id })),
    ] {
        let answer = client.call(tool, arguments);
        assert_eq!(answer.status(), "ok", "{tool}: {}", answer.raw);
        assert!(
            answer.raw.get("structuredContent").is_some(),
            "{tool} answered without structured content: {}",
            answer.raw
        );
    }

    let before = sandbox.note_bytes(&note_id);
    client.call_refused_by_the_argument_boundary(
        "noteit_append",
        json!({ "note_id": &note_id, "text": "SEM PRECONDIÇÃO" }),
    );
    assert_eq!(before, sandbox.note_bytes(&note_id));

    let revision = read_revision(&mut client, &note_id);
    let stale = "0".repeat(64);
    let conflicted = client.call(
        "noteit_append",
        json!({ "note_id": &note_id, "text": "OBSOLETA", "expected_revision": &stale }),
    );
    assert_eq!(
        conflicted.code(),
        Some("revision_conflict"),
        "{}",
        conflicted.raw
    );
    assert!(
        conflicted.structured().get("current_revision").is_none(),
        "a conflict published a revision the caller had not read: {}",
        conflicted.raw
    );
    assert_eq!(before, sandbox.note_bytes(&note_id));

    let written = client.call(
        "noteit_append",
        json!({ "note_id": &note_id, "text": "COM PRECONDIÇÃO", "expected_revision": &revision }),
    );
    assert_eq!(written.status(), "ok", "{}", written.raw);
    assert!(sandbox.body(&note_id).contains("COM PRECONDIÇÃO"));

    // The revision a successful write returned chains into the next one.
    let chained = client.call(
        "noteit_append",
        json!({
            "note_id": &note_id,
            "text": "ENCADEADA",
            "expected_revision": written.revision(),
        }),
    );
    assert_eq!(chained.status(), "ok", "{}", chained.raw);
    assert!(sandbox.body(&note_id).contains("ENCADEADA"));
}

// ==================================================== load, and what it costs

/// R1-08. Several large hostile requests at once stay bounded, and the
/// protocol keeps answering while they arrive.
///
/// Registered as measurement rather than as a guarantee. What is asserted is
/// what the phase can honestly assert: every answer is a small refusal, the
/// server answers a `ping` sent after them, and the process's peak resident
/// size does not run away. It is not a scheduler proof and does not pretend to
/// be one — `mcp_concurrency.rs` owns the ordering guarantees.
#[test]
fn r1_08_a_burst_of_hostile_arguments_is_answered_without_the_process_growing() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let payload = hostile(HOSTILE_BYTES);

    let before = peak_resident_kib(client.pid());

    // Sent without waiting, so they are in flight together rather than one
    // after another.
    let mut pending = Vec::new();
    for _ in 0..8 {
        pending.push(client.send_request(
            "tools/call",
            json!({
                "name": "noteit_list",
                "arguments": { "limit": &payload },
            }),
        ));
    }
    let ping = client.send_request("ping", json!({}));

    let mut refusals = 0;
    for id in pending {
        let (wire, answer) = client.await_response_on_the_wire(id);
        let error = answer.expect_err("a hostile argument must be refused");
        assert_eq!(error["message"], json!(INVALID_ARGUMENTS));
        assert!(wire <= REFUSAL_CEILING_BYTES, "a refusal weighed {wire}");
        refusals += 1;
    }
    assert_eq!(refusals, 8);
    client
        .await_response(ping)
        .expect("the server must still answer ping");

    let after = peak_resident_kib(client.pid());
    // Eight requests of 300 KiB each are 2.4 MiB of input. The number here is
    // deliberately loose: what would fail it is the process keeping what it
    // was sent, which is the failure mode worth catching.
    assert!(
        after - before < 32 * 1024,
        "peak resident size went from {before} KiB to {after} KiB over eight 300 KiB refusals"
    );
}

/// The type a published property declares, through the three spellings this
/// server's schemas actually use.
///
/// A plain `type`; an `anyOf` for a field that may be absent, which keeps the
/// type inside its branches; and a `$ref` into `$defs` for a named type such
/// as `TaskState`. All three predate this phase — they are `schemars`' doing,
/// carried through `crate::schema::portable` — and following them is what lets
/// the check be about the field rather than about the spelling.
fn declared_type(property: &Value, input_schema: &Value) -> Option<String> {
    if let Some(kind) = property["type"].as_str() {
        return Some(kind.to_string());
    }
    if let Some(branches) = property["anyOf"].as_array() {
        return branches
            .iter()
            .find_map(|branch| declared_type(branch, input_schema));
    }
    let reference = property["$ref"].as_str()?;
    let name = reference.strip_prefix("#/$defs/")?;
    declared_type(&input_schema["$defs"][name], input_schema)
}

/// The process's peak resident set size, straight from the kernel.
///
/// `/proc/<pid>/status`, which needs no dependency and no sampling thread.
fn peak_resident_kib(pid: u32) -> usize {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))
        .unwrap_or_else(|error| panic!("this suite needs procfs: {error}"));
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("no VmHWM in /proc/{pid}/status"))
}
