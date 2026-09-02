//! The stable machine interface: `noteit --json`.
//!
//! Everything here runs the real binary and parses the real channels, because
//! the failures this contract exists to prevent are the ones a function-level
//! test cannot see: a warning that escaped to standard error, a paragraph of
//! Portuguese in front of a document, a second document after it.
//!
//! Nothing in this file matches a substring of a message. Every assertion goes
//! through a JSON parser and reads a typed field, which is exactly the demand
//! the contract makes of its consumers.

mod support;

use noteit_core::write::{WriteOutcome, WriteOutcomeKind};
use noteit_core::Uuid;
use serde::Deserialize;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use support::{prefix, AuthorityBehaviour, FakeAuthority, Sandbox};

/// Words that only ever appear in output meant for a person.
const HUMAN_MARKERS: [&str; 4] = ["Aviso:", "Erro:", "Nenhuma nota", "Nada foi alterado"];

/// Reads a channel that must hold exactly one JSON document and nothing else.
///
/// Deliberately strict about the whole channel rather than about a fragment of
/// it: text in front of the document, text after it, a second document, an
/// escape sequence or a missing final newline are each a broken contract, and
/// each of them is caught here.
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
    assert!(
        channel.starts_with('{'),
        "something was printed before the document: {channel:?}"
    );
    for marker in HUMAN_MARKERS {
        assert!(
            !channel.contains(marker),
            "human prose `{marker}` reached a machine channel: {channel:?}"
        );
    }

    let mut deserializer = serde_json::Deserializer::from_str(channel);
    let value = Value::deserialize(&mut deserializer)
        .unwrap_or_else(|error| panic!("the channel is not one JSON document: {error}\n{channel}"));
    deserializer
        .end()
        .unwrap_or_else(|error| panic!("something followed the document: {error}\n{channel}"));

    // The envelope's own shape, asserted on every single document this suite
    // ever reads. A field that quietly disappears breaks every test at once,
    // which is what an API regression should do.
    let object = value.as_object().expect("the document must be an object");
    for key in [
        "schema_version",
        "status",
        "command",
        "data",
        "error",
        "warnings",
    ] {
        assert!(
            object.contains_key(key),
            "the envelope lost `{key}`: {value}"
        );
    }
    assert_eq!(value["schema_version"], 1, "schema_version changed");
    assert!(value["status"].is_string());
    assert!(value["warnings"].is_array());

    value
}

/// A successful machine command: the document on standard output, nothing at
/// all on standard error.
fn success(result: (i32, String, String)) -> Value {
    let (code, stdout, stderr) = result;
    assert_eq!(code, 0, "expected success, stderr was {stderr:?}");
    assert!(
        stderr.is_empty(),
        "a successful machine command wrote to stderr: {stderr:?}"
    );
    document(&stdout)
}

/// A failed machine command: the document on standard error, nothing at all on
/// standard output.
fn failure(result: (i32, String, String), expected_code: i32) -> Value {
    let (code, stdout, stderr) = result;
    assert_eq!(code, expected_code, "stderr was {stderr:?}");
    assert!(
        stdout.is_empty(),
        "a failed machine command wrote to stdout: {stdout:?}"
    );
    document(&stderr)
}

fn note_id_of(value: &Value) -> String {
    value["data"]["write"]["note_id"]
        .as_str()
        .expect("a write answers with a note identifier")
        .to_string()
}

// --- the envelope -----------------------------------------------------------

#[test]
fn a_successful_command_is_one_document_on_stdout_and_silence_on_stderr() {
    let sandbox = Sandbox::new();
    let created = success(sandbox.run(&["--json", "criar", "# Choque distributivo"]));

    assert_eq!(created["status"], "ok");
    assert_eq!(created["command"], "create");
    assert_eq!(created["error"], Value::Null);
    assert_eq!(created["warnings"], Value::Array(vec![]));
    assert_eq!(created["data"]["write"]["kind"], "note_created");
    assert_eq!(created["data"]["write"]["changed"], Value::Bool(true));
    assert_eq!(created["data"]["write"]["commit_state"], "committed");
    assert_eq!(created["data"]["write"]["ui_sync"]["status"], "ok");
}

#[test]
fn an_execution_error_is_one_document_on_stderr_and_silence_on_stdout() {
    let sandbox = Sandbox::new();
    let refused = failure(sandbox.run(&["--json", "ler", "00000000"]), 1);
    assert_eq!(refused["status"], "error");
    assert_eq!(refused["command"], "read");
    assert_eq!(refused["error"]["code"], "not_found");
    assert_eq!(refused["data"], Value::Null);
}

#[test]
fn a_usage_error_is_one_document_on_stderr_with_the_usage_exit_code() {
    let sandbox = Sandbox::new();
    for arguments in [
        vec!["--json", "batata"],
        vec!["--json", "adicionar"],
        vec!["--json", "--flag-inexistente"],
        vec!["--json", "buscar"],
    ] {
        let refused = failure(sandbox.run(&arguments), 2);
        assert_eq!(refused["status"], "error", "{arguments:?}");
        assert_eq!(refused["error"]["code"], "usage_error", "{arguments:?}");
        assert!(
            refused["error"]["message"].is_string(),
            "{arguments:?}: a usage error must still say what was wrong"
        );
    }
}

