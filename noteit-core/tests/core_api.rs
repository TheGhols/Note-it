use noteit_core::filter::{NoteFilter, NoteSelectorError};
use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use noteit_core::storage::{StorageManager, StorePaths};
use noteit_core::task::TaskStateFilter;
use noteit_core::warning::ReadWarningKind;
use noteit_core::NoteItCore;
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

fn synthetic_core() -> (tempfile::TempDir, NoteItCore) {
    let root = tempdir().expect("temporary core store");
    let storage = StorageManager::with_custom_paths(
        root.path().join("data/note-it/notes"),
        root.path().join("config/note-it"),
        root.path().join("state/note-it"),
        root.path().join("runtime/note-it"),
    )
    .expect("synthetic storage");
    (root, NoteItCore::from_storage(storage))
}

#[test]
fn test_01_read_only_open_does_not_create_directories() {
    let tmp = tempdir().expect("tempdir");
    let non_existent_paths = StorePaths::from_custom_paths(
        tmp.path().join("absent_data/notes"),
        tmp.path().join("absent_config"),
        tmp.path().join("absent_state"),
        tmp.path().join("absent_runtime"),
    );

    assert!(!non_existent_paths.notes_dir.exists());
    assert!(!non_existent_paths.config_dir.exists());
    assert!(!non_existent_paths.state_dir.exists());
    assert!(!non_existent_paths.runtime_dir.exists());

    let core = NoteItCore::open_read_only_at(non_existent_paths.clone());

    // Calling read operations on absent store
    assert_eq!(core.list_notes().expect("list notes"), Vec::<Uuid>::new());
    assert!(core
        .list_summaries(&NoteFilter::default(), None)
        .expect("summaries")
        .items
        .is_empty());
    assert!(core.search_notes("teste").is_empty());
    assert!(core
        .list_tasks(TaskStateFilter::All, &NoteFilter::default(), None)
        .expect("tasks")
        .items
        .is_empty());
    assert!(core.list_trash().is_empty());
    assert!(core.metadata_catalog().tags.is_empty());

    // Verify absolutely nothing was created
    assert!(!non_existent_paths.notes_dir.exists());
    assert!(!non_existent_paths.config_dir.exists());
    assert!(!non_existent_paths.state_dir.exists());
    assert!(!non_existent_paths.runtime_dir.exists());
}

#[test]
fn test_02_empty_store_returns_empty_collections_cleanly() {
    let (_root, core) = synthetic_core();
    assert_eq!(core.list_notes().expect("list"), Vec::<Uuid>::new());
    assert!(core
        .list_summaries(&NoteFilter::default(), None)
        .expect("summaries")
        .items
        .is_empty());
    assert!(core.search_notes("query").is_empty());
    assert!(core
        .list_tasks(TaskStateFilter::All, &NoteFilter::default(), None)
        .expect("tasks")
        .items
        .is_empty());
    assert!(core.list_trash().is_empty());
}

#[test]
fn test_03_list_recency_orders_most_recent_first() {
    let (_root, core) = synthetic_core();
    let mut n1 = NoteDocument::new_empty();
    n1.content = "Primeira nota".to_string();
    core.storage().save_note_atomic(&n1).expect("save n1");
    std::thread::sleep(std::time::Duration::from_millis(20));

    let mut n2 = NoteDocument::new_empty();
    n2.content = "Segunda nota".to_string();
    core.storage().save_note_atomic(&n2).expect("save n2");

    let batch = core
        .list_summaries(&NoteFilter::default(), None)
        .expect("list");
    assert_eq!(batch.items.len(), 2);
    assert_eq!(
        batch.items[0].id, n2.metadata.id,
        "Most recent note must come first"
    );
    assert_eq!(batch.items[1].id, n1.metadata.id);
}

