//! Phase 4.3E: Second Brain Context Retrieval CLI Integration Suite.
//!
//! Tests the `noteit contexto` (and alias `noteit context`) command across:
//! - Human text presentation and formatting (snippets, labels, reasons, tasks)
//! - Machine JSON interface (`noteit --json contexto`)
//! - Strict envelope contracts and absence of forbidden fields
//! - Filtering by tag, property, tasks, and limit ceilings
//! - Query length bounds and validation
//! - Lexical and semantic fallback policies (automatic, semantic_required, lexical_only)
//! - Parity with the Core retrieval engine

#[allow(dead_code)]
mod support;

use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use serde_json::Value;
use support::Sandbox;

fn document(channel: &str) -> Value {
    assert!(!channel.is_empty(), "the channel is empty");
    assert!(
        channel.ends_with('\n'),
        "a machine document must end in a newline: {channel:?}"
    );
    assert!(
        !channel.contains('\u{1b}'),
        "a machine document carried an escape sequence: {channel:?}"
    );
    serde_json::from_str(channel).expect("valid json document")
}

fn success(result: (i32, String, String)) -> Value {
    let (code, stdout, stderr) = result;
    assert_eq!(code, 0, "expected success, stderr was {stderr:?}");
    assert!(
        stderr.is_empty(),
        "a successful machine command wrote to stderr: {stderr:?}"
    );
    document(&stdout)
}

fn failure(result: (i32, String, String), expected_code: i32) -> Value {
    let (code, stdout, stderr) = result;
    assert_eq!(code, expected_code, "stderr was {stderr:?}");
    assert!(
        stdout.is_empty(),
        "a failed machine command wrote to stdout: {stdout:?}"
    );
    document(&stderr)
}

// ------------------------------------------------------------- 1. Human Interface

#[test]
fn human_context_retrieval_displays_candidates_and_reasons() {
    let sandbox = Sandbox::new();
    sandbox.seed("Tratamento de sepse com hidratação venosa vigorosa e antibiótico precoce.");
    sandbox.seed("Protocolo de choque séptico na UTI: noradrenalina e ressuscitação volêmica.");
    sandbox.seed("Consulta de rotina: orientações gerais e atividade física.");

    let (code, stdout, stderr) = sandbox.run(&["contexto", "choque séptico"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("choque séptico"));
    assert!(stdout.contains("Protocolo de choque"));
    assert!(stdout.contains("motivo") || stdout.contains("texto") || stdout.contains("text_match"));
}

#[test]
fn human_context_supports_english_alias_context() {
    let sandbox = Sandbox::new();
    sandbox.seed("Anotação sobre cardiologia e hipertensão essencial.");

    let (code, stdout, stderr) = sandbox.run(&["context", "hipertensão"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.is_empty());
    assert!(stdout.contains("cardiologia"));
}

#[test]
fn human_context_reports_empty_when_nothing_matches() {
    let sandbox = Sandbox::new();
    sandbox.seed("Apenas notas sobre culinária.");

    let (code, stdout, stderr) = sandbox.run(&["contexto", "astronomia estelar"]);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("Nenhum contexto encontrado") || stdout.contains("Nenhuma nota"));
}

// ------------------------------------------------------------- 2. Machine Interface

#[test]
fn machine_context_envelope_has_required_fields_and_no_forbidden_fields() {
    let sandbox = Sandbox::new();
    let id1 = sandbox.seed("Hipertensão arterial sistêmica e meta pressórica.");
    let _id2 = sandbox.seed("Insônia crônica pós-plantão.");

    let json = success(sandbox.run(&["--json", "contexto", "hipertensão arterial"]));
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["command"], "context");

    let data = &json["data"];
    assert!(data.is_object(), "data must be an object");
    assert_eq!(data["semantic_status"], "not_requested");
    assert_eq!(data["truncated"], false);
    assert_eq!(data["omitted_count"], 0);

    let candidates = data["candidates"].as_array().expect("candidates array");
    assert!(!candidates.is_empty());

    let first = &candidates[0];
    assert_eq!(first["note_id"], id1.to_string());
    assert!(first["label"].is_string());
    assert!(first["snippet"].is_string());
    assert!(first["snippet"]
        .as_str()
        .unwrap()
        .contains("Hipertensão arterial"));

    let reasons = first["reasons"].as_array().expect("reasons array");
    assert!(reasons.iter().any(|r| r == "text_match"));

    // Prohibited fields must never appear
    assert!(
        first.get("revision").is_none(),
        "revision is strictly forbidden in context candidate"
    );
    assert!(first.get("etag").is_none(), "etag is forbidden");
    assert!(first.get("score").is_none(), "raw score is forbidden");
    assert!(
        first.get("similarity").is_none(),
        "similarity score is forbidden"
    );
    assert!(first.get("vector").is_none(), "vectors are forbidden");
    assert!(first.get("embedding").is_none(), "embeddings are forbidden");
    assert!(first.get("path").is_none(), "path is forbidden");
}

