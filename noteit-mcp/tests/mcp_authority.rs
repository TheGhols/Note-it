//! MCP-21 … MCP-23, MCP-25, MCP-26: the writer, the store and the peer.
//!
//! These are the properties that cannot be proved structurally, because the
//! bugs they guard against were bugs about real processes, real sockets and
//! real paths. So there are real ones here: a real lease, a real Unix socket,
//! a real second process, and the real `noteit-mcp` binary.

mod support;

use serde_json::json;
use support::{
    create_note, fingerprint, read_revision, AuthorityBehaviour, FakeAuthority, McpClient, Sandbox,
};

// ------------------------------------------------------------------ MCP-21

#[test]
fn mcp_21_restoring_from_the_trash_keeps_the_identity_rules() {
    let sandbox = Sandbox::new();
    let deleted = sandbox.seed("NOTA APAGADA");
    sandbox
        .core()
        .storage()
        .move_note_to_trash(&deleted)
        .expect("move to the trash");

    let mut client = McpClient::start(&sandbox);
    let listing = client.call("noteit_trash_list", json!({}));
    assert_eq!(listing.status(), "ok", "{}", listing.raw);
    assert_eq!(listing.structured()["count"], 1, "{}", listing.raw);
    assert_eq!(
        listing.structured()["entries"][0]["note_id"],
        deleted.to_string()
    );

    // The live note is gone until it is restored: a trash selector never
    // resolves against the live directory.
    let answer = client.call("noteit_read", json!({ "note_id": deleted.to_string() }));
    assert_eq!(answer.code(), Some("not_found"), "{}", answer.raw);

    let answer = client.call(
        "noteit_trash_restore",
        json!({ "note_id": deleted.to_string() }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(answer.commit_state(), Some("committed"));
    assert_eq!(answer.note_id(), deleted.to_string());
    // A move is not an edit, so it names no new version of a note.
    assert!(
        answer.structured().get("revision").is_none(),
        "a restore invented a revision: {}",
        answer.raw
    );
    assert_eq!(sandbox.body(&deleted.to_string()), "NOTA APAGADA");
}

/// A restore that would land on a live note carrying the same identifier is
/// refused, and neither file is touched.
#[test]
fn mcp_21_a_restore_onto_an_occupied_identity_is_refused() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("ORIGINAL");
    sandbox
        .core()
        .storage()
        .move_note_to_trash(&id)
        .expect("move to the trash");

    // A different note put back under the same identifier: the collision the
    // restore has to notice.
    let mut impostor = noteit_core::model::NoteDocument::new_empty();
    impostor.metadata.id = id;
    impostor.content = "OCUPANTE VIVO".to_string();
    sandbox
        .core()
        .storage()
        .save_note_atomic_with_id(&id, &impostor)
        .expect("write the live note");

    let live_before = sandbox.note_bytes(&id.to_string());
    let mut client = McpClient::start(&sandbox);
    let answer = client.call("noteit_trash_restore", json!({ "note_id": id.to_string() }));

    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(
        answer.code(),
        Some("trash_target_occupied"),
        "{}",
        answer.raw
    );
    assert_eq!(answer.commit_state(), Some("not_committed"));
    assert_eq!(
        live_before,
        sandbox.note_bytes(&id.to_string()),
        "a refused restore overwrote the live note"
    );
    // And the trash still has it, so nothing was consumed by the attempt.
    let listing = client.call("noteit_trash_list", json!({}));
    assert_eq!(listing.structured()["count"], 1, "{}", listing.raw);
}

// ------------------------------------------------------------------ MCP-22

/// When another instance holds the store, the write goes *through* it.
///
/// The evidence is on both sides: the authority saw exactly one request, and
/// the note changed. This server never took the lease, so it could not have
/// written the file itself.
#[test]
fn mcp_22_a_held_store_is_written_through_its_holder_and_never_around_it() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);
    let revision = read_revision(&mut client, &id);

    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::CommitForReal);

    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "PELA AUTORIDADE", "expected_revision": &revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(answer.commit_state(), Some("committed"));
    assert_eq!(
        authority.handled(),
        1,
        "the write did not go through the holder of the store"
    );
    assert_eq!(sandbox.body(&id), "BASE\nPELA AUTORIDADE");

    // The conditional guarantee survives the handover: a stale base sent to
    // the authority is refused there, not quietly written.
    let before = sandbox.note_bytes(&id);
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "OBSOLETO", "expected_revision": &revision }),
    );
    assert_eq!(answer.code(), Some("revision_conflict"), "{}", answer.raw);
    assert_eq!(authority.handled(), 2);
    assert_eq!(before, sandbox.note_bytes(&id));
}

// ------------------------------------------------------------------ MCP-23

