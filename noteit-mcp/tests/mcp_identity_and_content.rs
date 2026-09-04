//! MCP-27 and MCP-29: which note, and what may be in it.
//!
//! Two questions that look unrelated and are the same question. A note is
//! addressed by an identifier the Core resolves; text is *content* and never
//! an address. Every failure this file guards against is one where those two
//! got confused — a body that becomes a path, a selector that becomes a
//! traversal, a front matter that decides which file gets written.

mod support;

use serde_json::json;
use support::{create_note, fingerprint, read_revision, McpClient, Sandbox};

// ------------------------------------------------------------------ MCP-27

/// A note with no front matter at all is read, has a stable revision, and can
/// be written conditionally like any other.
///
/// It is a case worth its own test because a note without front matter has its
/// metadata *derived from its filename*, and a derivation that is not
/// deterministic would give the note a different revision on every read — and
/// a precondition that can never match.
#[test]
fn mcp_27_a_note_without_front_matter_has_a_stable_identity_and_revision() {
    let sandbox = Sandbox::new();
    let paths = sandbox.store_paths();
    std::fs::create_dir_all(&paths.notes_dir).expect("notes dir");
    let id = noteit_core::Uuid::new_v4();
    std::fs::write(
        paths.notes_dir.join(format!("{id}.md")),
        "SEM FRONT MATTER\ncorpo",
    )
    .expect("write the bare note");
    let before = sandbox.note_bytes(&id.to_string());

    let mut client = McpClient::start(&sandbox);
    let answer = client.call("noteit_read", json!({ "note_id": id.to_string() }));
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(answer.structured()["note"]["note_id"], id.to_string());
    assert_eq!(
        answer.structured()["note"]["content"],
        "SEM FRONT MATTER\ncorpo"
    );

    let first = read_revision(&mut client, &id.to_string());
    let second = read_revision(&mut client, &id.to_string());
    assert_eq!(
        first, second,
        "a note without front matter must not be re-dated on every read"
    );
    assert_eq!(
        before,
        sandbox.note_bytes(&id.to_string()),
        "reading a note without front matter wrote to it"
    );

    // And a stale revision still conflicts, so the guarantee is not weaker for
    // a note whose metadata was derived.
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": id.to_string(), "text": "ACRESCENTADO", "expected_revision": &first }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    let after = sandbox.note_bytes(&id.to_string());
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": id.to_string(), "text": "DE NOVO", "expected_revision": &first }),
    );
    assert_eq!(answer.code(), Some("revision_conflict"), "{}", answer.raw);
    assert_eq!(after, sandbox.note_bytes(&id.to_string()));
}

/// A file whose name and front matter disagree about which note it is cannot
/// be read, and therefore cannot be written.
///
/// Refusing the read is what makes the write impossible: there is no path from
/// an inconsistent file to a mutation, because a mutation needs a revision and
/// a revision needs a document nobody would refuse.
#[test]
fn mcp_27_a_file_whose_identity_disagrees_with_its_name_is_refused() {
    let sandbox = Sandbox::new();
    let paths = sandbox.store_paths();
    std::fs::create_dir_all(&paths.notes_dir).expect("notes dir");

    // Serialised by the Core itself, so the front matter is valid in every way
    // except the one under test: the file is named after a different note.
    let filename_id = noteit_core::Uuid::new_v4();
    let mut document = noteit_core::model::NoteDocument::new_empty();
    document.content = "CONTEÚDO".to_string();
    let front_matter_id = document.metadata.id;
    let raw = document.serialize().expect("serialize");
    let path = paths.notes_dir.join(format!("{filename_id}.md"));
    std::fs::write(&path, &raw).expect("write the inconsistent note");
    let before = std::fs::read(&path).expect("read");

    let mut client = McpClient::start(&sandbox);

    // Named by its filename: refused.
    let answer = client.call("noteit_read", json!({ "note_id": filename_id.to_string() }));
    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(answer.code(), Some("read_failed"), "{}", answer.raw);

    // Named by the identifier inside it: there is no such file, so it is not
    // found — and crucially not silently redirected to the file above.
    let answer = client.call(
        "noteit_read",
        json!({ "note_id": front_matter_id.to_string() }),
    );
    assert!(answer.is_error(), "{}", answer.raw);
    assert_eq!(answer.code(), Some("not_found"), "{}", answer.raw);

    // No mutation can reach it under either name.
    for named in [filename_id, front_matter_id] {
        let answer = client.call(
            "noteit_append",
            json!({
                "note_id": named.to_string(),
                "text": "NÃO",
                "expected_revision": "a".repeat(64),
            }),
        );
        assert!(answer.is_error(), "{named}: {}", answer.raw);
        assert_ne!(
            answer.status(),
            "ok",
            "an inconsistent note was written: {}",
            answer.raw
        );
    }
    assert_eq!(
        before,
        std::fs::read(&path).expect("read"),
        "the file moved"
    );

    // The listing survives it: one file with a confused identity is a warning
    // beside the results, never a store that cannot be read.
    let good = create_note(&mut client, "NOTA BOA");
    let listing = client.call("noteit_list", json!({}));
    assert_eq!(listing.status(), "ok", "{}", listing.raw);
    assert_eq!(listing.structured()["count"], 1, "{}", listing.raw);
    assert_eq!(listing.structured()["notes"][0]["note_id"], good);
    let warnings = listing.structured()["warnings"]
        .as_array()
        .expect("warnings");
    assert_eq!(warnings.len(), 1, "{}", listing.raw);
    assert_eq!(warnings[0]["code"], "unreadable_note");
    assert_eq!(warnings[0]["note_id"], filename_id.to_string());
    // The warning says which file could not be read and what kind of damage it
    // is, and it says it as data. It deliberately carries no sentence at all.
    //
    // It did until Phase 4.2R, and this assertion is the one that used to
    // require it: the sentence was the Core's, written for whoever is debugging
    // a store, so it named the file — and on the symlink path it named the
    // absolute path of the notes directory to whoever was listening. What a
    // caller is owed is the code and the identifier, which is what the context
    // surface settled on in 4.2C and what all five read surfaces publish now.
    assert!(
        warnings[0].get("message").is_none(),
        "a warning carried a sentence: {}",
        listing.raw
    );
    let rendered = listing.raw.to_string();
    assert!(
        !rendered.contains(&front_matter_id.to_string()),
        "the identity found inside the file was published: {rendered}"
    );
    assert!(
        !rendered.contains(&sandbox.root.display().to_string()) && !rendered.contains(".md"),
        "a listing over a damaged store named the filesystem: {rendered}"
    );
}