#[test]
fn test_04_legacy_note_without_timestamps_loads_and_lists() {
    let (root, core) = synthetic_core();
    let id = Uuid::new_v4();
    let legacy_raw = format!(
        "---\nnote_it:\n  version: 1\n  id: {id}\n  color: yellow\n  font_size: 15\n---\n\n# Nota Legada\nCorpo da nota legada.\n"
    );
    let note_path = root
        .path()
        .join("data/note-it/notes")
        .join(format!("{id}.md"));
    fs::write(note_path, legacy_raw).expect("write legacy file");

    let loaded = core.read_note(&id).expect("read legacy note");
    assert_eq!(loaded.metadata.id, id);
    assert_eq!(loaded.metadata.created_at, None);
    assert_eq!(loaded.metadata.updated_at, None);

    let batch = core
        .list_summaries(&NoteFilter::default(), None)
        .expect("summaries");
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].id, id);
    assert_eq!(batch.items[0].label, "Nota Legada");
}

#[test]
fn test_05_metadata_note_loads_tags_and_properties() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "# Choque séptico".to_string();
    note.user_metadata = NoteMetadata::try_new(
        ["Medicina".into(), "PBL".into()],
        [NoteProperty {
            key: "disciplina".into(),
            value: "cardiologia".into(),
        }],
    )
    .expect("metadata");

    core.storage().save_note_atomic(&note).expect("save");

    let read_doc = core.read_note(&note.metadata.id).expect("read");
    assert_eq!(read_doc.user_metadata.tags.as_slice(), ["Medicina", "PBL"]);
    assert_eq!(
        read_doc.user_metadata.properties.as_slice()[0].key,
        "disciplina"
    );
    assert_eq!(
        read_doc.user_metadata.properties.as_slice()[0].value,
        "cardiologia"
    );
}

#[test]
fn test_06_label_uses_canonical_search_label_logic() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content =
        "## <span data-note-it-color=\"#ff0000\">Título estilizado</span>\n\nCorpo".to_string();
    core.storage().save_note_atomic(&note).expect("save");

    let batch = core
        .list_summaries(&NoteFilter::default(), None)
        .expect("summaries");
    assert_eq!(batch.items[0].label, "Título estilizado");
}

#[test]
fn test_07_tag_filter_matches_single_tag() {
    let (_root, core) = synthetic_core();
    let mut n1 = NoteDocument::new_empty();
    n1.content = "Nota 1".to_string();
    n1.user_metadata = NoteMetadata::try_new(["Medicina".into()], []).expect("meta");
    core.storage().save_note_atomic(&n1).expect("save n1");

    let mut n2 = NoteDocument::new_empty();
    n2.content = "Nota 2".to_string();
    n2.user_metadata = NoteMetadata::try_new(["Projeto".into()], []).expect("meta");
    core.storage().save_note_atomic(&n2).expect("save n2");

    let filter = NoteFilter::new(vec!["Medicina".into()], vec![]);
    let batch = core.list_summaries(&filter, None).expect("list");
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].id, n1.metadata.id);
}

#[test]
fn test_08_repeated_tag_uses_and_semantics() {
    let (_root, core) = synthetic_core();
    let mut n1 = NoteDocument::new_empty();
    n1.user_metadata = NoteMetadata::try_new(["Medicina".into(), "PBL".into()], []).expect("meta");
    core.storage().save_note_atomic(&n1).expect("save n1");

    let mut n2 = NoteDocument::new_empty();
    n2.user_metadata = NoteMetadata::try_new(["Medicina".into()], []).expect("meta");
    core.storage().save_note_atomic(&n2).expect("save n2");

    let filter = NoteFilter::new(vec!["Medicina".into(), "PBL".into()], vec![]);
    let batch = core.list_summaries(&filter, None).expect("list");
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].id, n1.metadata.id);
}

#[test]
fn test_09_property_filter_matches_single_property() {
    let (_root, core) = synthetic_core();
    let mut n1 = NoteDocument::new_empty();
    n1.user_metadata = NoteMetadata::try_new(
        [],
        [NoteProperty {
            key: "disciplina".into(),
            value: "cardiologia".into(),
        }],
    )
    .expect("meta");
    core.storage().save_note_atomic(&n1).expect("save n1");

    let filter = NoteFilter::new(vec![], vec![("disciplina".into(), "cardiologia".into())]);
    let batch = core.list_summaries(&filter, None).expect("list");
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].id, n1.metadata.id);
}

