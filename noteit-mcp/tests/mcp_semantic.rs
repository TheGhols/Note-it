//! The semantic channel over the wire, in the two states that matter.
//!
//! `semantic_lifecycle.rs` drives the adapter directly, which is where the
//! lifecycle can be inspected. This suite drives the **shipped binary** through
//! a pipe, because two of the claims are about the process rather than about a
//! function:
//!
//! * a machine on the factory default runs a server that never reads an
//!   artifact, never allocates a model and never opens a file outside its
//!   store — whatever is sitting in its cache directory;
//! * the reactor keeps answering while an index is being built, which is the
//!   4.2R property repeated under the load 4.3C adds.

mod support;

use serde_json::json;
use std::time::{Duration, Instant};
use support::{McpClient, Sandbox, ANSWER_TIMEOUT};

fn context(client: &mut McpClient, query: &str) -> serde_json::Value {
    client
        .call("noteit_context", json!({ "query": query }))
        .structured()
        .clone()
}

// =========================================================== factory default

#[test]
fn the_shipped_default_answers_without_a_model_and_says_so() {
    let sandbox = Sandbox::new();
    sandbox.seed("hipertensão arterial e restrição de sal");
    sandbox.seed("insônia depois do plantão noturno");
    let mut client = McpClient::start(&sandbox);

    let answer = context(&mut client, "hipertensão");
    assert_eq!(answer["status"], "ok");
    assert_eq!(
        answer["semantic_status"], "not_requested",
        "a default install must not have attempted the semantic channel"
    );
    assert!(
        !answer["candidates"]
            .as_array()
            .expect("candidates")
            .is_empty(),
        "the lexical answer disappeared"
    );

    // Not one candidate may claim a channel that was never run.
    for candidate in answer["candidates"].as_array().expect("candidates") {
        let reasons = candidate["reasons"].as_array().expect("reasons");
        assert!(
            !reasons.iter().any(|reason| reason == "semantic_match"),
            "a semantic_match arrived from a server with no provider"
        );
    }
}

#[test]
fn the_shipped_default_opens_no_artifact_even_when_one_is_lying_there() {
    let sandbox = Sandbox::new();
    // Something that looks exactly like a provisioned artifact, in exactly the
    // place the provider would look. The default must still not read it.
    let artifact = sandbox
        .root
        .join("cache/note-it/embedding/potion-multilingual-128M")
        .join("73908c3438cf03b6a01bcb9611d62b23d0726f08");
    std::fs::create_dir_all(&artifact).expect("artifact directory");
    std::fs::write(artifact.join("model.safetensors"), vec![0u8; 4096]).expect("weights");
    std::fs::write(artifact.join("tokenizer.json"), b"{}").expect("tokenizer");

    sandbox.seed("hipertensão arterial");
    let mut client = McpClient::start(&sandbox);

    let answer = context(&mut client, "hipertensão");
    assert_eq!(answer["status"], "ok");
    assert_eq!(answer["semantic_status"], "not_requested");

    // A file the server had opened and not closed would still be listed; one it
    // read and closed would not. The stronger evidence is that a malformed
    // artifact did not become an error: had it been read, it would have.
    let descriptors = client.open_descriptors();
    assert!(
        !descriptors
            .iter()
            .any(|(_, target)| target.contains("model.safetensors")),
        "the default configuration is holding the artifact open: {descriptors:?}"
    );
    assert_eq!(
        answer["code"],
        serde_json::Value::Null,
        "a model nobody asked for produced an error"
    );
}

#[test]
fn a_configuration_from_before_this_phase_still_means_lexical() {
    let sandbox = Sandbox::new();
    let config_dir = sandbox.root.join("config/note-it");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    // Byte for byte the shape a 4.3B installation wrote.
    std::fs::write(
        config_dir.join("config.toml"),
        "default_color = \"yellow\"\n\
         default_font_size = 15\n\
         default_width = 360\n\
         default_height = 300\n\
         autosave_interval_ms = 300\n\
         theme = \"dark\"\n\
         ui_scale_percent = 130\n\
         capture_delimiter = \"blankLine\"\n",
    )
    .expect("config");

    sandbox.seed("hipertensão arterial");
    let mut client = McpClient::start(&sandbox);
    let answer = context(&mut client, "hipertensão");
    assert_eq!(answer["status"], "ok");
    assert_eq!(
        answer["semantic_status"], "not_requested",
        "an upgrade turned the semantic channel on"
    );
}

