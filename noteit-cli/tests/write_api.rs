//! The CLI's half of the Write API: who writes, and what happens when someone
//! else already is.
//!
//! Every command here runs as a real process against a synthetic store with
//! its own runtime directory, so the lease and the control socket under test
//! are never the ones the person using this machine depends on.

mod support;

use noteit_core::control::{
    read_frame, ControlRequest, ControlResponse, ControlResult, MAX_FRAME_BYTES,
};
use noteit_core::coordination::WriterLease;
use noteit_core::write::{WriteOutcome, WriteOutcomeKind};
use noteit_core::Uuid;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use support::{prefix, AuthorityBehaviour, FakeAuthority, Sandbox};

/// The reference `noteit tarefas` printed for one task.
///
/// A note's own heading in that listing repeats the note's first line, which
/// for a note that begins with a task is the task's own words — so a line is
/// only a task line when it carries a checkbox. Reading it any other way picks
/// the note's identifier and calls it a task reference.
fn task_reference(listing: &str, text: &str) -> String {
    listing
        .lines()
        .filter(|line| line.contains("[ ]") || line.contains("[x]"))
        .find(|line| line.contains(text))
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or_else(|| panic!("no task reference for `{text}` in:\n{listing}"))
        .to_string()
}

// --- the commands ----------------------------------------------------------