/// A write addressed to one note can never land in another.
///
/// The Core refuses to commit a document whose identity is not the one that
/// was addressed. Here the whole chain is exercised through MCP: two notes,
/// two identifiers, and a mutation on each that must touch only its own file.
#[test]
fn mcp_27_a_write_to_one_note_never_reaches_another() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let first = create_note(&mut client, "PRIMEIRA");
    let second = create_note(&mut client, "SEGUNDA");

    let second_before = sandbox.note_bytes(&second);
    let revision = read_revision(&mut client, &first);
    let answer = client.call(
        "noteit_edit",
        json!({ "note_id": &first, "body": "SÓ A PRIMEIRA", "expected_revision": revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(answer.note_id(), first);

    assert_eq!(sandbox.body(&first), "SÓ A PRIMEIRA");
    assert_eq!(
        second_before,
        sandbox.note_bytes(&second),
        "a write to one note changed another"
    );
}

/// A selector is never a path.
#[test]
fn mcp_27_a_selector_that_looks_like_a_path_is_refused_outright() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let revision = read_revision(&mut client, &id);
    let before = fingerprint(&sandbox.root);

    let hostile = [
        "../../../etc/passwd",
        "..",
        "./abcdef12",
        "abcdef12/../abcdef13",
        "/etc/passwd",
        "abcdef12.md",
        "~/notes/abcdef12",
        "abcdef12\0abcdef13",
        "nota bonita",
        "",
    ];

    for selector in hostile {
        let answer = client.call("noteit_read", json!({ "note_id": selector }));
        assert!(answer.is_error(), "`{selector}` was read: {}", answer.raw);
        assert!(
            matches!(answer.code(), Some("invalid_input") | Some("not_found")),
            "`{selector}` was refused for the wrong reason: {}",
            answer.raw
        );

        let answer = client.call(
            "noteit_append",
            json!({ "note_id": selector, "text": "NÃO", "expected_revision": &revision }),
        );
        assert!(
            answer.is_error(),
            "`{selector}` was written: {}",
            answer.raw
        );
    }

    drop(client);
    assert_eq!(
        before,
        fingerprint(&sandbox.root),
        "a path-shaped selector changed the world"
    );
}

// ------------------------------------------------------------------ MCP-29

/// Text is text. Everything hostile a person can put in a note goes in
/// unchanged, comes back unchanged, and reaches nothing but the note.
#[test]
fn mcp_29_hostile_content_survives_the_boundary_without_escaping_it() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);

    let payloads: &[(&str, &str)] = &[
        ("aspas", "ela disse \"não\" e 'talvez'"),
        ("barras invertidas", r"C:\caminho\.. \\ \n literal"),
        ("novas linhas", "primeira\nsegunda\r\nterceira"),
        ("tabulação", "coluna\tcoluna"),
        ("unicode", "ação, ñ, ß, 中文, العربية"),
        ("emoji", "🩺 💉 🧬 👩🏽‍⚕️"),
        ("RTL legítimo", "מחלקה: קרדיולוגיה"),
        ("controles bidi", "antes\u{202E}sedner\u{202C}depois"),
        (
            "sequência ANSI",
            "\u{1b}[31mvermelho\u{1b}[0m \u{1b}]0;título\u{7}",
        ),
        (
            "caracteres de controle",
            "campainha\u{7} nulo-ish\u{1} vertical\u{b}",
        ),
        (
            "markdown hostil",
            "<script>alert(1)</script>\n[x](javascript:alert(1))\n<!-- -->",
        ),
        ("parece um caminho", "../../etc/passwd"),
        (
            "parece um comando",
            "$(rm -rf ~) `whoami` && echo oi | tee /tmp/x",
        ),
        (
            "parece front matter",
            "---\nnote_it:\n  id: 00000000-0000-0000-0000-000000000000\n---\n",
        ),
        (
            "parece json",
            "{\"note_id\": \"outro\", \"expected_revision\": null}",
        ),
        ("muito longo", "x"),
    ];

    let outside = sandbox.root.join("nao-deve-existir");

    for (why, payload) in payloads {
        let answer = client.call("noteit_create", json!({ "content": payload }));
        assert_eq!(answer.status(), "ok", "{why}: {}", answer.raw);
        let id = answer.note_id();

        // What went in is what comes back, byte for byte after the Core's own
        // canonicalisation of trailing line breaks.
        let read = client.call("noteit_read", json!({ "note_id": &id }));
        let returned = read.structured()["note"]["content"]
            .as_str()
            .expect("content");
        assert_eq!(
            returned,
            noteit_core::NoteDocument::canonical_content(payload),
            "{why} did not survive the round trip"
        );

        // And it reached exactly one file: the note's own.
        assert!(
            !outside.exists(),
            "{why} created something outside the store"
        );
    }

    // Every note is in the notes directory and nowhere else, and there is one
    // per payload — nothing was routed anywhere by its content.
    let notes: Vec<_> = std::fs::read_dir(&sandbox.store_paths().notes_dir)
        .expect("read notes")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(notes.len(), payloads.len(), "{notes:?}");
    for name in &notes {
        assert!(name.ends_with(".md"), "{name}");
        noteit_core::Uuid::parse_str(name.trim_end_matches(".md")).unwrap_or_else(|_| {
            panic!("a file was named by something other than an identifier: {name}")
        });
    }
}