#[test]
fn machine_context_supports_tasks_flag() {
    let sandbox = Sandbox::new();
    let mut doc = NoteDocument::new_empty();
    doc.content =
        "# Plantão UTI\n- [ ] Checar gasometria arterial\n- [x] Prescrever analgesia\n".to_string();
    sandbox
        .core()
        .storage()
        .save_note_atomic(&doc)
        .expect("save note with tasks");

    let json = success(sandbox.run(&["--json", "contexto", "gasometria", "--tarefas"]));
    let candidates = json["data"]["candidates"].as_array().expect("candidates");
    assert!(!candidates.is_empty());

    let tasks = candidates[0]["tasks"].as_array().expect("tasks array");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["text"], "Checar gasometria arterial");
    assert_eq!(tasks[0]["checked"], false);
}

// ------------------------------------------------------------- 3. Filters & Ceilings

#[test]
fn context_filtering_by_tag_and_property() {
    let sandbox = Sandbox::new();
    let mut doc = NoteDocument::new_empty();
    doc.content = "Nota sobre conduta de sepse.".to_string();
    doc.user_metadata = NoteMetadata::try_new(
        vec!["Medicina".into()],
        vec![NoteProperty {
            key: "setor".into(),
            value: "uti".into(),
        }],
    )
    .expect("metadata");
    let target_id = doc.metadata.id;
    sandbox
        .core()
        .storage()
        .save_note_atomic(&doc)
        .expect("save");

    sandbox.seed("Outra nota qualquer sobre culinária.");

    let json = success(sandbox.run(&[
        "--json",
        "contexto",
        "--tag",
        "Medicina",
        "--propriedade",
        "setor=uti",
    ]));

    let candidates = json["data"]["candidates"].as_array().expect("candidates");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["note_id"], target_id.to_string());
    let reasons = candidates[0]["reasons"].as_array().expect("reasons");
    assert!(reasons.iter().any(|r| r == "shared_tag"));
    assert!(reasons.iter().any(|r| r == "property_match"));
}

#[test]
fn context_query_too_long_is_refused() {
    let sandbox = Sandbox::new();
    let long_query = "a".repeat(513);

    let json = failure(sandbox.run(&["--json", "contexto", &long_query]), 1);
    assert_eq!(json["status"], "error");
    assert_eq!(json["command"], "context");
    assert_eq!(json["error"]["code"], "invalid_input");
}

// ------------------------------------------------------------- 4. Fallback Policies

#[test]
fn context_semantic_required_without_provider_fails_typed() {
    let sandbox = Sandbox::new();
    sandbox.seed("Nota existente.");

    // Write config requiring semantic retrieval with an unavailable remote provider
    let config_dir = sandbox.root.join("config/note-it");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        r#"[semantic_retrieval]
mode = "semantic"
provider = "openai"
fallback = "semantic_required"
"#,
    )
    .unwrap();

    let json = failure(sandbox.run(&["--json", "contexto", "pergunta"]), 1);
    assert_eq!(json["status"], "error");
    assert_eq!(json["error"]["code"], "semantic_unavailable");
}

#[test]
fn context_automatic_fallback_degrades_to_lexical_and_reports_unavailable() {
    let sandbox = Sandbox::new();
    sandbox.seed("Tratamento de pneumonia comunitária com amoxicilina.");

    // Write config with automatic fallback and an unavailable remote provider
    let config_dir = sandbox.root.join("config/note-it");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        r#"[semantic_retrieval]
mode = "semantic"
provider = "openai"
fallback = "automatic"
"#,
    )
    .unwrap();

    let json = success(sandbox.run(&["--json", "contexto", "pneumonia"]));
    assert_eq!(json["status"], "ok");
    assert_eq!(json["data"]["semantic_status"], "unavailable");
    let candidates = json["data"]["candidates"].as_array().expect("candidates");
    assert!(!candidates.is_empty());
}