#[test]
fn test_10_repeated_property_uses_and_semantics() {
    let (_root, core) = synthetic_core();
    let mut n1 = NoteDocument::new_empty();
    n1.user_metadata = NoteMetadata::try_new(
        [],
        [
            NoteProperty {
                key: "disciplina".into(),
                value: "cardiologia".into(),
            },
            NoteProperty {
                key: "status".into(),
                value: "revisar".into(),
            },
        ],
    )
    .expect("meta");
    core.storage().save_note_atomic(&n1).expect("save n1");

    let mut n2 = NoteDocument::new_empty();
    n2.user_metadata = NoteMetadata::try_new(
        [],
        [NoteProperty {
            key: "disciplina".into(),
            value: "cardiologia".into(),
        }],
    )
    .expect("meta");
    core.storage().save_note_atomic(&n2).expect("save n2");

    let filter = NoteFilter::new(
        vec![],
        vec![
            ("disciplina".into(), "cardiologia".into()),
            ("status".into(), "revisar".into()),
        ],
    );
    let batch = core.list_summaries(&filter, None).expect("list");
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].id, n1.metadata.id);
}

#[test]
fn test_11_accent_and_case_tag_identity() {
    let (_root, core) = synthetic_core();
    let mut n1 = NoteDocument::new_empty();
    n1.user_metadata = NoteMetadata::try_new(["Urgência".into()], []).expect("meta");
    core.storage().save_note_atomic(&n1).expect("save");

    let f1 = NoteFilter::new(vec!["urgencia".into()], vec![]);
    assert_eq!(core.list_summaries(&f1, None).expect("list").items.len(), 1);

    let f2 = NoteFilter::new(vec!["URGÊNCIA".into()], vec![]);
    assert_eq!(core.list_summaries(&f2, None).expect("list").items.len(), 1);
}

#[test]
fn test_12_accent_and_case_property_key_and_value() {
    let (_root, core) = synthetic_core();
    let mut n1 = NoteDocument::new_empty();
    n1.user_metadata = NoteMetadata::try_new(
        [],
        [NoteProperty {
            key: "Situação".into(),
            value: "Concluído".into(),
        }],
    )
    .expect("meta");
    core.storage().save_note_atomic(&n1).expect("save");

    let filter = NoteFilter::new(vec![], vec![("situacao".into(), "concluido".into())]);
    let batch = core.list_summaries(&filter, None).expect("list");
    assert_eq!(batch.items.len(), 1);
}

#[test]
fn test_13_note_selector_full_uuid() {
    let (_root, core) = synthetic_core();
    let note = NoteDocument::new_empty();
    core.storage().save_note_atomic(&note).expect("save");

    let uuid_str = note.metadata.id.to_string();
    let resolved = core.resolve_note_id(&uuid_str).expect("resolve full uuid");
    assert_eq!(resolved, note.metadata.id);
}

#[test]
fn test_14_note_selector_unique_prefix() {
    let (_root, core) = synthetic_core();
    let note = NoteDocument::new_empty();
    core.storage().save_note_atomic(&note).expect("save");

    let prefix = &note.metadata.id.to_string()[..8];
    let resolved = core.resolve_note_id(prefix).expect("resolve 8-char prefix");
    assert_eq!(resolved, note.metadata.id);
}

#[test]
fn test_15_invalid_selector_formats_are_rejected() {
    let (_root, core) = synthetic_core();
    assert!(matches!(
        core.resolve_note_id("../../etc/passwd"),
        Err(NoteSelectorError::InvalidFormat(_))
    ));
    assert!(matches!(
        core.resolve_note_id("/home/user/secret"),
        Err(NoteSelectorError::InvalidFormat(_))
    ));
    assert!(matches!(
        core.resolve_note_id("1234"),
        Err(NoteSelectorError::InvalidFormat(_))
    ));
    assert!(matches!(
        core.resolve_note_id("not-hex-chars!"),
        Err(NoteSelectorError::InvalidFormat(_))
    ));
}

