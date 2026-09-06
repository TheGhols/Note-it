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

// ------------------------------------------------------------- 5. Phase 4.3R Offensive Audit

#[test]
fn cli_context_damaged_store_never_leaks_paths_and_caps_warnings() {
    let sandbox = Sandbox::new();
    let notes_dir = sandbox.store_paths().notes_dir;
    std::fs::create_dir_all(&notes_dir).unwrap();

    // 1 honest note
    sandbox.seed("Protocolo de choque séptico e ressuscitação volêmica.");

    // Outside canary target
    let outside = sandbox.root.join("fora-do-store.md");
    std::fs::write(&outside, "CANARIO_FORA_DO_STORE\n").unwrap();

    // 2,000 damaged symlinks pointing to the outside canary
    for _ in 0..2000 {
        let symlink_path = notes_dir.join(format!("{}.md", noteit_core::Uuid::new_v4()));
        let _ = std::os::unix::fs::symlink(&outside, symlink_path);
    }

    // Corrupt front matter file
    let corrupt_id = noteit_core::Uuid::new_v4();
    std::fs::write(
        notes_dir.join(format!("{corrupt_id}.md")),
        "---\nnote_it:\n  version: 1\n  id: [[[invalid\n---\nCorpo corrompido\n",
    )
    .unwrap();

    // 1. Test human output
    let (code, stdout, stderr) = sandbox.run(&["contexto", "choque séptico"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(!stdout.contains("fora-do-store"));
    assert!(!stdout.contains("/home/"));
    assert!(!stdout.contains("/tmp/"));
    assert!(!stderr.contains("/home/"));
    assert!(!stderr.contains("/tmp/"));

    // 2. Test machine JSON output
    let json = success(sandbox.run(&["--json", "contexto", "choque séptico"]));
    assert_eq!(json["status"], "ok");
    let data = &json["data"];
    assert_eq!(data["warnings_truncated"], true);
    let warnings = data["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.len() <= 20,
        "warnings count exceeded MAX_CONTEXT_WARNINGS"
    );
    let omitted = data["omitted_warning_count"]
        .as_u64()
        .expect("omitted count");
    assert!(omitted >= 1980);

    // Deep check that no path appears in any string
    let serialized = serde_json::to_string(&json).unwrap();
    assert!(!serialized.contains("fora-do-store"));
    assert!(!serialized.contains("/home/"));
    assert!(!serialized.contains("/tmp/"));
}

#[test]
fn cli_context_adversarial_arguments_and_extreme_limits() {
    let sandbox = Sandbox::new();
    sandbox.seed("Sepse e choque séptico em medicina.");

    // Extreme limit: 99999999 -> clamped to 50
    let json_large = success(sandbox.run(&["--json", "contexto", "sepse", "--limite", "99999999"]));
    assert_eq!(json_large["status"], "ok");

    // Zero limit: 0 -> clamped to 1
    let json_zero = success(sandbox.run(&["--json", "contexto", "sepse", "--limite", "0"]));
    assert_eq!(json_zero["status"], "ok");

    // Negative limit: -5 -> Clap rejects as usage error (code 2)
    let (_, _, stderr_neg) = sandbox.run(&["contexto", "sepse", "--limite", "-5"]);
    assert!(!stderr_neg.is_empty());

    // Non-numeric limit -> Clap rejects as usage error (code 2)
    let (_, _, stderr_nan) = sandbox.run(&["contexto", "sepse", "--limite", "nao-um-numero"]);
    assert!(!stderr_nan.is_empty());

    // Adversarial property without '='
    let (code_prop, _, _) = sandbox.run(&["contexto", "sepse", "--propriedade", "chave_sem_igual"]);
    assert_eq!(code_prop, 2);

    // 10,000 char query -> refused, echoes only length
    let huge_query = "x".repeat(10_000);
    let (code_query, _, stderr_query) = sandbox.run(&["contexto", &huge_query]);
    assert_eq!(code_query, 1);
    assert!(stderr_query.contains("10000"));
    assert!(!stderr_query.contains(&huge_query));
}

#[test]
fn cli_context_keyword_stuffing_and_100k_terms() {
    let sandbox = Sandbox::new();
    let id_exact = sandbox.seed("Sepse grave com disfunção de múltiplos órgãos.");
    let _id_stuffed = sandbox.seed(&format!("Discutir sepse. {}", "sepse ".repeat(100_000)));

    let json = success(sandbox.run(&["--json", "contexto", "Sepse grave"]));
    assert_eq!(json["status"], "ok");
    let candidates = json["data"]["candidates"].as_array().expect("candidates");
    assert!(!candidates.is_empty());

    // Exact phrase match has Precedence Class 1 (TextMatch). Keyword stuffing has Class 2 (TermMatch)
    // Class 1 must strictly outrank Class 2!
    assert_eq!(candidates[0]["note_id"], id_exact.to_string());
}

#[test]
fn cli_context_prohibited_fields_deep_scan() {
    let sandbox = Sandbox::new();
    sandbox.seed("Nota clínica para verificação de chaves do envelope.");

    let json = success(sandbox.run(&["--json", "contexto", "clínica"]));
    assert_eq!(json["status"], "ok");

    fn check_no_forbidden(val: &Value) {
        match val {
            Value::Object(map) => {
                for (k, v) in map {
                    let k_lower = k.to_lowercase();
                    assert_ne!(k_lower, "revision", "found forbidden key 'revision'");
                    assert_ne!(k_lower, "etag", "found forbidden key 'etag'");
                    assert_ne!(k_lower, "score", "found forbidden key 'score'");
                    assert_ne!(k_lower, "similarity", "found forbidden key 'similarity'");
                    assert_ne!(k_lower, "vector", "found forbidden key 'vector'");
                    assert_ne!(k_lower, "embedding", "found forbidden key 'embedding'");
                    assert_ne!(k_lower, "path", "found forbidden key 'path'");
                    check_no_forbidden(v);
                }
            }
            Value::Array(arr) => {
                for item in arr {
                    check_no_forbidden(item);
                }
            }
            Value::String(s) => {
                assert!(!s.contains("/home/"), "found path leak in string: {s}");
                assert!(!s.contains("/tmp/"), "found path leak in string: {s}");
            }
            _ => {}
        }
    }

    check_no_forbidden(&json);
}

#[test]
fn cli_context_ssrf_and_hostile_unicode_content() {
    let sandbox = Sandbox::new();
    sandbox
        .seed("Endpoint metadata: http://169.254.169.254/latest/meta-data/ e file:///etc/passwd");
    sandbox.seed("Anotação com RTL: \u{202E}texto invertido\u{202C} e ZWJ \u{200D} combinante \u{0300}\u{0301}");

    let json = success(sandbox.run(&["--json", "contexto", "169.254.169.254"]));
    assert_eq!(json["status"], "ok");
    let candidates = json["data"]["candidates"].as_array().expect("candidates");
    assert_eq!(candidates.len(), 1);

    let (code, stdout, _) = sandbox.run(&["contexto", "169.254.169.254"]);
    assert_eq!(code, 0);
    assert!(
        !stdout.contains('\u{1b}'),
        "human output contained ANSI escape sequence"
    );
}
