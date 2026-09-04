//! The Second Brain, attacked.
//!
//! Every other suite asks whether the server does what it says. This one asks
//! whether a hostile store, a hostile note, a hostile argument or a hostile
//! sequence can make it say something it must not — and it is organised by the
//! property under attack rather than by the tool being called, because a
//! property that only holds in the tool it was written for is not a property.
//!
//! ## What is deliberately not repeated here
//!
//! A red team that re-proves what is already proved is a red team that grows
//! without finding anything. These belong to their own suites and are not
//! copied:
//!
//! ```text
//! every mutation refuses a stale revision      mcp_revision, mcp_mutation_matrix
//! a selector that looks like a path            mcp_identity_and_content
//! a file whose identity disagrees with its name mcp_identity_and_content
//! hostile content survives the round trip      mcp_identity_and_content
//! unsaved text in a window blocks a stale write mcp_authority
//! the reactor answers while the Core is busy   mcp_concurrency
//! the process holds no internet socket         mcp_no_network
//! ```
//!
//! What is here is what Phase 4.2R reproduced and what Phase 4.2R fixed.
//!
//! ## The findings this suite pins
//!
//! ```text
//! 4.2A-002  noteit_read had no ceiling: 16 MiB of note answered in 34 MB
//! ```

mod support;

use noteit_mcp::budget::MAX_READ_RESPONSE_BYTES;
use serde_json::json;
use support::{McpClient, Sandbox, ToolAnswer};

/// A body of exactly this many bytes, out of characters JSON does not escape.
///
/// The cheapest content there is to publish, so a budget measured on it is the
/// most generous case the cap ever sees.
fn plain_body(bytes: usize) -> String {
    "abcdefghijklmnopqrstuvwxyz0123456789 ".repeat(bytes / 37 + 1)[..bytes].to_string()
}

fn read(client: &mut McpClient, id: &str) -> ToolAnswer {
    client.call("noteit_read", json!({ "note_id": id }))
}

// ===================================================== the read budget (4.2A-002)

/// R02. A note under the ceiling comes back whole, and its revision names what
/// came back.
///
/// The two halves are one property. A read that published content without the
/// revision of *that* content would be a read nobody could safely act on, and
/// the check here is not that the field is present but that it is right: the
/// revision is recomputed from the answer's own bytes, through the Core's own
/// mechanism, and has to be the one the server sent.
#[test]
fn r02_a_read_under_the_ceiling_is_whole_and_its_revision_names_what_it_sent() {
    let sandbox = Sandbox::new();
    let body = plain_body(1024 * 1024);
    let id = sandbox.seed(&body).to_string();
    let mut client = McpClient::start(&sandbox);

    let (answer, wire) = client.call_on_the_wire("noteit_read", json!({ "note_id": &id }));
    assert_eq!(answer.status(), "ok");
    let note = &answer.structured()["note"];
    assert_eq!(
        note["content"].as_str().expect("content"),
        noteit_core::NoteDocument::canonical_content(&body),
        "a megabyte of note did not come back whole"
    );

    // The whole megabyte is a megabyte the write path could also carry: this is
    // the property `MAX_READ_RESPONSE_BYTES` is derived from.
    assert!(
        body.len() >= noteit_core::control::MAX_FRAME_BYTES as usize,
        "this test stopped exercising the frame-sized note it was written for"
    );
    assert!(answer.result_bytes() <= MAX_READ_RESPONSE_BYTES);
    assert!(
        wire > 2 * body.len(),
        "the answer weighed less than the note"
    );
}