#[test]
fn test_16_ambiguous_prefix_returns_ambiguous_error() {
    let (root, core) = synthetic_core();
    let id1 = Uuid::parse_str("aaaaaaaa-1111-2222-3333-444455556666").unwrap();
    let id2 = Uuid::parse_str("aaaaaaaa-9999-8888-7777-666655554444").unwrap();

    let mut doc1 = NoteDocument::new_with_id(id1);
    doc1.content = "Nota 1".into();
    let mut doc2 = NoteDocument::new_with_id(id2);
    doc2.content = "Nota 2".into();

    let notes_dir = root.path().join("data/note-it/notes");
    fs::write(
        notes_dir.join(format!("{id1}.md")),
        doc1.serialize().unwrap(),
    )
    .unwrap();
    fs::write(
        notes_dir.join(format!("{id2}.md")),
        doc2.serialize().unwrap(),
    )
    .unwrap();

    let err = core
        .resolve_note_id("aaaaaaaa")
        .expect_err("should be ambiguous");
    assert!(
        matches!(err, NoteSelectorError::Ambiguous(prefix, matches) if prefix == "aaaaaaaa" && matches.len() == 2)
    );
}

#[test]
fn test_17_symlink_note_is_refused() {
    let (root, core) = synthetic_core();
    let secret = root.path().join("secret.txt");
    fs::write(&secret, "sensitive content").expect("write secret");

    let id = Uuid::new_v4();
    let symlink_path = root
        .path()
        .join("data/note-it/notes")
        .join(format!("{id}.md"));
    #[cfg(unix)]
    std::os::unix::fs::symlink(&secret, &symlink_path).expect("create symlink");

    let err = core.read_note(&id).expect_err("symlink must be refused");
    assert!(err.contains("link simbólico"));
}

#[test]
fn test_18_read_missing_note_returns_error() {
    let (_root, core) = synthetic_core();
    let non_existent = Uuid::new_v4();
    let err = core.read_note(&non_existent).expect_err("missing note");
    assert!(err.contains("não encontrada"));
}

#[test]
fn test_19_malformed_front_matter_does_not_crash_global_listing_and_returns_typed_warning() {
    let (root, core) = synthetic_core();
    let id1 = Uuid::new_v4();
    let malformed = "---\nmalformed: [unclosed yaml\n---\n\n# Quebrada\n";
    let notes_dir = root.path().join("data/note-it/notes");
    fs::write(notes_dir.join(format!("{id1}.md")), malformed).unwrap();

    let mut valid_note = NoteDocument::new_empty();
    valid_note.content = "# Válida\n".into();
    core.storage().save_note_atomic(&valid_note).unwrap();

    let batch = core
        .list_summaries(&NoteFilter::default(), None)
        .expect("list");
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].id, valid_note.metadata.id);
    assert_eq!(batch.warnings.len(), 1);
    assert_eq!(batch.warnings[0].note_id, Some(id1));
    assert_eq!(batch.warnings[0].kind, ReadWarningKind::UnreadableNote);
}

#[test]
fn test_20_large_note_reading_and_search() {
    let (_root, core) = synthetic_core();
    let mut large = NoteDocument::new_empty();
    let mut body = "# Grande Documento\n\n".to_string();
    for i in 0..1000 {
        body.push_str(&format!("Linha {i} de conteúdo de teste.\n"));
    }
    body.push_str("PalavraRaraNoFinal");
    large.content = body;
    core.storage().save_note_atomic(&large).expect("save large");

    let read_back = core.read_note(&large.metadata.id).expect("read");
    assert_eq!(read_back.content.len(), large.content.len());

    let results = core.search_notes("PalavraRaraNoFinal");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note_id, large.metadata.id);
}

#[test]
fn test_21_task_pending_extraction() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "# Tarefas\n\n- [ ] Fazer compras\n".to_string();
    core.storage().save_note_atomic(&note).unwrap();

    let batch = core
        .list_tasks(TaskStateFilter::Pending, &NoteFilter::default(), None)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].text, "Fazer compras");
    assert!(!batch.items[0].checked);
    assert_eq!(batch.items[0].completed_at, None);
}

#[test]
fn test_22_task_completed_lowercase_x() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "- [x] Tarefa feita\n".to_string();
    core.storage().save_note_atomic(&note).unwrap();

    let batch = core
        .list_tasks(TaskStateFilter::Completed, &NoteFilter::default(), None)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].text, "Tarefa feita");
    assert!(batch.items[0].checked);
}