#[test]
fn a_usage_error_this_cli_raises_itself_names_the_command_and_committed_nothing() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("valioso");
    let refused = failure(
        sandbox.run(&["--json", "editar", &prefix(id), "--vazio", "texto"]),
        2,
    );
    assert_eq!(refused["command"], "edit");
    assert_eq!(refused["error"]["code"], "usage_error");
    assert_eq!(refused["error"]["commit_state"], "not_committed");
    assert_eq!(sandbox.body(id), "valioso");
}

#[test]
fn a_hostile_argument_is_escaped_by_the_serialiser_rather_than_destroyed() {
    // JSON escaping is what makes a control character safe in a document, so
    // the argument the process really received is what comes back — and the
    // channel still parses, and still carries no live escape.
    let sandbox = Sandbox::new();
    let refused = failure(sandbox.run(&["--json", "\u{1b}[2Jbatata"]), 2);
    assert_eq!(refused["error"]["code"], "usage_error");
    let message = refused["error"]["message"].as_str().expect("message");
    assert!(
        message.contains('\u{1b}'),
        "the parsed value lost the argument it was given: {message:?}"
    );

    // The same argument through the human adapter is neutralised, because that
    // one really is going to a terminal.
    let (_, _, human) = sandbox.run(&["\u{1b}[2Jbatata"]);
    assert!(!human.contains('\u{1b}'), "{human:?}");
    assert!(human.contains("`batata`"), "{human:?}");
}

#[test]
fn a_read_warning_is_data_on_stdout_rather_than_a_sentence_on_stderr() {
    let sandbox = Sandbox::new();
    let good = sandbox.seed("# Nota legível");
    std::fs::write(
        sandbox.notes_dir().join(format!("{}.md", Uuid::new_v4())),
        b"---\nnote_it:\n  id: [isto nao e um id\n---\n\ncorpo\n",
    )
    .expect("plant a broken note");

    let listed = success(sandbox.run(&["--json", "listar"]));
    assert_eq!(listed["status"], "warning");
    assert!(
        !listed["warnings"].as_array().expect("warnings").is_empty(),
        "the broken note produced no warning: {listed}"
    );
    assert!(listed["warnings"][0]["code"].is_string());
    // The valid note is still in the result: a warning does not lose data.
    let ids: Vec<&str> = listed["data"]["notes"]
        .as_array()
        .expect("notes")
        .iter()
        .map(|note| note["note_id"].as_str().expect("id"))
        .collect();
    assert!(ids.contains(&good.to_string().as_str()), "{listed}");

    // And the same command without `--json` still says it in Portuguese.
    let (code, _, stderr) = sandbox.run(&["listar"]);
    assert_eq!(code, 0);
    assert!(stderr.contains("Aviso:"), "{stderr}");
}

// --- where the flag may go --------------------------------------------------

#[test]
fn the_flag_is_accepted_before_the_command_after_it_and_inside_a_group() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");

    let before = success(sandbox.run(&["--json", "listar"]));
    let after = success(sandbox.run(&["listar", "--json"]));
    assert_eq!(before, after);

    let grouped = success(sandbox.run(&["tags", "adicionar", &prefix(id), "Medicina", "--json"]));
    assert_eq!(grouped["command"], "tag_add");
    assert_eq!(grouped["data"]["write"]["commit_state"], "committed");

    // With a positional argument in front of it, and with two.
    let selector = prefix(id);
    let read_before = success(sandbox.run(&["--json", "ler", &selector]));
    let read_after = success(sandbox.run(&["ler", &selector, "--json"]));
    assert_eq!(read_before, read_after);

    let appended_before = success(sandbox.run(&["--json", "adicionar", &selector, "um"]));
    let appended_after = success(sandbox.run(&["adicionar", &selector, "dois", "--json"]));
    assert_eq!(appended_before["command"], appended_after["command"]);
    assert_eq!(
        appended_before["data"]["write"]["commit_state"],
        appended_after["data"]["write"]["commit_state"]
    );
    assert_eq!(sandbox.body(id), "corpo\num\ndois");
}

#[test]
fn the_envelope_uses_real_json_types_and_never_strings_for_them() {
    // The regressions this catches are the ones that look harmless in a diff:
    // a version that became "1", a flag that became "true", a count that
    // became "0".
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let written = success(sandbox.run(&["--json", "adicionar", &prefix(id), "mais"]));

    assert!(written["schema_version"].is_u64(), "{written}");
    assert!(
        written["data"]["write"]["changed"].is_boolean(),
        "{written}"
    );
    assert!(written["data"]["write"]["note_id"].is_string());
    assert!(written["warnings"].is_array());

    let listed = success(sandbox.run(&["--json", "listar"]));
    assert!(listed["data"]["count"].is_u64(), "{listed}");
    assert!(listed["data"]["notes"].is_array());
    assert!(listed["data"]["notes"][0]["tags"].is_array());
    assert!(listed["data"]["notes"][0]["properties"].is_array());

    let tasks = success(sandbox.run(&["--json", "tarefas"]));
    assert!(tasks["data"]["count"].is_u64());

    let status = success(sandbox.run(&["--json", "status"]));
    assert!(status["data"]["store_exists"].is_boolean());
    assert!(status["data"]["cli_ready"].is_boolean());
}