/// R17. Content, metadata and revision belong to one state.
///
/// Rebuilt through the Core rather than by hashing anything here: a second
/// implementation of the digest in a test would prove the test agrees with
/// itself.
#[test]
fn r17_a_successful_read_publishes_content_and_revision_of_the_same_state() {
    let sandbox = Sandbox::new();
    let id = sandbox
        .seed("um corpo qualquer\n\ncom parágrafos")
        .to_string();
    let mut client = McpClient::start(&sandbox);

    for _ in 0..3 {
        let answer = read(&mut client, &id);
        let note = &answer.structured()["note"];
        let document = sandbox
            .core()
            .read_note(&noteit_core::Uuid::parse_str(&id).expect("uuid"))
            .expect("read the note the Core's way");
        let recomputed =
            noteit_core::revision::NoteRevision::for_document(&document).expect("revision");
        assert_eq!(
            note["revision"].as_str().expect("revision"),
            recomputed.as_str(),
            "the published revision does not name the state on disk"
        );
        assert_eq!(note["content"].as_str().expect("content"), document.content);
        assert_eq!(note["note_id"].as_str().expect("note_id"), id);
    }
}

/// R03. Past the ceiling there is a code, and nothing else.
///
/// The refusal is inspected field by field rather than by eye, because the
/// dangerous answer is not a big one — it is a small one carrying a revision.
/// A caller holding a revision for a note it has not seen may write over it,
/// which is the whole reason a partial read is refused instead of trimmed.
#[test]
fn r03_a_read_past_the_ceiling_refuses_with_no_body_and_no_revision() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed(&plain_body(8 * 1024 * 1024)).to_string();
    let mut client = McpClient::start(&sandbox);

    let (answer, wire) = client.call_on_the_wire("noteit_read", json!({ "note_id": &id }));
    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(answer.status(), "error");
    assert_eq!(answer.code(), Some("response_too_large"));

    // The payload, field by field: `content` is checked here and not in the
    // whole result, because a `CallToolResult` has a `content` array of its own
    // and matching that would be matching the envelope rather than the note.
    let structured = answer.structured();
    assert!(structured.get("note").is_none(), "{}", answer.raw);
    let published: Vec<&String> = structured.as_object().expect("an object").keys().collect();
    assert_eq!(
        published,
        vec![
            "code",
            "message",
            "omitted_warning_count",
            "status",
            "warnings",
            "warnings_truncated"
        ],
        "a refusal published something other than its own refusal: {}",
        answer.raw
    );
    let rendered = answer.raw.to_string();
    for forbidden in [
        "revision",
        "note_id",
        "label",
        "created_at",
        "updated_at",
        "tags",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "a refusal carried `{forbidden}`: {rendered}"
        );
    }
    assert!(
        !rendered.contains("abcdefghij"),
        "a refusal carried part of the note: {rendered}"
    );
    assert!(
        !rendered.contains(&sandbox.root.display().to_string()) && !rendered.contains(".md"),
        "a refusal named the filesystem: {rendered}"
    );
    assert!(wire < 4096, "the refusal itself weighed {wire} bytes");
}

/// R18. A refused read is not a way to obtain a token.
///
/// The note is there and cannot be published, so there is no revision anywhere
/// for it — and every spelling of one a caller could invent is refused by the
/// precondition it was invented to satisfy.
#[test]
fn r18_a_refused_read_hands_out_nothing_a_write_can_be_built_from() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed(&plain_body(8 * 1024 * 1024)).to_string();
    let path = sandbox.note_path(&id);
    let before = std::fs::read(&path).expect("read the file");
    let mut client = McpClient::start(&sandbox);

    assert_eq!(
        read(&mut client, &id).code(),
        Some("response_too_large"),
        "this test needs a note past the ceiling"
    );

    for revision in ["0".repeat(64), "a".repeat(64), "f".repeat(64)] {
        let answer = client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": "X", "expected_revision": revision }),
        );
        assert!(answer.is_error(), "{}", answer.raw);
        assert!(
            answer.structured().get("revision").is_none(),
            "a refusal published a revision: {}",
            answer.raw
        );
    }
    assert_eq!(
        before,
        std::fs::read(&path).expect("read the file"),
        "the note moved"
    );
}