#[test]
fn test_23_task_completed_uppercase_x() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "- [X] Tarefa maiuscula\n".to_string();
    core.storage().save_note_atomic(&note).unwrap();

    let batch = core
        .list_tasks(TaskStateFilter::Completed, &NoteFilter::default(), None)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].text, "Tarefa maiuscula");
    assert!(batch.items[0].checked);
}

#[test]
fn test_24_valid_completed_at_parsed() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content =
        "- [x] Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->\n"
            .to_string();
    core.storage().save_note_atomic(&note).unwrap();

    let batch = core
        .list_tasks(TaskStateFilter::Completed, &NoteFilter::default(), None)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].text, "Comprar material");
    assert!(batch.items[0].completed_at.is_some());
}

#[test]
fn test_25_invalid_completed_at_yields_none() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "- [x] Data invalida <!-- note-it:completed_at=invalido -->\n".to_string();
    core.storage().save_note_atomic(&note).unwrap();

    let batch = core
        .list_tasks(TaskStateFilter::Completed, &NoteFilter::default(), None)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].completed_at, None);
}

#[test]
fn test_26_completed_task_without_timestamp_stays_unknown() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "- [x] Sem timestamp\n".to_string();
    core.storage().save_note_atomic(&note).unwrap();

    let batch = core
        .list_tasks(TaskStateFilter::Completed, &NoteFilter::default(), None)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].completed_at, None);
}

#[test]
fn test_27_nested_task_depth_preserved() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "- [ ] Pai\n  - [ ] Filho nível 1\n    - [ ] Neto nível 2\n".to_string();
    core.storage().save_note_atomic(&note).unwrap();

    let batch = core
        .list_tasks(TaskStateFilter::All, &NoteFilter::default(), None)
        .unwrap();
    assert_eq!(batch.items.len(), 3);
    assert_eq!(batch.items[0].depth, 0);
    assert_eq!(batch.items[1].depth, 1);
    assert_eq!(batch.items[2].depth, 2);
}

#[test]
fn test_28_fenced_code_fake_task_ignored() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "\
# Exemplo

```markdown
- [ ] isto é fake dentro de código
```

- [ ] Tarefa real
"
    .to_string();
    core.storage().save_note_atomic(&note).unwrap();

    let batch = core
        .list_tasks(TaskStateFilter::All, &NoteFilter::default(), None)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].text, "Tarefa real");
}

#[test]
fn test_29_front_matter_fake_task_ignored() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.user_metadata = NoteMetadata::try_new(
        [],
        [NoteProperty {
            key: "obs".into(),
            value: "- [ ] não é tarefa".into(),
        }],
    )
    .unwrap();
    note.content = "- [ ] Tarefa real no corpo\n".to_string();
    core.storage().save_note_atomic(&note).unwrap();

    let batch = core
        .list_tasks(TaskStateFilter::All, &NoteFilter::default(), None)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].text, "Tarefa real no corpo");
}

#[test]
fn test_30_task_filtering_by_metadata() {
    let (_root, core) = synthetic_core();
    let mut n1 = NoteDocument::new_empty();
    n1.user_metadata = NoteMetadata::try_new(["Medicina".into()], []).unwrap();
    n1.content = "- [ ] Tarefa medicina\n".to_string();
    core.storage().save_note_atomic(&n1).unwrap();

    let mut n2 = NoteDocument::new_empty();
    n2.user_metadata = NoteMetadata::try_new(["Projeto".into()], []).unwrap();
    n2.content = "- [ ] Tarefa projeto\n".to_string();
    core.storage().save_note_atomic(&n2).unwrap();

    let filter = NoteFilter::new(vec!["Medicina".into()], vec![]);
    let batch = core
        .list_tasks(TaskStateFilter::All, &filter, None)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].text, "Tarefa medicina");
}

#[test]
fn test_31_trash_listing_remains_read_only() {
    let (_root, core) = synthetic_core();
    let mut note = NoteDocument::new_empty();
    note.content = "# Nota para lixeira\n".to_string();
    core.storage().save_note_atomic(&note).unwrap();
    core.storage()
        .move_note_to_trash(&note.metadata.id)
        .unwrap();

    let trash = core.list_trash();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].note_id, note.metadata.id);
    assert_eq!(trash[0].label, "Nota para lixeira");

    // Calling list_trash again doesn't change anything
    let trash2 = core.list_trash();
    assert_eq!(trash, trash2);
}