#[test]
fn a_literal_json_payload_after_the_escape_never_turns_the_mode_on() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("ABC");

    let (code, stdout, stderr) = sandbox.run(&["adicionar", &prefix(id), "--", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        !stdout.starts_with('{'),
        "a payload switched the machine interface on: {stdout}"
    );
    assert_eq!(sandbox.body(id), "ABC\n--json");
}

#[test]
fn every_alias_produces_the_same_document() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("- [ ] Revisar noradrenalina\n\ncorpo com texto");
    sandbox.run(&["tags", "adicionar", &prefix(id), "Medicina"]);
    sandbox.run(&["propriedades", "definir", &prefix(id), "fonte=Harrison"]);

    for (portuguese, english) in [
        (vec!["--json", "listar"], vec!["--json", "list"]),
        (
            vec!["--json", "buscar", "corpo"],
            vec!["--json", "search", "corpo"],
        ),
        (vec!["--json", "propriedades"], vec!["--json", "properties"]),
        (vec!["--json", "tarefas"], vec!["--json", "tasks"]),
        (vec!["--json", "lixeira"], vec!["--json", "trash"]),
    ] {
        let left = success(sandbox.run(&portuguese));
        let right = success(sandbox.run(&english));
        assert_eq!(left, right, "{portuguese:?} and {english:?} disagree");
    }

    let selector = prefix(id);
    let read_pt = success(sandbox.run(&["--json", "ler", &selector]));
    let read_en = success(sandbox.run(&["--json", "read", &selector]));
    assert_eq!(read_pt, read_en);

    // Writes change the store, so two runs cannot be identical documents — but
    // the contract they answer with must be.
    let created_pt = success(sandbox.run(&["--json", "criar", "a"]));
    let created_en = success(sandbox.run(&["--json", "create", "b"]));
    assert_eq!(created_pt["command"], created_en["command"]);
    assert_eq!(
        created_pt["data"]["write"]["kind"],
        created_en["data"]["write"]["kind"]
    );

    let appended_pt = success(sandbox.run(&["--json", "adicionar", &selector, "x"]));
    let appended_en = success(sandbox.run(&["--json", "append", &selector, "y"]));
    assert_eq!(appended_pt["command"], "append");
    assert_eq!(appended_en["command"], "append");

    let edited_pt = success(sandbox.run(&["--json", "editar", &selector, "novo"]));
    let edited_en = success(sandbox.run(&["--json", "edit", &selector, "outro"]));
    assert_eq!(edited_pt["command"], "edit");
    assert_eq!(edited_en["command"], "edit");
    assert_eq!(edited_pt["data"]["write"]["kind"], "content_replaced");
    assert_eq!(edited_en["data"]["write"]["kind"], "content_replaced");
}

// --- the informational commands ---------------------------------------------

#[test]
fn welcome_help_version_and_status_all_answer_in_json() {
    let sandbox = Sandbox::new();

    let welcome = success(sandbox.run(&["--json"]));
    assert_eq!(welcome["command"], "welcome");
    assert!(welcome["data"]["version"].is_string());
    assert_eq!(welcome["data"]["machine_interface"], Value::Bool(true));

    for arguments in [
        vec!["--json", "ajuda"],
        vec!["ajuda", "--json"],
        vec!["--json", "help"],
        vec!["--json", "--help"],
    ] {
        let help = success(sandbox.run(&arguments));
        assert_eq!(help["command"], "help", "{arguments:?}");
        let text = help["data"]["help"].as_str().expect("help text");
        assert!(text.contains("--json"), "the help must document the flag");
        assert!(!text.contains('\u{1b}'), "the help carried styling");
    }

    let version = success(sandbox.run(&["--json", "versao"]));
    assert_eq!(version["command"], "version");
    assert_eq!(
        version["data"]["version"],
        Value::String(env!("CARGO_PKG_VERSION").to_string())
    );

    let status = success(sandbox.run(&["--json", "status"]));
    assert_eq!(status["command"], "status");
    assert_eq!(status["data"]["cli_ready"], Value::Bool(true));
    assert_eq!(status["data"]["core_available"], Value::Bool(true));
    assert_eq!(status["data"]["store_exists"], Value::Bool(false));
    assert!(status["data"]["data_path"].is_string());
    // The private coordination state is not part of any public answer.
    let text = status.to_string();
    for forbidden in ["writer.lock", "control", "socket", "lease", "generation"] {
        assert!(!text.contains(forbidden), "`{forbidden}` leaked: {text}");
    }
}