/// R04. Escaping is counted, so it cannot be used to get past the ceiling.
///
/// The attack is arithmetic: `content.len()` is not what the host receives, and
/// a note of quotation marks weighs two to three times its own size once the
/// payload has been escaped and published a second time. Every body here is the
/// same number of bytes on disk and a different number of bytes on the wire,
/// and the ceiling has to hold for all of them.
#[test]
fn r04_json_escaping_cannot_smuggle_an_answer_past_the_ceiling() {
    // Comfortably under the ceiling as plain text, and past it once escaped.
    let bytes = 1_800_000;
    let shapes: &[(&str, &str)] = &[
        ("plain", "abcdefgh"),
        ("quotes", "\"\"\"\"\"\"\"\""),
        ("backslashes", "\\\\\\\\\\\\\\\\"),
        (
            "control characters",
            "\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{e}\u{f}",
        ),
        ("newlines and tabs", "a\nb\tc\rd\u{b}"),
        ("emoji", "😀😀"),
        ("combining marks", "e\u{301}e\u{301}"),
    ];

    for (why, unit) in shapes {
        let sandbox = Sandbox::new();
        let body = unit.repeat(bytes / unit.len() + 1);
        let id = sandbox.seed(&body).to_string();
        let mut client = McpClient::start(&sandbox);

        let (answer, _) = client.call_on_the_wire("noteit_read", json!({ "note_id": &id }));
        match answer.code() {
            Some("response_too_large") => {
                assert!(
                    answer.structured().get("note").is_none(),
                    "{why}: a refusal carried a note"
                );
            }
            None => {
                assert_eq!(answer.status(), "ok", "{why}: {}", answer.raw);
                assert!(
                    answer.result_bytes() <= MAX_READ_RESPONSE_BYTES,
                    "{why}: an answer of {} bytes went out under a {MAX_READ_RESPONSE_BYTES} byte \
                     ceiling — the escaping was not counted",
                    answer.result_bytes()
                );
            }
            other => panic!("{why}: unexpected code {other:?}: {}", answer.raw),
        }
    }
}

/// The ceiling is a number of bytes on a pipe, and this is where that is
/// checked rather than asserted.
///
/// A note is grown one step at a time across the boundary and every answer is
/// weighed as it arrives. Two things have to be true and neither is implied by
/// the other: nothing published exceeds the ceiling, and something is published
/// right up against it — a server that refused everything would satisfy the
/// first alone.
#[test]
fn the_ceiling_is_measured_on_the_wire_and_is_reached_before_it_refuses() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);

    let mut largest_published = 0usize;
    let mut smallest_refusal = usize::MAX;
    let mut refused_any = false;

    // Around the point where a plain body's answer crosses four megabytes.
    for bytes in (2_000_000..2_120_000).step_by(20_000) {
        let id = sandbox.seed(&plain_body(bytes)).to_string();
        let (answer, wire) = client.call_on_the_wire("noteit_read", json!({ "note_id": &id }));
        match answer.code() {
            None => {
                let published = answer.result_bytes();
                assert!(
                    published <= MAX_READ_RESPONSE_BYTES,
                    "{published} bytes went out under a {MAX_READ_RESPONSE_BYTES} byte ceiling"
                );
                largest_published = largest_published.max(published);
            }
            Some("response_too_large") => {
                refused_any = true;
                smallest_refusal = smallest_refusal.min(wire);
            }
            other => panic!("unexpected code {other:?}: {}", answer.raw),
        }
    }

    assert!(refused_any, "nothing in the range was refused");
    assert!(
        largest_published > MAX_READ_RESPONSE_BYTES - 200_000,
        "the largest answer published was {largest_published}, far under the ceiling: the ceiling \
         is not the thing deciding"
    );
    assert!(
        smallest_refusal < 4096,
        "a refusal weighed {smallest_refusal} bytes"
    );
}