/// The case a file cannot answer: somebody is typing, and the agent is behind.
///
/// The authority here is the one with the editor. Its base is the committed
/// note with the unsaved paragraph folded in — exactly what the desktop does,
/// through the same Core function — so a revision taken from the file is
/// already stale, and the write is refused before it can erase what is on the
/// person's screen.
#[test]
fn mcp_23_a_stale_agent_cannot_overwrite_text_a_person_has_not_saved() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);

    // The agent reads the note from disk. This is a legitimate, current
    // revision *of the file*.
    let revision = read_revision(&mut client, &id);
    let file_before = sandbox.note_bytes(&id);

    // Now a person types into the open window, and it has not been saved.
    let unsaved = "BASE\nA PESSOA ESTÁ DIGITANDO ISTO";
    let authority = FakeAuthority::start(
        &sandbox,
        AuthorityBehaviour::LiveEditor {
            unsaved_text: unsaved.to_string(),
        },
    );

    let answer = client.call(
        "noteit_edit",
        json!({ "note_id": &id, "body": "O AGENTE APAGOU TUDO", "expected_revision": &revision }),
    );

    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(
        answer.code(),
        Some("revision_conflict"),
        "an agent overwrote unsaved text: {}",
        answer.raw
    );
    assert_eq!(answer.commit_state(), Some("not_committed"));
    assert_eq!(authority.handled(), 1);
    assert_eq!(
        file_before,
        sandbox.note_bytes(&id),
        "the refused write reached the file"
    );
    assert!(
        !sandbox.body(&id).contains("O AGENTE APAGOU TUDO"),
        "the agent's text is in the note"
    );
}

/// And the same agent, having read the note again, writes on top of what the
/// person actually has. Which is the point of refusing rather than failing.
#[test]
fn mcp_23_the_agent_can_proceed_once_it_has_read_what_the_person_wrote() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);

    // The person's paragraph, this time already saved, so the file and the
    // editor agree — the ordinary case after an autosave.
    let unsaved = "BASE\nPARÁGRAFO DA PESSOA";
    let mut document = sandbox.core().read_note(&id.parse().unwrap()).unwrap();
    document.content = unsaved.to_string();
    sandbox
        .core()
        .storage()
        .save_note_atomic(&document)
        .expect("save");

    let authority = FakeAuthority::start(
        &sandbox,
        AuthorityBehaviour::LiveEditor {
            unsaved_text: unsaved.to_string(),
        },
    );

    // The agent reads *now*, which is the whole instruction after a conflict.
    let revision = read_revision(&mut client, &id);
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "CONCLUSÃO DO AGENTE", "expected_revision": &revision }),
    );

    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(authority.handled(), 1);
    let body = sandbox.body(&id);
    assert!(body.contains("PARÁGRAFO DA PESSOA"), "{body:?}");
    assert!(body.contains("CONCLUSÃO DO AGENTE"), "{body:?}");
}

// ------------------------------------------------------------------ MCP-25

/// A peer that speaks a different private protocol is refused, with no
/// fallback to writing anyway.
///
/// The failure this closes is specific and was real: two builds that disagree
/// about what a precondition means must not write for each other, because the
/// older one would drop the field and perform a *conditional* write
/// unconditionally.
#[test]
fn mcp_25_an_authority_speaking_another_private_protocol_is_refused() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);
    let revision = read_revision(&mut client, &id);
    let before = sandbox.note_bytes(&id);

    // Older, and newer. Neither may be met halfway.
    for version in [1u32, noteit_core::control::PROTOCOL_VERSION + 1] {
        let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::WrongVersion(version));
        let answer = client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": "NUNCA", "expected_revision": &revision }),
        );

        assert!(answer.is_error(), "version {version}: {}", answer.raw);
        assert_eq!(
            answer.code(),
            Some("invalid_input"),
            "version {version} was not refused as a mismatch: {}",
            answer.raw
        );
        assert_eq!(
            answer.commit_state(),
            Some("not_committed"),
            "version {version}: {}",
            answer.raw
        );
        assert_eq!(authority.handled(), 1, "version {version}");
        assert_eq!(
            before,
            sandbox.note_bytes(&id),
            "version {version} let a write through"
        );
        drop(authority);
    }
}

/// The other direction: a peer that refuses *us* on the version. Nothing is
/// written and nothing is retried without the precondition.
#[test]
fn mcp_25_a_peer_that_refuses_our_version_gets_no_second_unconditional_try() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);
    let revision = read_revision(&mut client, &id);
    let before = sandbox.note_bytes(&id);

    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::RefuseOnVersion);
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "NUNCA", "expected_revision": &revision }),
    );

    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(
        answer.commit_state(),
        Some("not_committed"),
        "{}",
        answer.raw
    );
    assert_eq!(
        authority.handled(),
        1,
        "a refusal on the version was retried"
    );
    assert_eq!(before, sandbox.note_bytes(&id));
}