// --- the reads --------------------------------------------------------------

#[test]
fn empty_collections_are_empty_arrays_and_a_zero_count() {
    let sandbox = Sandbox::new();
    for (arguments, key) in [
        (vec!["--json", "listar"], "notes"),
        (vec!["--json", "buscar", "nada"], "results"),
        (vec!["--json", "tags"], "tags"),
        (vec!["--json", "propriedades"], "properties"),
        (vec!["--json", "tarefas"], "tasks"),
        (vec!["--json", "lixeira"], "entries"),
    ] {
        let answer = success(sandbox.run(&arguments));
        assert_eq!(answer["status"], "ok", "{arguments:?}");
        assert_eq!(
            answer["data"][key],
            Value::Array(vec![]),
            "{arguments:?} did not answer with an empty array"
        );
        assert_eq!(answer["data"]["count"], 0, "{arguments:?}");
    }
}

#[test]
fn read_returns_the_note_exactly_as_the_core_holds_it() {
    let sandbox = Sandbox::new();
    // Quotes, a backslash, a newline, a tab, an emoji with a zero-width joiner
    // and a live escape sequence: everything a naive serialiser gets wrong.
    let content = "# Olá \"mundo\" 👨‍⚕️\ncaminho \\ teste\nlinha 2\ntab:\tfim\nesc:\u{1b}[2J";
    let id = sandbox.seed(content);
    sandbox.run(&["tags", "adicionar", &prefix(id), "Medicina"]);
    sandbox.run(&["propriedades", "definir", &prefix(id), "fonte=Harrison"]);

    let answer = success(sandbox.run(&["--json", "ler", &prefix(id)]));
    let note = &answer["data"]["note"];

    assert_eq!(note["note_id"], id.to_string());
    assert_eq!(
        note["content"].as_str().expect("content"),
        sandbox.body(id),
        "the machine interface did not return the note the Core holds"
    );
    assert_eq!(
        note["content"].as_str().expect("content"),
        content,
        "the terminal sanitizer was applied to data"
    );
    assert_eq!(note["tags"], serde_json::json!(["Medicina"]));
    assert_eq!(note["properties"][0]["key"], "fonte");
    assert_eq!(note["properties"][0]["value"], "Harrison");

    // The same note through the human adapter is deliberately not the same
    // text: that one is protecting a terminal.
    let (_, human, _) = sandbox.run(&["ler", &prefix(id)]);
    assert!(!human.contains('\u{1b}'), "the human view kept an escape");
}

#[test]
fn machine_timestamps_are_rfc3339_in_utc_while_the_human_view_stays_local() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("# Com data");

    let answer = success(sandbox.run(&["--json", "ler", &prefix(id)]));
    let created = answer["data"]["note"]["created_at"]
        .as_str()
        .expect("created_at");
    assert!(created.ends_with('Z'), "not UTC: {created}");
    let parsed = noteit_core::chrono::DateTime::parse_from_rfc3339(created)
        .expect("created_at must be RFC 3339");
    assert_eq!(parsed.timezone().local_minus_utc(), 0);
    assert!(
        !created.contains('/'),
        "a localised date reached the machine interface: {created}"
    );

    let (_, human, _) = sandbox.run(&["ler", &prefix(id)]);
    assert!(
        human.contains("Criada: ") && human.contains('/'),
        "the human view lost its local date: {human}"
    );
}

#[test]
fn a_note_created_through_json_is_addressable_by_the_identifier_it_answered_with() {
    let sandbox = Sandbox::new();
    let created = success(sandbox.run(&["--json", "criar", "primeiro"]));
    let id = note_id_of(&created);
    assert_eq!(id.len(), 36, "the identifier is not a full UUID: {id}");
    Uuid::parse_str(&id).expect("a usable identifier");

    let read = success(sandbox.run(&["--json", "ler", &id]));
    assert_eq!(read["data"]["note"]["note_id"], Value::String(id.clone()));
    assert_eq!(read["data"]["note"]["content"], "primeiro");

    let appended = success(sandbox.run(&["--json", "adicionar", &id, "segundo"]));
    assert_eq!(
        appended["data"]["write"]["note_id"],
        Value::String(id.clone())
    );
    assert_eq!(appended["data"]["write"]["commit_state"], "committed");

    let read_again = success(sandbox.run(&["--json", "ler", &id]));
    assert_eq!(read_again["data"]["note"]["content"], "primeiro\nsegundo");

    // Every identifier in every listing is a full UUID too.
    let listed = success(sandbox.run(&["--json", "listar"]));
    for note in listed["data"]["notes"].as_array().expect("notes") {
        let listed_id = note["note_id"].as_str().expect("id");
        Uuid::parse_str(listed_id).unwrap_or_else(|_| panic!("truncated id {listed_id}"));
    }
}