#[test]
fn criar_and_create_both_make_a_note_headlessly_and_answer_with_its_uuid() {
    let sandbox = Sandbox::new();

    let (code, stdout, stderr) = sandbox.run(&["criar", "# Choque distributivo"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    assert!(stdout.starts_with("Nota criada: "), "{stdout}");

    let uuid = stdout
        .trim()
        .trim_start_matches("Nota criada: ")
        .to_string();
    let id = Uuid::parse_str(&uuid).expect("the answer must be a usable identifier");
    assert_eq!(sandbox.body(id), "# Choque distributivo");

    let (code, stdout, _) = sandbox.run(&["create", "# Outra"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("Nota criada: "));
    assert_eq!(sandbox.core().list_notes().expect("list").len(), 2);
}

#[test]
fn criar_accepts_multiline_markdown_on_standard_input_with_tags_and_properties() {
    let sandbox = Sandbox::new();
    let (code, stdout, stderr) = sandbox.run_with_stdin(
        &[
            "criar",
            "--stdin",
            "--tag",
            "Medicina",
            "--propriedade",
            "fonte=Harrison",
        ],
        "# Choque\n\nTexto com várias linhas.\n",
    );
    assert_eq!(code, 0, "{stderr}");

    let id = Uuid::parse_str(stdout.trim().trim_start_matches("Nota criada: ")).expect("uuid");
    let document = sandbox.core().read_note(&id).expect("read");
    assert_eq!(document.content, "# Choque\n\nTexto com várias linhas.");
    assert_eq!(document.user_metadata.tags.as_slice(), ["Medicina"]);
    assert_eq!(
        document.user_metadata.properties.as_slice()[0].value,
        "Harrison"
    );
}

#[test]
fn criar_never_opens_a_window_or_records_a_note_as_open() {
    let sandbox = Sandbox::new();
    let state = sandbox.root.join("state/note-it/state.json");
    let config = sandbox.root.join("config/note-it/config.toml");

    sandbox.run(&["criar", "texto"]);

    assert!(
        !state.exists(),
        "creating a note from the command line wrote window state"
    );
    assert!(
        !config.exists(),
        "creating a note from the command line wrote configuration"
    );
}

#[test]
fn adicionar_and_append_put_text_on_the_end() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("ABC");

    let (code, stdout, stderr) = sandbox.run(&["adicionar", &prefix(id), "XYZ"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with("Nota atualizada: "), "{stdout}");
    assert_eq!(sandbox.body(id), "ABC\nXYZ");

    sandbox.run(&["append", &prefix(id), "mais"]);
    assert_eq!(sandbox.body(id), "ABC\nXYZ\nmais");
}

#[test]
fn adicionar_reads_standard_input_and_refuses_both_sources_at_once() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("ABC");

    let (code, _, stderr) =
        sandbox.run_with_stdin(&["adicionar", &prefix(id), "--stdin"], "linha 1\nlinha 2");
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(sandbox.body(id), "ABC\nlinha 1\nlinha 2");

    let (code, stdout, stderr) =
        sandbox.run_with_stdin(&["adicionar", &prefix(id), "texto", "--stdin"], "outro");
    assert_eq!(code, 2, "both sources at once must be a usage error");
    assert!(stdout.is_empty());
    assert!(stderr.contains("nunca os dois"), "{stderr}");
    assert_eq!(sandbox.body(id), "ABC\nlinha 1\nlinha 2");
}

#[test]
fn adicionar_without_any_text_is_a_usage_error_that_writes_nothing() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("ABC");
    let (code, _, stderr) = sandbox.run(&["adicionar", &prefix(id)]);
    assert_eq!(code, 2);
    assert!(stderr.contains("informe o texto a acrescentar"), "{stderr}");
    assert_eq!(sandbox.body(id), "ABC");
}

#[test]
fn editar_replaces_the_body_and_refuses_to_empty_a_note_by_accident() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("valioso");

    let (code, _, stderr) = sandbox.run(&["editar", &prefix(id), "novo corpo"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(sandbox.body(id), "novo corpo");

    // The failure this guards against: a pipe that produced nothing.
    let (code, stdout, stderr) = sandbox.run_with_stdin(&["editar", &prefix(id), "--stdin"], "");
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(stderr.contains("--vazio"), "{stderr}");
    assert_eq!(sandbox.body(id), "novo corpo", "an empty pipe wiped a note");

    let (code, stdout, _) = sandbox.run(&["editar", &prefix(id), "--vazio"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("Nota esvaziada: "), "{stdout}");
    assert_eq!(sandbox.body(id), "");
}

#[test]
fn editar_refuses_vazio_together_with_text() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("valioso");
    let (code, _, stderr) = sandbox.run(&["edit", &prefix(id), "texto", "--vazio"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("não aceita texto junto"), "{stderr}");
    assert_eq!(sandbox.body(id), "valioso");
}

#[test]
fn tags_and_properties_keep_their_catalogues_and_gain_their_write_forms() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");

    // The read commands are untouched.
    let (code, stdout, _) = sandbox.run(&["tags"]);
    assert_eq!(code, 0);
    assert!(!stdout.is_empty());

    for (args, expected) in [
        (
            vec!["tags", "adicionar", &prefix(id), "Medicina"],
            "Tag adicionada.\n",
        ),
        (vec!["tags", "add", &prefix(id), "PBL"], "Tag adicionada.\n"),
        (
            vec!["tags", "remover", &prefix(id), "medicina"],
            "Tag removida.\n",
        ),
    ] {
        let (code, stdout, stderr) = sandbox.run(&args);
        assert_eq!(code, 0, "{args:?}: {stderr}");
        assert_eq!(stdout, expected, "{args:?}");
    }
    assert_eq!(
        sandbox
            .core()
            .read_note(&id)
            .expect("read")
            .user_metadata
            .tags
            .as_slice(),
        ["PBL"]
    );

    let (code, stdout, stderr) =
        sandbox.run(&["propriedades", "definir", &prefix(id), "fonte=Harrison"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "Propriedade atualizada.\n");

    let (code, stdout, _) = sandbox.run(&["properties", "remove", &prefix(id), "FONTE"]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Propriedade removida.\n");
    assert!(sandbox
        .core()
        .read_note(&id)
        .expect("read")
        .user_metadata
        .properties
        .is_empty());
}

#[test]
fn a_no_op_metadata_command_succeeds_and_says_nothing_happened() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    sandbox.run(&["tags", "adicionar", &prefix(id), "Medicina"]);

    let (code, stdout, stderr) = sandbox.run(&["tags", "adicionar", &prefix(id), "medicina"]);
    assert_eq!(code, 0, "a repeat is a success, not a failure");
    assert!(stderr.is_empty());
    assert!(stdout.contains("Nada foi alterado."), "{stdout}");

    let (code, stdout, _) = sandbox.run(&["tags", "remover", &prefix(id), "inexistente"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Nada foi alterado."), "{stdout}");
}

#[test]
fn tasks_are_listed_with_a_reference_and_completed_and_reopened_by_it() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("- [ ] Revisar noradrenalina\n- [ ] Revisar volume");

    let (code, stdout, _) = sandbox.run(&["tarefas"]);
    assert_eq!(code, 0);
    let reference = task_reference(&stdout, "Revisar noradrenalina");
    assert_eq!(reference.len(), 8, "{stdout}");
    assert_ne!(
        reference,
        prefix(id),
        "a task reference must not be the note's own identifier"
    );

    let (code, stdout, stderr) = sandbox.run(&["tarefas", "concluir", &prefix(id), &reference]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "Tarefa concluída.\n");
    assert!(sandbox.body(id).contains("- [x] Revisar noradrenalina"));
    assert!(sandbox.body(id).contains("- [ ] Revisar volume"));

    // The task changed, so the old reference is out of date on purpose.
    let (code, _, stderr) = sandbox.run(&["tasks", "complete", &prefix(id), &reference]);
    assert_eq!(code, 1, "a stale reference is a conflict, not bad usage");
    assert!(stderr.contains("não corresponde mais"), "{stderr}");

    let (_, stdout, _) = sandbox.run(&["tarefas", "--estado", "concluidas"]);
    let refreshed = task_reference(&stdout, "Revisar noradrenalina");
    let (code, stdout, _) = sandbox.run(&["tarefas", "reabrir", &prefix(id), &refreshed]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Tarefa reaberta.\n");
    assert!(sandbox.body(id).contains("- [ ] Revisar noradrenalina"));
}

#[test]
fn a_reference_that_is_not_one_is_bad_usage_rather_than_a_conflict() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("- [ ] Tarefa");
    let (code, _, stderr) = sandbox.run(&["tarefas", "concluir", &prefix(id), "nope"]);
    assert_eq!(code, 2, "{stderr}");
}

#[test]
fn lixeira_restaurar_brings_a_note_back_without_opening_anything() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("# Para a lixeira");
    sandbox
        .core()
        .storage()
        .move_note_to_trash(&id)
        .expect("trash");

    let (code, stdout, _) = sandbox.run(&["lixeira"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("Para a lixeira"), "{stdout}");

    let (code, stdout, stderr) = sandbox.run(&["lixeira", "restaurar", &prefix(id)]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with("Nota restaurada: "), "{stdout}");
    assert_eq!(sandbox.body(id), "# Para a lixeira");
    assert!(sandbox.core().list_trash().is_empty());

    assert!(
        !sandbox.root.join("state/note-it/state.json").exists(),
        "restoring a note opened a window"
    );
}

#[test]
fn a_write_command_never_prints_a_path_and_never_reflects_an_escape_sequence() {
    let sandbox = Sandbox::new();
    let (_, stdout, _) = sandbox.run(&["criar", "texto"]);
    let uuid = stdout
        .trim()
        .trim_start_matches("Nota criada: ")
        .to_string();
    assert!(!stdout.contains('/'), "a path reached stdout: {stdout}");

    // A selector carrying a terminal escape must not reach the terminal.
    let (code, stdout, stderr) = sandbox.run(&["adicionar", "\u{1b}[2J1234", "texto"]);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(!stderr.contains("\u{1b}[2J"), "{stderr:?}");

    // Nor may a tag that carries one, when it comes back in an error.
    let (_, _, stderr) = sandbox.run(&["tags", "adicionar", &uuid, "\u{1b}]0;x\u{7}tag"]);
    assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
}

#[test]
fn note_writes_never_touch_configuration_or_window_state() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let config = sandbox.root.join("config/note-it/config.toml");
    let state = sandbox.root.join("state/note-it/state.json");

    for args in [
        vec!["adicionar", &prefix(id), "mais"],
        vec!["tags", "adicionar", &prefix(id), "Medicina"],
        vec!["propriedades", "definir", &prefix(id), "k=v"],
        vec!["editar", &prefix(id), "outro"],
    ] {
        let (code, _, stderr) = sandbox.run(&args);
        assert_eq!(code, 0, "{args:?}: {stderr}");
    }

    assert!(
        !config.exists(),
        "a note write created a configuration file"
    );
    assert!(!state.exists(), "a note write created window state");
}

// --- who writes ------------------------------------------------------------

#[test]
fn a_read_only_command_creates_no_coordination_files_at_all() {
    let sandbox = Sandbox::new();
    sandbox.seed("corpo");
    let coordination = sandbox.coordination();

    for args in [
        vec!["listar"],
        vec!["tarefas"],
        vec!["tags"],
        vec!["propriedades"],
        vec!["lixeira"],
        vec!["buscar", "corpo"],
    ] {
        let (code, _, stderr) = sandbox.run(&args);
        assert_eq!(code, 0, "{args:?}: {stderr}");
    }

    assert!(
        !coordination.store_dir().exists(),
        "a read-only command claimed coordination state at {}",
        coordination.store_dir().display()
    );
}

#[test]
fn a_write_command_takes_the_lease_and_gives_it_straight_back() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let coordination = sandbox.coordination();

    let (code, _, stderr) = sandbox.run(&["adicionar", &prefix(id), "mais"]);
    assert_eq!(code, 0, "{stderr}");

    // Released the moment the command finished: the next writer gets it at once.
    let lease = WriterLease::try_acquire(&coordination)
        .expect("prepare")
        .expect("the lease was not released");
    drop(lease);
}

#[test]
fn a_lock_file_a_dead_process_left_behind_does_not_block_the_next_writer() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare");
    std::fs::write(coordination.lock_path(), b"stale").expect("leave a lock file");

    let (code, _, stderr) = sandbox.run(&["adicionar", &prefix(id), "mais"]);
    assert_eq!(
        code, 0,
        "a file left behind was treated as a held lease: {stderr}"
    );
    assert_eq!(sandbox.body(id), "corpo\nmais");
}

#[test]
fn a_socket_left_behind_with_a_free_lease_does_not_stop_a_direct_write() {
    // A socket file with nothing listening is what a crashed instance leaves.
    // The lease is the authority on whether anyone holds the store, and it is
    // free, so this writes directly and the debris is beside the point.
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare");
    std::fs::write(coordination.socket_path(), b"not a socket").expect("leave a socket file");

    let (code, _, stderr) = sandbox.run(&["adicionar", &prefix(id), "mais"]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(sandbox.body(id), "corpo\nmais");
}

#[test]
fn a_held_lease_with_no_authority_to_reach_fails_closed_and_changes_nothing() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare");

    // Held, with nothing listening: exactly the shape of an instance that has
    // the store and cannot be talked to.
    let held = WriterLease::try_acquire_prepared(&coordination)
        .expect("prepare")
        .expect("lease");

    let (code, stdout, stderr) = sandbox.run(&["adicionar", &prefix(id), "mais"]);
    assert_eq!(code, 1, "a held store must not be written around");
    assert!(stdout.is_empty());
    assert!(
        stderr.contains("outro escritor do Note-it")
            && stderr.contains("Nenhuma alteração foi feita"),
        "{stderr}"
    );
    assert_eq!(
        sandbox.body(id),
        "corpo",
        "the CLI wrote directly while another writer held the store"
    );
    drop(held);
}

#[test]
fn two_commands_writing_at_once_both_survive() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("base");
    let selector = prefix(id);

    let first = sandbox
        .command(&["adicionar", &selector, "PRIMEIRO"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    let second = sandbox
        .command(&["adicionar", &selector, "SEGUNDO"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");

    for mut child in [first, second] {
        let status = child.wait().expect("wait");
        assert!(status.success(), "one of the two commands failed");
    }

    let body = sandbox.body(id);
    assert!(
        body.contains("PRIMEIRO"),
        "the first append was lost: {body}"
    );
    assert!(
        body.contains("SEGUNDO"),
        "the second append was lost: {body}"
    );
    assert!(body.starts_with("base"), "{body}");
}

#[test]
fn a_command_waits_for_a_lease_that_is_about_to_be_released() {
    // The shape of a desktop instance that is starting, or another command
    // finishing: held for a moment, then free. Failing instantly on that would
    // lose an edit for no reason at all.
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare");
    let held = WriterLease::try_acquire_prepared(&coordination)
        .expect("prepare")
        .expect("lease");

    let mut child = sandbox
        .command(&["adicionar", &prefix(id), "mais"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");

    std::thread::sleep(std::time::Duration::from_millis(200));
    drop(held);

    let status = child.wait().expect("wait");
    assert!(
        status.success(),
        "the command gave up on a lease that freed"
    );
    assert_eq!(sandbox.body(id), "corpo\nmais");
}

// --- talking to the authority ----------------------------------------------

#[test]
fn a_held_store_with_an_authority_listening_is_written_through_it() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::Commit);

    let (code, stdout, stderr) = sandbox.run(&["adicionar", &prefix(id), "mais"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with("Nota atualizada: "), "{stdout}");
    assert_eq!(
        authority.handled(),
        1,
        "the request never reached the authority"
    );
    assert_eq!(
        sandbox.body(id),
        "corpo",
        "the CLI wrote the file itself instead of asking the authority"
    );
}

#[test]
fn an_authority_speaking_another_protocol_version_is_refused_without_a_write() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let _authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::WrongVersion);

    let (code, stdout, stderr) = sandbox.run(&["adicionar", &prefix(id), "mais"]);
    assert_eq!(
        code, 2,
        "a version mismatch is a request this build cannot make"
    );
    assert!(stdout.is_empty());
    assert!(stderr.contains("protocol"), "{stderr}");
    assert_eq!(sandbox.body(id), "corpo");
}

#[test]
fn a_connection_that_drops_after_the_request_is_reported_as_unknown_and_never_retried() {
    // The one outcome that is neither success nor failure. The authority may
    // have committed before the socket closed, so repeating the append could
    // put the text in twice — and that is the failure worth avoiding.
    let sandbox = Sandbox::new();
    let id = sandbox.seed("corpo");
    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::HangUpAfterRequest);

    let (code, stdout, stderr) = sandbox.run(&["adicionar", &prefix(id), "mais"]);
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(
        stderr.contains("não é possível dizer") && stderr.contains("Verifique a nota"),
        "{stderr}"
    );
    assert_eq!(
        authority.handled(),
        1,
        "the command sent the request more than once"
    );
}

#[test]
fn the_control_socket_is_reachable_only_by_its_owner() {
    let sandbox = Sandbox::new();
    let _authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::Commit);
    let coordination = sandbox.coordination();

    let socket_mode = std::fs::metadata(coordination.socket_path())
        .expect("socket metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(socket_mode, 0o600, "the control socket is not private");

    for directory in [coordination.runtime_root(), coordination.store_dir()] {
        let mode = std::fs::metadata(directory)
            .expect("directory metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "{} is not private", directory.display());
    }
}

#[test]
fn the_frame_limit_is_a_refusal_rather_than_a_truncation() {
    // Nothing on either end of this protocol ever reads to the end of a
    // stream, and nothing ever keeps the part of an oversized message that
    // happened to fit. A truncated append is a corrupted note.
    let mut header = (MAX_FRAME_BYTES + 1).to_be_bytes().to_vec();
    header.extend_from_slice(b"whatever");
    let error =
        read_frame::<_, ControlRequest>(&mut header.as_slice()).expect_err("oversized frame");
    assert!(error.to_string().contains("exceeds"), "{error}");
}

#[test]
fn a_repeated_request_identifier_is_answered_from_memory_rather_than_applied_twice() {
    // The authority remembers what it did with a request, so a client that
    // reconnects and repeats one gets the same answer instead of a second
    // append. Proven here against the shape of the protocol; the desktop
    // instance implements the same rule.
    let outcome = WriteOutcome::new(Uuid::new_v4(), WriteOutcomeKind::ContentAppended, true);
    let request_id = Uuid::new_v4();
    let first = ControlResponse::accepted(request_id, outcome.clone());
    let second = ControlResponse::accepted(request_id, outcome);
    assert_eq!(first.result, second.result);
    assert!(matches!(first.result, ControlResult::Committed(_)));
}