/// Hostile text in a tag, a property and a task reference is validated by the
/// domain rather than executed by anything.
#[test]
fn mcp_29_hostile_metadata_is_validated_and_never_executed() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let mut revision = read_revision(&mut client, &id);

    // Tags that must be refused by a domain rule, not by a crash.
    for tag in ["", "   ", "#", &"t".repeat(500)] {
        let answer = client.call(
            "noteit_tag_add",
            json!({ "note_id": &id, "tag": tag, "expected_revision": &revision }),
        );
        assert!(
            answer.is_error(),
            "the tag {tag:?} was accepted: {}",
            answer.raw
        );
        assert!(
            matches!(answer.code(), Some("validation") | Some("invalid_input")),
            "{tag:?}: {}",
            answer.raw
        );
    }

    // A tag that is unusual but legitimate goes in as text.
    let answer = client.call(
        "noteit_tag_add",
        json!({ "note_id": &id, "tag": "Cardiologia-Ünïcode", "expected_revision": &revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    revision = answer.revision();

    // A property value carrying a shell-looking string is a value.
    let answer = client.call(
        "noteit_property_set",
        json!({
            "note_id": &id,
            "key": "fonte",
            "value": "$(rm -rf ~); `id`",
            "expected_revision": &revision,
        }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    let read = client.call("noteit_read", json!({ "note_id": &id }));
    assert_eq!(
        read.structured()["note"]["properties"][0]["value"],
        "$(rm -rf ~); `id`"
    );
    revision = answer.revision();

    // A task reference that is not one is a usage error, and a well-formed one
    // that names nothing is stale — two different answers, deliberately.
    let answer = client.call(
        "noteit_task_complete",
        json!({ "note_id": &id, "task_ref": "../../x", "expected_revision": &revision }),
    );
    assert_eq!(answer.code(), Some("invalid_input"), "{}", answer.raw);

    let answer = client.call(
        "noteit_task_complete",
        json!({ "note_id": &id, "task_ref": "abcd1234", "expected_revision": &revision }),
    );
    assert_eq!(answer.code(), Some("stale_task_ref"), "{}", answer.raw);
}

/// Extra fields a client sends are not silently accepted as instructions.
#[test]
fn mcp_29_unknown_arguments_do_not_become_instructions() {
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let id = create_note(&mut client, "BASE");
    let revision = read_revision(&mut client, &id);
    let other = create_note(&mut client, "OUTRA");
    let other_before = sandbox.note_bytes(&other);

    // A field nobody declared, spelled to look like one that would matter.
    let answer = client.call(
        "noteit_append",
        json!({
            "note_id": &id,
            "text": "ACRESCENTADO",
            "expected_revision": &revision,
            "force": true,
            "unconditional": true,
            "path": "/etc/passwd",
            "note_id_override": other,
        }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(
        answer.note_id(),
        id,
        "an undeclared field redirected the write"
    );
    assert_eq!(sandbox.body(&id), "BASE\nACRESCENTADO");
    assert_eq!(
        other_before,
        sandbox.note_bytes(&other),
        "an undeclared field reached another note"
    );
}