#[test]
fn search_answers_with_the_query_and_typed_results() {
    let sandbox = Sandbox::new();
    sandbox.seed("# Biópsia hepática\n\ncorpo");
    sandbox.seed("# Outro assunto");

    let found = success(sandbox.run(&["--json", "buscar", "biopsia"]));
    assert_eq!(found["command"], "search");
    assert_eq!(found["data"]["query"], "biopsia");
    assert_eq!(found["data"]["count"], 1);
    let result = &found["data"]["results"][0];
    assert!(result["match_count"].is_number());
    assert!(result["matched_text"].is_string());
    Uuid::parse_str(result["note_id"].as_str().expect("id")).expect("full uuid");
}

#[test]
fn the_tag_and_property_catalogues_carry_their_real_counts() {
    let sandbox = Sandbox::new();
    let first = sandbox.seed("um");
    let second = sandbox.seed("dois");
    for id in [first, second] {
        sandbox.run(&["tags", "adicionar", &prefix(id), "Medicina"]);
        sandbox.run(&["propriedades", "definir", &prefix(id), "fonte=Harrison"]);
    }

    let tags = success(sandbox.run(&["--json", "tags"]));
    assert_eq!(tags["data"]["count"], 1);
    assert_eq!(tags["data"]["tags"][0]["name"], "Medicina");
    assert_eq!(tags["data"]["tags"][0]["note_count"], 2);

    let properties = success(sandbox.run(&["--json", "propriedades"]));
    assert_eq!(properties["data"]["properties"][0]["key"], "fonte");
    assert_eq!(properties["data"]["properties"][0]["note_count"], 2);
}

// --- task and trash round trips ---------------------------------------------

#[test]
fn a_task_reference_read_from_json_completes_and_reopens_that_task() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("- [ ] Revisar noradrenalina\n- [ ] Revisar volume");

    let listed = success(sandbox.run(&["--json", "tarefas"]));
    assert_eq!(listed["data"]["state"], "pending");
    let task = listed["data"]["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["text"] == "Revisar noradrenalina")
        .expect("the task must be listed");
    let reference = task["task_ref"].as_str().expect("task_ref").to_string();
    let note = task["note_id"].as_str().expect("note_id").to_string();
    assert_eq!(note, id.to_string(), "a task must name its note in full");
    assert_eq!(task["checked"], Value::Bool(false));
    assert_eq!(task["completed_at"], Value::Null);

    let completed = success(sandbox.run(&["--json", "tarefas", "concluir", &note, &reference]));
    assert_eq!(completed["command"], "task_complete");
    assert_eq!(completed["data"]["write"]["commit_state"], "committed");

    // The task changed, so the reference that named it is out of date on
    // purpose. A conflict, never bad usage, and nothing was committed.
    let stale = failure(
        sandbox.run(&["--json", "tarefas", "concluir", &note, &reference]),
        1,
    );
    assert_eq!(stale["error"]["code"], "stale_task_ref");
    assert_eq!(stale["error"]["commit_state"], "not_committed");

    let done = success(sandbox.run(&["--json", "tarefas", "--estado", "concluidas"]));
    let refreshed = done["data"]["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["text"] == "Revisar noradrenalina")
        .expect("the completed task must be listed");
    assert_eq!(refreshed["checked"], Value::Bool(true));
    let refreshed_reference = refreshed["task_ref"].as_str().expect("task_ref");

    let reopened = success(sandbox.run(&["--json", "tasks", "reopen", &note, refreshed_reference]));
    assert_eq!(reopened["command"], "task_reopen");
    assert_eq!(reopened["data"]["write"]["kind"], "task_reopened");
    assert!(sandbox.body(id).contains("- [ ] Revisar noradrenalina"));
}

#[test]
fn a_trash_entry_read_from_json_restores_by_its_identifier() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("# Para a lixeira");
    sandbox
        .core()
        .storage()
        .move_note_to_trash(&id)
        .expect("trash");

    let listed = success(sandbox.run(&["--json", "lixeira"]));
    assert_eq!(listed["data"]["count"], 1);
    let entry = &listed["data"]["entries"][0];
    let note = entry["note_id"].as_str().expect("note_id").to_string();
    assert_eq!(note, id.to_string());
    assert!(entry["label"].is_string());
    assert!(
        !listed.to_string().contains(".md"),
        "a path leaked: {listed}"
    );

    let restored = success(sandbox.run(&["--json", "lixeira", "restaurar", &note]));
    assert_eq!(restored["command"], "trash_restore");
    assert_eq!(restored["data"]["write"]["kind"], "note_restored");
    assert_eq!(restored["data"]["write"]["commit_state"], "committed");
    assert_eq!(sandbox.body(id), "# Para a lixeira");
}

// --- the write contract -----------------------------------------------------

#[test]
fn a_direct_write_commits_and_leaves_configuration_and_window_state_alone() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("ABC");

    let appended = success(sandbox.run(&["--json", "adicionar", &prefix(id), "XYZ"]));
    assert_eq!(appended["data"]["write"]["commit_state"], "committed");
    assert_eq!(sandbox.body(id), "ABC\nXYZ");

    assert!(!sandbox.root.join("state/note-it/state.json").exists());
    assert!(!sandbox.root.join("config/note-it/config.toml").exists());
}