/// The version this build states is the current one, and it is not the machine
/// interface's schema version. Two contracts, two numbers.
#[test]
fn mcp_25_the_private_protocol_is_version_two_and_is_not_a_public_number() {
    assert_eq!(noteit_core::control::PROTOCOL_VERSION, 2);
}

// ------------------------------------------------------------------ MCP-26

/// One physical store is one identity, however it is spelled.
///
/// The server is started against an alias — a symbolic link, a `./`, a `..`
/// round trip — and has to contend for the *same* lease a process using the
/// canonical path holds. A second authority raised because a path was written
/// differently would be two writers on one store, which is the failure the
/// lease exists to prevent.
#[test]
fn mcp_26_aliases_of_one_store_share_one_identity_and_one_lease() {
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();

    // The canonical world takes the lease and answers on the socket.
    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::CommitForReal);

    // The same directories, named differently.
    let alias_root = sandbox.root.join("alias");
    std::os::unix::fs::symlink(&sandbox.root, &alias_root).expect("symlink the sandbox");

    let spellings = [
        alias_root.display().to_string(),
        format!("{}/./", sandbox.root.display()),
        format!("{}/data/../", sandbox.root.display()),
        format!("{}//", sandbox.root.display()),
    ];

    let mut handled = authority.handled();
    for spelling in spellings {
        let mut command = std::process::Command::new(support::mcp_bin());
        command.env_remove("DISPLAY");
        command.env_remove("WAYLAND_DISPLAY");
        command.env_remove("DBUS_SESSION_BUS_ADDRESS");
        command.env("HOME", &spelling);
        command.env("XDG_DATA_HOME", format!("{spelling}/data"));
        command.env("XDG_CONFIG_HOME", format!("{spelling}/config"));
        command.env("XDG_STATE_HOME", format!("{spelling}/state"));
        command.env("XDG_CACHE_HOME", format!("{spelling}/cache"));
        command.env("XDG_RUNTIME_DIR", format!("{spelling}/runtime"));

        let mut client = support::McpClient::from_command(command);
        client.initialize(support::HANDSHAKE_PROTOCOL_VERSION);

        let revision = read_revision(&mut client, &id);
        let answer = client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": format!("VIA {spelling}"), "expected_revision": revision }),
        );
        assert_eq!(answer.status(), "ok", "{spelling}: {}", answer.raw);

        // The proof: the write went through the *same* authority. A server
        // that had resolved a second identity would have found the lease free
        // and written directly, and this counter would not have moved.
        let now = authority.handled();
        assert_eq!(
            now,
            handled + 1,
            "`{spelling}` did not resolve to the same store as the canonical path"
        );
        handled = now;
    }

    // Every append landed in the one note, in order, with nothing lost.
    assert_eq!(
        sandbox.body(&id).lines().count(),
        5,
        "{}",
        sandbox.body(&id)
    );
}

// ------------------------------------------------------------------ MCP-28

/// A note is a note. Changing one changes nothing else.
#[test]
fn mcp_28_note_mutations_never_touch_configuration_or_state() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE\n\n- [ ] fazer");

    // A configuration and a state the server has no business in.
    let paths = sandbox.store_paths();
    std::fs::create_dir_all(&paths.config_dir).expect("config dir");
    std::fs::create_dir_all(&paths.state_dir).expect("state dir");
    std::fs::write(
        paths.config_dir.join("config.toml"),
        "[janela]\nlargura = 400\n",
    )
    .expect("write config");
    std::fs::write(paths.state_dir.join("state.json"), "{\"janelas\":[]}").expect("write state");

    let config_before = fingerprint(&paths.config_dir);
    let state_before = fingerprint(&paths.state_dir);

    let tasks = client.call("noteit_tasks_list", json!({ "state": "all" }));
    let task_ref = tasks.structured()["tasks"][0]["task_ref"]
        .as_str()
        .unwrap()
        .to_string();

    let mut revision = read_revision(&mut client, &id);
    for (name, mut arguments) in [
        ("noteit_append", json!({ "note_id": &id, "text": "MAIS" })),
        ("noteit_tag_add", json!({ "note_id": &id, "tag": "Uma" })),
        (
            "noteit_property_set",
            json!({ "note_id": &id, "key": "k", "value": "v" }),
        ),
        (
            "noteit_task_complete",
            json!({ "note_id": &id, "task_ref": task_ref }),
        ),
    ] {
        arguments["expected_revision"] = json!(&revision);
        let answer = client.call(name, arguments);
        assert_eq!(answer.status(), "ok", "{name}: {}", answer.raw);
        revision = answer.revision();
    }

    assert_eq!(
        config_before,
        fingerprint(&paths.config_dir),
        "a note mutation changed the configuration"
    );
    assert_eq!(
        state_before,
        fingerprint(&paths.state_dir),
        "a note mutation changed the state"
    );
}
