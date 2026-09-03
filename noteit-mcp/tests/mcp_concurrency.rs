//! The protocol has to keep answering while the disk is busy.
//!
//! `noteit-mcp` runs a *current-thread* Tokio runtime, and every tool is a
//! Core call that reads or writes files. If those calls run on the runtime's
//! own thread, one slow store operation stops the whole server: no `ping`, no
//! second request, no cancellation — the host waits, and it cannot even find
//! out whether the server is alive.
//!
//! That is what this suite exists to refuse. It does not check that the source
//! contains `spawn_blocking`; a grep would do that and would prove nothing
//! about behaviour. It puts a call in flight, holds it inside the Core
//! operation, and asks the server something else.
//!
//! ## Why there is no `sleep` here
//!
//! A test that sleeps three seconds and hopes the answer arrives first is a
//! test about this machine's load. Both proofs below are about *order*:
//!
//! - the write proof is fully deterministic. A fake authority opens a gate the
//!   instant it has the request — so the server is provably inside the
//!   blocking operation — and does not answer until the test opens a second
//!   gate. The `ping` is sent between the two, so a reactor that could not
//!   answer it would not answer it *later*: it would still be stuck.
//! - the read proof needs no authority and covers the path that has none. It
//!   asks which answer reaches the host first. A blocked reactor cannot
//!   reorder anything, so the search would have to answer before the ping.
//!
//! And every read is bounded by `ANSWER_TIMEOUT`, so a server that stops
//! answering fails these tests with a sentence instead of hanging the run.

mod support;

use serde_json::json;
use std::time::Duration;
use support::{AuthorityBehaviour, FakeAuthority, Gate, McpClient, Sandbox};

/// How long the test waits for the authority to say it has the request.
const ARRIVAL: Duration = Duration::from_secs(30);

#[test]
fn a_ping_is_answered_while_a_write_is_held_inside_the_core() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();

    let arrived = Gate::new();
    let release = Gate::new();
    let _authority = FakeAuthority::start(
        &sandbox,
        AuthorityBehaviour::CommitWhenReleased {
            arrived: arrived.clone(),
            release: release.clone(),
        },
    );

    let mut client = McpClient::start(&sandbox);
    let revision = read_revision(&mut client, &id);

    // In flight, and deliberately not waited on.
    let append = client.send_request(
        "tools/call",
        json!({
            "name": "noteit_append",
            "arguments": { "note_id": &id, "text": "SEGUNDA LINHA", "expected_revision": &revision },
        }),
    );

    // The server is now inside the Core operation: the authority has the
    // request and is holding it. Nothing about this is a duration.
    assert!(
        arrived.wait_for(ARRIVAL),
        "the write never reached the authority, so this test never got to ask its question"
    );

    // The question. If Core I/O ran on the reactor, this line would still be
    // sitting in the pipe unread.
    let ping = client.send_request("ping", json!({}));
    let (first, answer) = client.next_response();
    assert_eq!(
        first, ping,
        "the first answer back was not the ping: the reactor is blocked behind the write"
    );
    answer.expect("ping must be answered while a handler is busy");

    // Only now may the write finish.
    release.open();
    let result = client
        .await_response(append)
        .expect("the held write must still complete");
    assert_eq!(
        result["structuredContent"]["status"], "ok",
        "the write that waited must still commit: {result}"
    );
    assert_eq!(
        sandbox.body(&id),
        "BASE\nSEGUNDA LINHA",
        "the write committed something other than what it was given"
    );
}

#[test]
fn a_ping_overtakes_a_read_that_is_scanning_the_store() {
    // The read path has no authority to gate, so this proof is about which
    // answer arrives first. The store is large enough that scanning it is
    // orders of magnitude slower than answering a ping, and a reactor that
    // could not interleave would answer strictly in order.
    let sandbox = Sandbox::new();
    let filler = "conteúdo de preenchimento para dar trabalho à varredura. ".repeat(400);
    for index in 0..300 {
        sandbox.seed(&format!("nota {index}\n\n{filler}"));
    }

    let mut client = McpClient::start(&sandbox);

    let search = client.send_request(
        "tools/call",
        json!({ "name": "noteit_search", "arguments": { "query": "AGULHA-QUE-NAO-EXISTE" } }),
    );
    let ping = client.send_request("ping", json!({}));

    let (first, answer) = client.next_response();
    assert_eq!(
        first, ping,
        "the search answered before the ping: the reactor is blocked while the store is scanned"
    );
    answer.expect("ping must be answered while a search is running");

    let result = client
        .await_response(search)
        .expect("the search must still answer");
    assert_eq!(
        result["structuredContent"]["count"], 0,
        "the search found something it should not have: {result}"
    );
}

fn read_revision(client: &mut McpClient, id: &str) -> String {
    let answer = client.call("noteit_read", json!({ "note_id": id }));
    answer.raw["structuredContent"]["note"]["revision"]
        .as_str()
        .expect("the read must carry a revision")
        .to_string()
}