#[test]
fn a_no_op_write_is_a_success_that_needed_no_commit() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let selector = prefix(id);
    sandbox.run(&["tags", "adicionar", &selector, "Medicina"]);
    sandbox.run(&["propriedades", "definir", &selector, "fonte=Harrison"]);
    let before = sandbox.note_file(id);

    for arguments in [
        vec!["--json", "tags", "adicionar", &selector, "medicina"],
        vec!["--json", "tags", "remover", &selector, "inexistente"],
        vec![
            "--json",
            "propriedades",
            "definir",
            &selector,
            "fonte=Harrison",
        ],
        vec!["--json", "propriedades", "remover", &selector, "ausente"],
    ] {
        let answer = success(sandbox.run(&arguments));
        assert_eq!(answer["status"], "ok", "{arguments:?}");
        assert_eq!(
            answer["data"]["write"]["changed"],
            Value::Bool(false),
            "{arguments:?}"
        );
        assert_eq!(
            answer["data"]["write"]["commit_state"], "not_needed",
            "{arguments:?}"
        );
    }

    assert_eq!(
        sandbox.note_file(id),
        before,
        "a no-op write rewrote the note"
    );
}

#[test]
fn a_write_through_an_authority_answers_with_the_same_public_contract() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let outcome = WriteOutcome::new(id, WriteOutcomeKind::ContentAppended, true);
    let authority =
        FakeAuthority::start(&sandbox, AuthorityBehaviour::CommitOutcome(outcome.clone()));

    let through = success(sandbox.run(&["--json", "adicionar", &prefix(id), "mais"]));
    assert_eq!(authority.handled(), 1, "the request never reached anyone");
    assert_eq!(through["status"], "ok");
    assert_eq!(through["command"], "append");
    assert_eq!(through["data"]["write"]["note_id"], id.to_string());
    assert_eq!(through["data"]["write"]["kind"], "content_appended");
    assert_eq!(through["data"]["write"]["changed"], Value::Bool(true));
    assert_eq!(through["data"]["write"]["commit_state"], "committed");
    assert_eq!(through["data"]["write"]["ui_sync"]["status"], "ok");
    drop(authority);

    // Now the same operation on the direct path, on a store nobody holds.
    let direct_sandbox = Sandbox::new();
    let direct_id = direct_sandbox.seed("corpo");
    let direct = success(direct_sandbox.run(&["--json", "adicionar", &prefix(direct_id), "mais"]));

    // Everything except the identifier itself is the same document. Which of
    // the two paths ran is not something a consumer is told, because it is not
    // something a consumer can act on.
    assert_eq!(direct["status"], through["status"]);
    assert_eq!(direct["command"], through["command"]);
    assert_eq!(
        direct["data"]["write"]["kind"],
        through["data"]["write"]["kind"]
    );
    assert_eq!(
        direct["data"]["write"]["changed"],
        through["data"]["write"]["changed"]
    );
    assert_eq!(
        direct["data"]["write"]["commit_state"],
        through["data"]["write"]["commit_state"]
    );
    assert_eq!(
        direct["data"]["write"]["ui_sync"],
        through["data"]["write"]["ui_sync"]
    );
    assert_eq!(direct["warnings"], through["warnings"]);
}

#[test]
fn a_committed_write_whose_window_lagged_is_a_warning_and_stays_committed() {
    // The case a machine consumer must never read as a failure: the file has
    // the new text and only the open window is behind. Repeating the command
    // would append the same paragraph twice.
    let sandbox = Sandbox::new();
    let id = sandbox.seed("ABCD");
    let outcome = WriteOutcome::new(id, WriteOutcomeKind::ContentAppended, true)
        .with_ui_sync_warning("a janela aberta não confirmou a alteração");
    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::CommitOutcome(outcome));

    let (code, stdout, stderr) = sandbox.run(&["--json", "adicionar", &prefix(id), "XYZ"]);
    assert_eq!(code, 0, "a warning is not a failure: {stderr}");
    assert!(stderr.is_empty(), "the warning escaped to stderr: {stderr}");
    let answer = document(&stdout);

    assert_eq!(answer["status"], "warning");
    assert_eq!(answer["data"]["write"]["commit_state"], "committed");
    assert_eq!(answer["data"]["write"]["changed"], Value::Bool(true));
    assert_eq!(answer["data"]["write"]["ui_sync"]["status"], "warning");
    assert_eq!(
        answer["data"]["write"]["ui_sync"]["code"],
        "window_not_confirmed"
    );
    assert_eq!(answer["error"], Value::Null);
    assert_eq!(answer["warnings"].as_array().expect("warnings").len(), 1);
    assert_eq!(authority.handled(), 1, "the command sent the request twice");
}