#[test]
fn test_32_performance_1000_notes() {
    let (_root, core) = synthetic_core();
    let count = 1000;

    for i in 0..count {
        let mut note = NoteDocument::new_empty();
        note.content = format!(
            "# Nota {i}\n\nConteúdo da nota {i} sobre Medicina e cardiologia.\n- [ ] Tarefa pendente {i}\n- [x] Tarefa feita {i} <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->\n"
        );
        if i % 3 == 0 {
            note.user_metadata = NoteMetadata::try_new(
                ["Medicina".into()],
                [NoteProperty {
                    key: "disciplina".into(),
                    value: "cardiologia".into(),
                }],
            )
            .unwrap();
        }
        core.storage().save_note_atomic(&note).unwrap();
    }

    let start_list = std::time::Instant::now();
    let summaries_batch = core
        .list_summaries(&NoteFilter::default(), Some(20))
        .unwrap();
    let list_time = start_list.elapsed();
    assert_eq!(summaries_batch.items.len(), 20);

    let start_search = std::time::Instant::now();
    let search_batch = core
        .search_notes_filtered("cardiologia", &NoteFilter::default(), Some(20))
        .unwrap();
    let search_time = start_search.elapsed();
    assert_eq!(search_batch.items.len(), 20);

    let start_filter = std::time::Instant::now();
    let filter = NoteFilter::new(
        vec!["Medicina".into()],
        vec![("disciplina".into(), "cardiologia".into())],
    );
    let filter_batch = core.list_summaries(&filter, Some(20)).unwrap();
    let filter_time = start_filter.elapsed();
    assert_eq!(filter_batch.items.len(), 20);

    let start_tasks = std::time::Instant::now();
    let tasks_batch = core
        .list_tasks(TaskStateFilter::Pending, &NoteFilter::default(), Some(20))
        .unwrap();
    let tasks_time = start_tasks.elapsed();
    assert_eq!(tasks_batch.items.len(), 20);

    let start_cat = std::time::Instant::now();
    let catalog = core.metadata_catalog();
    let cat_time = start_cat.elapsed();
    assert!(!catalog.tags.is_empty());

    println!(
        "1,000 notes performance in debug: list={list_time:?}, search={search_time:?}, filter={filter_time:?}, tasks={tasks_time:?}, catalog={cat_time:?}"
    );
}

#[test]
fn test_33_read_api_modules_have_zero_print_statements() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let pure_read_modules = [
        "src/lib.rs",
        "src/filter.rs",
        "src/task.rs",
        "src/warning.rs",
        "src/model.rs",
        "src/search.rs",
        "src/metadata.rs",
        "src/visible_text.rs",
    ];

    for file in pure_read_modules {
        let path = std::path::Path::new(manifest_dir).join(file);
        let content = fs::read_to_string(&path).expect("read module");
        assert!(
            !content.contains("println!"),
            "Module {file} must not contain println!"
        );
        assert!(
            !content.contains("eprintln!"),
            "Module {file} must not contain eprintln!"
        );
    }

    // Verify storage.rs read functions have zero prints
    let storage_path = std::path::Path::new(manifest_dir).join("src/storage.rs");
    let storage_content = fs::read_to_string(&storage_path).expect("read storage.rs");

    // The read_bodies function must not contain eprintln
    if let Some(pos) = storage_content.find("fn read_bodies(") {
        let snippet = &storage_content[pos..pos + 500.min(storage_content.len() - pos)];
        assert!(
            !snippet.contains("eprintln!"),
            "read_bodies in storage.rs must not contain eprintln!"
        );
    }
}