#[test]
fn asking_for_semantics_without_a_model_still_answers_lexically() {
    let sandbox = Sandbox::new();
    let config_dir = sandbox.root.join("config/note-it");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(
        config_dir.join("config.toml"),
        "[semantic_retrieval]\nmode = \"semantic\"\n",
    )
    .expect("config");

    let note = sandbox
        .seed("hipertensão arterial e restrição de sal")
        .to_string();
    let mut client = McpClient::start(&sandbox);

    let answer = context(&mut client, "hipertensão");
    assert_eq!(answer["status"], "ok", "a missing model broke retrieval");
    assert_eq!(
        answer["semantic_status"], "unavailable",
        "the answer degraded and did not say so"
    );
    let returned: Vec<String> = answer["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .map(|candidate| candidate["note_id"].as_str().expect("id").to_string())
        .collect();
    assert!(
        returned.contains(&note),
        "the lexical result disappeared because the semantic half failed"
    );
}

#[test]
fn semantic_required_without_a_model_refuses_and_carries_no_answer() {
    let sandbox = Sandbox::new();
    let config_dir = sandbox.root.join("config/note-it");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(
        config_dir.join("config.toml"),
        "[semantic_retrieval]\nmode = \"semantic\"\nfallback = \"semantic_required\"\n",
    )
    .expect("config");

    sandbox.seed("hipertensão arterial");
    let mut client = McpClient::start(&sandbox);
    let answer = client.call("noteit_context", json!({ "query": "hipertensão" }));
    assert!(answer.is_error());
    assert_eq!(answer.code(), Some("semantic_unavailable"));
    assert!(
        answer
            .structured()
            .get("candidates")
            .and_then(|candidates| candidates.as_array())
            .map(|candidates| candidates.is_empty())
            .unwrap_or(true),
        "a refusal carried half an answer"
    );

    // And the message is Note-it's own: no path, no library sentence, no digest.
    let message = serde_json::to_string(answer.structured()).expect("serialise");
    for leak in ["safetensors", "tokenizer", ".cache", "/home/", "sha256"] {
        assert!(
            !message.contains(leak),
            "the refusal leaked `{leak}`: {message}"
        );
    }

    // A request with no question is still answered, because it asked the
    // semantic channel for nothing.
    let empty = context(&mut client, "");
    assert_eq!(empty["status"], "ok");
    assert_eq!(empty["semantic_status"], "not_requested");
}

// ================================================================= privacy

/// An embedding is derived from a private note, and derived is not "not
/// private".
///
/// The note here carries a string that occurs nowhere else on the machine, so
/// finding it anywhere outside the note file is unambiguous. What is checked is
/// every surface 4.3C could plausibly have added one: the server's standard
/// error, the names of any files it created, and everything under the cache
/// directory where a model would live.
#[test]
fn note_content_reaches_no_surface_the_semantic_channel_added() {
    const MARKER: &str = "zqxjvkbrwmpfhg-marcador-privado-8817";

    let sandbox = Sandbox::new();
    let config_dir = sandbox.root.join("config/note-it");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(
        config_dir.join("config.toml"),
        "[semantic_retrieval]\nmode = \"semantic\"\n",
    )
    .expect("config");
    sandbox.seed(&format!("consulta clínica sobre {MARKER} e pressão alta"));
    sandbox.seed("insônia depois do plantão noturno");

    let mut client = McpClient::start(&sandbox);
    // Ask about it several ways, including with the marker itself as the
    // question, which is the case most likely to echo it somewhere.
    for query in ["hipertensão", MARKER, "pressão alta", ""] {
        let answer = context(&mut client, query);
        assert_eq!(answer["status"], "ok", "query {query:?} failed");
    }
    let finished = client.finish();

    assert!(
        !finished.stderr.contains(MARKER),
        "note content reached standard error:\n{}",
        finished.stderr
    );

    // Nothing outside the notes directory may contain it — not the cache, not
    // the state directory, not a temporary file left behind.
    let notes_dir = sandbox.store_paths().notes_dir;
    let mut checked = 0usize;
    for entry in walk(&sandbox.root) {
        assert!(
            !entry.to_string_lossy().contains(MARKER),
            "a file name carries note content: {}",
            entry.display()
        );
        if entry.starts_with(&notes_dir) {
            continue;
        }
        checked += 1;
        let bytes = std::fs::read(&entry).unwrap_or_default();
        assert!(
            !String::from_utf8_lossy(&bytes).contains(MARKER),
            "note content was written outside the notes directory: {}",
            entry.display()
        );
    }
    assert!(
        checked > 0,
        "the walk found nothing outside the notes directory, so it proved nothing"
    );
}

fn walk(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(path),
                Ok(kind) if kind.is_file() => found.push(path),
                _ => {}
            }
        }
    }
    found
}

// ========================================================== responsiveness

/// The 4.2R hostile-load proof, repeated with an index being built.
///
/// Four clients, eight requests each, payloads near 300 KiB, and the semantic
/// channel switched on over a store large enough that indexing is real work.
/// The model is absent, which is the honest worst case for *this* property:
/// every one of those requests takes the full path through the session, the
/// lock, and the load attempt, and the reactor must keep answering `ping`
/// throughout.
#[test]
fn the_reactor_keeps_answering_under_hostile_load_with_the_channel_on() {
    let sandbox = Sandbox::new();
    let config_dir = sandbox.root.join("config/note-it");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(
        config_dir.join("config.toml"),
        "[semantic_retrieval]\nmode = \"semantic\"\n",
    )
    .expect("config");
    for index in 0..300 {
        sandbox.seed(&format!(
            "nota {index} sobre hipertensão arterial\n\ninsônia depois do plantão\n\nreunião de equipe"
        ));
    }

    let hostile = "á".repeat(150_000); // ~300 KiB of UTF-8 on the wire.
    let started = Instant::now();
    let outcomes: Vec<Duration> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let sandbox = &sandbox;
                let hostile = hostile.clone();
                scope.spawn(move || {
                    let mut client = McpClient::start(sandbox);
                    let mut worst = Duration::ZERO;
                    for _ in 0..8 {
                        // A request the server must chew on, in flight.
                        let heavy = client.send_request(
                            "tools/call",
                            json!({
                                "name": "noteit_context",
                                "arguments": { "query": hostile },
                            }),
                        );
                        // And a ping behind it. A reactor blocked inside the
                        // Core could not answer this one at all — not late,
                        // never.
                        let ping = Instant::now();
                        client.request("ping", json!({})).expect("ping answered");
                        worst = worst.max(ping.elapsed());
                        client
                            .await_response(heavy)
                            .expect("the heavy call answered");
                    }
                    worst
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("no client thread panicked"))
            .collect()
    });

    let worst = outcomes.into_iter().max().expect("four clients");
    assert!(
        worst < ANSWER_TIMEOUT,
        "a ping took {worst:?} while the semantic channel was working"
    );
    println!(
        "4 clients x 8 hostile requests with the semantic channel on: {:?} total, worst ping {worst:?}",
        started.elapsed()
    );
}