#[test]
fn the_human_adapter_still_says_the_same_warning_its_own_way() {
    // The other half of the previous test. The same committed-but-unconfirmed
    // write must keep the human contract exactly as Phase 4.0E.1 left it: the
    // success line on standard output, the warning on standard error, exit 0,
    // and a sentence that tells the reader not to run it again.
    let sandbox = Sandbox::new();
    let id = sandbox.seed("ABCD");
    let outcome = WriteOutcome::new(id, WriteOutcomeKind::ContentAppended, true)
        .with_ui_sync_warning("a janela aberta não confirmou a alteração");
    let _authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::CommitOutcome(outcome));

    let (code, stdout, stderr) = sandbox.run(&["adicionar", &prefix(id), "XYZ"]);
    assert_eq!(code, 0, "the human adapter turned a warning into a failure");
    assert!(stdout.starts_with("Nota atualizada: "), "{stdout}");
    assert!(stderr.starts_with("Aviso:"), "{stderr}");
    assert!(stderr.contains("Não repita o comando"), "{stderr}");
}

#[test]
fn a_connection_that_drops_after_the_request_is_indeterminate_and_never_repeated() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::HangUpAfterRequest);

    let answer = failure(
        sandbox.run(&["--json", "adicionar", &prefix(id), "mais"]),
        1,
    );
    assert_eq!(answer["status"], "indeterminate");
    assert_eq!(answer["error"]["code"], "indeterminate");
    assert_eq!(
        answer["error"]["commit_state"], "unknown",
        "an unknown result was reported as a failed commit"
    );
    assert_ne!(answer["error"]["commit_state"], "not_committed");
    assert_eq!(
        authority.handled(),
        1,
        "the command repeated a request whose result it did not know"
    );
}

#[test]
fn an_answer_belonging_to_another_request_is_indeterminate_too() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::MismatchedResponseId);

    let answer = failure(
        sandbox.run(&["--json", "adicionar", &prefix(id), "mais"]),
        1,
    );
    assert_eq!(answer["status"], "indeterminate");
    assert_eq!(answer["error"]["code"], "indeterminate");
    assert_eq!(answer["error"]["commit_state"], "unknown");
    assert_eq!(authority.handled(), 1);
}

#[test]
fn an_authority_that_cannot_be_reached_committed_nothing() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare");
    let held = noteit_core::coordination::WriterLease::try_acquire_prepared(&coordination)
        .expect("prepare")
        .expect("lease");

    let answer = failure(
        sandbox.run(&["--json", "adicionar", &prefix(id), "mais"]),
        1,
    );
    assert_eq!(answer["status"], "error");
    assert_eq!(answer["error"]["code"], "authority_unavailable");
    assert_eq!(answer["error"]["commit_state"], "not_committed");
    assert_eq!(sandbox.body(id), "corpo");
    drop(held);
}

#[test]
fn an_authority_speaking_another_protocol_is_a_request_this_build_cannot_make() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let _authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::WrongVersion);

    let answer = failure(
        sandbox.run(&["--json", "adicionar", &prefix(id), "mais"]),
        2,
    );
    assert_eq!(answer["status"], "error");
    assert_eq!(answer["error"]["code"], "invalid_input");
    assert_eq!(answer["error"]["commit_state"], "not_committed");
    assert_eq!(sandbox.body(id), "corpo");
}

#[test]
fn a_store_that_cannot_be_written_reports_it_and_leaves_the_note_alone() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("intocada");
    let notes = sandbox.notes_dir();
    let original = std::fs::metadata(&notes).expect("metadata").permissions();

    let mut readonly = original.clone();
    readonly.set_mode(0o500);
    std::fs::set_permissions(&notes, readonly).expect("make the store read-only");

    let result = sandbox.run(&["--json", "adicionar", &prefix(id), "mais"]);

    // Restored before any assertion, so a failing assertion still leaves a
    // directory the temporary tree can be removed from.
    std::fs::set_permissions(&notes, original).expect("restore");

    let answer = failure(result, 1);
    assert_eq!(answer["status"], "error");
    assert_eq!(answer["error"]["code"], "persistence");
    assert_eq!(answer["error"]["commit_state"], "not_committed");
    assert_eq!(sandbox.body(id), "intocada");
}