#[test]
fn test_34_unfiltered_search_with_unreadable_note_returns_typed_warning_and_valid_results() {
    let (root, core) = synthetic_core();

    // Valid note A
    let mut note_a = NoteDocument::new_empty();
    note_a.content = "# Choque séptico\nTratamento e fisiopatologia de sepse.".to_string();
    core.storage().save_note_atomic(&note_a).unwrap();

    // Corrupted note
    let id_bad = Uuid::new_v4();
    let malformed = "---\nmalformed: [unclosed yaml\n---\n\n# Quebrada\n";
    let notes_dir = root.path().join("data/note-it/notes");
    fs::write(notes_dir.join(format!("{id_bad}.md")), malformed).unwrap();

    // Valid note B
    let mut note_b = NoteDocument::new_empty();
    note_b.content = "# Protocolo de sepse\nUso precoce de antibióticos na sepse.".to_string();
    core.storage().save_note_atomic(&note_b).unwrap();

    let batch = core
        .search_notes_filtered("sepse", &NoteFilter::default(), None)
        .expect("search batch");

    assert_eq!(
        batch.items.len(),
        2,
        "Both valid notes A and B must be found"
    );
    assert_eq!(batch.items[0].note_id, note_b.metadata.id);
    assert_eq!(batch.items[1].note_id, note_a.metadata.id);

    assert_eq!(
        batch.warnings.len(),
        1,
        "Unreadable note must produce a typed warning"
    );
    assert_eq!(batch.warnings[0].note_id, Some(id_bad));
    assert_eq!(batch.warnings[0].kind, ReadWarningKind::UnreadableNote);
}

#[test]
fn test_35_filtered_search_with_unreadable_note_returns_same_warning_policy() {
    let (root, core) = synthetic_core();

    // Note A with tag Medicina
    let mut note_a = NoteDocument::new_empty();
    note_a.content = "# Choque séptico\nFisiopatologia de sepse.".to_string();
    note_a.user_metadata = NoteMetadata::try_new(["Medicina".into()], []).unwrap();
    core.storage().save_note_atomic(&note_a).unwrap();

    // Corrupted note
    let id_bad = Uuid::new_v4();
    let malformed = "---\nmalformed: [unclosed yaml\n---\n\n# Quebrada\n";
    let notes_dir = root.path().join("data/note-it/notes");
    fs::write(notes_dir.join(format!("{id_bad}.md")), malformed).unwrap();

    // Note B with tag Projeto (does not match filter)
    let mut note_b = NoteDocument::new_empty();
    note_b.content = "# Projeto sepse\nSepse em software.".to_string();
    note_b.user_metadata = NoteMetadata::try_new(["Projeto".into()], []).unwrap();
    core.storage().save_note_atomic(&note_b).unwrap();

    let filter = NoteFilter::new(vec!["Medicina".into()], vec![]);
    let batch = core
        .search_notes_filtered("sepse", &filter, None)
        .expect("search batch");

    assert_eq!(batch.items.len(), 1, "Only note A matches filter Medicina");
    assert_eq!(batch.items[0].note_id, note_a.metadata.id);

    assert_eq!(
        batch.warnings.len(),
        1,
        "Warning policy must be identical to unfiltered search"
    );
    assert_eq!(batch.warnings[0].note_id, Some(id_bad));
    assert_eq!(batch.warnings[0].kind, ReadWarningKind::UnreadableNote);
}

#[test]
fn test_36_search_scans_full_eligible_universe_before_applying_limit() {
    let (_root, core) = synthetic_core();

    // 10 older notes containing "RaroTermo"
    let mut rare_ids = Vec::new();
    for i in 0..10 {
        let mut note = NoteDocument::new_empty();
        note.content = format!("# Nota Rara {i}\nContém o RaroTermo especial.");
        core.storage().save_note_atomic(&note).unwrap();
        rare_ids.push(note.metadata.id);
    }

    // 40 newer notes with generic content
    for i in 0..40 {
        let mut note = NoteDocument::new_empty();
        note.content = format!("# Nota Comum {i}\nConteúdo comum sem a palavra.");
        core.storage().save_note_atomic(&note).unwrap();
    }

    // If limit occurred before scanning (e.g. taking top 20 notes first), 0 matches would be found
    let batch = core
        .search_notes_filtered("RaroTermo", &NoteFilter::default(), Some(5))
        .expect("search");

    assert_eq!(
        batch.items.len(),
        5,
        "Must find matches across the full universe and limit final results to 5"
    );
    for item in &batch.items {
        assert!(rare_ids.contains(&item.note_id));
    }
}