#[test]
fn every_selector_failure_is_told_apart_by_its_code() {
    let sandbox = Sandbox::new();
    let first = sandbox.seed("um");
    // Two notes sharing a prefix, so an ambiguous selector is reachable.
    let shared = &first.as_simple().to_string()[..8];

    let invalid = failure(sandbox.run(&["--json", "ler", "xyz"]), 1);
    assert_eq!(invalid["error"]["code"], "invalid_input");

    let missing = failure(sandbox.run(&["--json", "ler", "00000000"]), 1);
    assert_eq!(missing["error"]["code"], "not_found");

    let missing_write = failure(sandbox.run(&["--json", "adicionar", "00000000", "x"]), 1);
    assert_eq!(missing_write["error"]["code"], "not_found");
    assert_eq!(missing_write["error"]["commit_state"], "not_committed");

    let bad_reference = failure(
        sandbox.run(&["--json", "tarefas", "concluir", shared, "nope"]),
        2,
    );
    assert_eq!(bad_reference["error"]["code"], "invalid_input");
    assert_eq!(bad_reference["error"]["commit_state"], "not_committed");

    // A trash restore whose live note is still there changes neither file.
    let trashed = sandbox.seed("# ocupada");
    sandbox
        .core()
        .storage()
        .move_note_to_trash(&trashed)
        .expect("trash");
    let mut revived = noteit_core::model::NoteDocument::new_empty();
    revived.metadata.id = trashed;
    revived.content = "outra coisa".to_string();
    sandbox
        .core()
        .storage()
        .save_note_atomic(&revived)
        .expect("occupy the identifier");

    let occupied = failure(
        sandbox.run(&["--json", "lixeira", "restaurar", &prefix(trashed)]),
        1,
    );
    assert_eq!(occupied["error"]["code"], "trash_target_occupied");
    assert_eq!(occupied["error"]["commit_state"], "not_committed");
    assert_eq!(sandbox.body(trashed), "outra coisa");
}

// --- input and equivalence --------------------------------------------------

#[test]
fn standard_input_is_unchanged_by_the_machine_interface() {
    let sandbox = Sandbox::new();
    let human_note = sandbox.seed("ABC");
    let machine_note = sandbox.seed("ABC");

    sandbox.run_with_stdin(
        &["adicionar", &prefix(human_note), "--stdin"],
        "linha 1\nlinha 2",
    );
    let answer = sandbox.run_with_stdin(
        &["--json", "adicionar", &prefix(machine_note), "--stdin"],
        "linha 1\nlinha 2",
    );
    let parsed = success(answer);
    assert_eq!(parsed["data"]["write"]["commit_state"], "committed");

    assert_eq!(sandbox.body(human_note), "ABC\nlinha 1\nlinha 2");
    assert_eq!(
        sandbox.body(machine_note),
        sandbox.body(human_note),
        "the machine interface changed what a payload means"
    );
}

#[test]
fn the_same_operation_changes_the_store_identically_in_both_modes() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let selector = prefix(id);
    let seeded = sandbox.note_file(id);
    let path = sandbox.notes_dir().join(format!("{id}.md"));

    // A tag moves neither timestamp, so the two runs are comparable byte for
    // byte — the strongest form of "the output changed and the operation did
    // not".
    sandbox.run(&["tags", "adicionar", &selector, "Medicina"]);
    let after_human = sandbox.note_file(id);

    std::fs::write(&path, &seeded).expect("restore the seed");
    success(sandbox.run(&["--json", "tags", "adicionar", &selector, "Medicina"]));
    let after_machine = sandbox.note_file(id);

    assert_eq!(
        after_human, after_machine,
        "the same tag write produced two different files"
    );
    assert_ne!(after_human, seeded, "the test proved nothing");

    // A content write moves `updated_at`, so the bodies are compared and the
    // creation instant is required to have stayed put.
    std::fs::write(&path, &seeded).expect("restore the seed");
    sandbox.run(&["adicionar", &selector, "XYZ"]);
    let human_document = sandbox.core().read_note(&id).expect("read");

    std::fs::write(&path, &seeded).expect("restore the seed");
    success(sandbox.run(&["--json", "adicionar", &selector, "XYZ"]));
    let machine_document = sandbox.core().read_note(&id).expect("read");

    assert_eq!(human_document.content, machine_document.content);
    assert_eq!(human_document.content, "corpo\nXYZ");
    assert_eq!(
        human_document.metadata.created_at,
        machine_document.metadata.created_at
    );
    assert_eq!(human_document.user_metadata, machine_document.user_metadata);
}

#[test]
fn a_machine_read_claims_no_coordination_state_at_all() {
    let sandbox = Sandbox::new();
    sandbox.seed("corpo");
    let coordination = sandbox.coordination();

    for arguments in [
        vec!["--json", "listar"],
        vec!["--json", "buscar", "corpo"],
        vec!["--json", "tags"],
        vec!["--json", "propriedades"],
        vec!["--json", "tarefas"],
        vec!["--json", "lixeira"],
        vec!["--json", "status"],
    ] {
        success(sandbox.run(&arguments));
    }

    assert!(
        !coordination.store_dir().exists(),
        "a machine read took a writer lease"
    );
}

#[test]
fn no_machine_document_carries_the_private_control_protocol() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::Commit);
    let through = success(sandbox.run(&["--json", "adicionar", &prefix(id), "mais"]));
    drop(authority);

    let created = success(sandbox.run(&["--json", "criar", "outra"]));
    let listed = success(sandbox.run(&["--json", "listar"]));

    for value in [through, created, listed] {
        let text = value.to_string();
        for forbidden in [
            "request_id",
            "protocol_version",
            "socket",
            "writer.lock",
            "lease",
            "generation",
            "write_path",
            "\"Direct\"",
            "\"Authority\"",
        ] {
            assert!(
                !text.contains(forbidden),
                "the private protocol leaked `{forbidden}`: {text}"
            );
        }
    }
}
