use crossterm::event::{KeyCode, KeyEvent};
use noteit_core::{
    chrono::{Duration as ChronoDuration, Utc},
    metadata::NoteTags,
    model::NoteDocument,
    StorePaths, Uuid,
};
use noteit_tui::app::{ActivePanel, App, Focus};
use ratatui::{backend::TestBackend, Terminal};
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test store populated with synthetic notes, tasks, and trash items.
struct TestFixture {
    _temp: TempDir,
    paths: StorePaths,
    note1_id: Uuid,
    note2_id: Uuid,
    note3_id: Uuid,
    trash_id: Uuid,
}

impl TestFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let base = temp.path().join("note-it");
        let notes_dir = base.join("notes");
        let trash_dir = base.join("trash");
        fs::create_dir_all(&notes_dir).expect("create notes dir");
        fs::create_dir_all(&trash_dir).expect("create trash dir");

        let paths = StorePaths {
            data_dir: base.clone(),
            notes_dir: notes_dir.clone(),
            trash_dir: trash_dir.clone(),
            backups_dir: base.join("backup"),
            assets_dir: base.join("assets"),
            config_dir: base.join("config"),
            state_dir: base.join("state"),
            runtime_dir: base.join("runtime"),
        };

        let now = Utc::now();

        // Note 1: Architecture guide with rich Markdown formatting
        let note1_id = Uuid::new_v4();
        let note1_content = "\
# Guia de Arquitetura

Este documento descreve os componentes do Note-it.

## Visão Geral

- Item Alpha
- Item Beta
1. Primeiro passo
2. Segundo passo

### Tarefas da Arquitetura

- [ ] Implementar modo leitura
- [x] Construir portão de fronteira <!-- note-it:completed_at=2026-09-06T12:00:00Z -->

### Citações e Alertas GFM

> Citação simples inspiradora sobre software.

> [!NOTE]
> Esta é uma nota explicativa importante.

> [!TIP]
> Dica de performance: utilize TestBackend para asserções sem tela física.

> [!IMPORTANT]
> Importante: o Core é a autoridade única e soberana sobre os dados.

> [!WARNING]
> Atenção: nunca retenha lease de escrita persistente na TUI.

> [!CAUTION]
> Cuidado: manipulações atômicas de disco devem passar pelo Core.

### Bloco de Código

```rust
fn execute_task() -> Result<(), String> {
    println!(\"Operação segura\");
    Ok(())
}
```

### Linha de Cálculo

= 100 * 2.5
taxa := 0.15
";
        let mut doc1 = NoteDocument::new_with_id(note1_id);
        doc1.content = note1_content.to_string();
        doc1.metadata.created_at = Some(now - ChronoDuration::hours(5));
        doc1.metadata.updated_at = Some(now - ChronoDuration::minutes(10));
        doc1.user_metadata.tags =
            NoteTags::try_new(vec!["arquitetura".to_string(), "tui".to_string()]).unwrap();
        fs::write(
            notes_dir.join(format!("{note1_id}.md")),
            doc1.serialize().unwrap(),
        )
        .unwrap();

        // Note 2: Tasks sprint
        let note2_id = Uuid::new_v4();
        let note2_content = "\
# Sprint Backlog

Lista de prioridades da semana.

- [ ] Revisar testes de regressão
- [ ] Validar conformidade de empacotamento
- [x] Fechar Fase 5.0B
";
        let mut doc2 = NoteDocument::new_with_id(note2_id);
        doc2.content = note2_content.to_string();
        doc2.metadata.created_at = Some(now - ChronoDuration::hours(2));
        doc2.metadata.updated_at = Some(now - ChronoDuration::minutes(30));
        doc2.user_metadata.tags = NoteTags::try_new(vec!["sprint".to_string()]).unwrap();
        fs::write(
            notes_dir.join(format!("{note2_id}.md")),
            doc2.serialize().unwrap(),
        )
        .unwrap();

        // Note 3: Older quick notes
        let note3_id = Uuid::new_v4();
        let note3_content = "\
# Anotações Rápidas

Cálculos e rascunhos diários.

= 2 + 2
";
        let mut doc3 = NoteDocument::new_with_id(note3_id);
        doc3.content = note3_content.to_string();
        doc3.metadata.created_at = Some(now - ChronoDuration::days(1));
        doc3.metadata.updated_at = Some(now - ChronoDuration::hours(1));
        fs::write(
            notes_dir.join(format!("{note3_id}.md")),
            doc3.serialize().unwrap(),
        )
        .unwrap();

        // Note in Trash: deleted note
        let trash_id = Uuid::new_v4();
        let trash_content = "\
# Nota Descartada

Esta nota foi movida para a lixeira anteriormente.
";
        let mut trash_doc = NoteDocument::new_with_id(trash_id);
        trash_doc.content = trash_content.to_string();
        trash_doc.metadata.created_at = Some(now - ChronoDuration::days(2));
        trash_doc.metadata.updated_at = Some(now - ChronoDuration::days(1));
        fs::write(
            trash_dir.join(format!("{trash_id}.md")),
            trash_doc.serialize().unwrap(),
        )
        .unwrap();

        // Sidecar for trash
        let sidecar = format!(
            r#"{{"version":1,"deleted_at":"{}"}}"#,
            (now - ChronoDuration::hours(3)).to_rfc3339()
        );
        fs::write(trash_dir.join(format!("{trash_id}.json")), sidecar).unwrap();

        Self {
            _temp: temp,
            paths,
            note1_id,
            note2_id,
            note3_id,
            trash_id,
        }
    }

    fn create_app(&self) -> App {
        let flag = Arc::new(AtomicBool::new(false));
        App::new_at(self.paths.clone(), flag)
    }
}

/// Helper to extract rendered string lines from a TestBackend buffer.
fn buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

#[test]
fn test_recency_ordering_follows_updated_at() {
    let fixture = TestFixture::new();
    let app = fixture.create_app();

    assert_eq!(app.recent_notes.len(), 3);
    // Canonical recency: Note 1 (-10 min) > Note 2 (-30 min) > Note 3 (-1 hour)
    assert_eq!(app.recent_notes[0].id, fixture.note1_id);
    assert_eq!(app.recent_notes[1].id, fixture.note2_id);
    assert_eq!(app.recent_notes[2].id, fixture.note3_id);
}

#[test]
fn test_panel_navigation_shortcuts() {
    let fixture = TestFixture::new();
    let mut app = fixture.create_app();

    // Starts on RecentNotes
    assert_eq!(app.panel, ActivePanel::RecentNotes);

    // Tab -> PendingTasks
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.panel, ActivePanel::PendingTasks);

    // Tab -> Trash
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.panel, ActivePanel::Trash);

    // Tab -> wraps back to RecentNotes
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.panel, ActivePanel::RecentNotes);

    // Direct jump keys: 2, 3, 1
    app.handle_key(KeyEvent::from(KeyCode::Char('2')));
    assert_eq!(app.panel, ActivePanel::PendingTasks);

    app.handle_key(KeyEvent::from(KeyCode::Char('3')));
    assert_eq!(app.panel, ActivePanel::Trash);

    app.handle_key(KeyEvent::from(KeyCode::Char('1')));
    assert_eq!(app.panel, ActivePanel::RecentNotes);
}

#[test]
fn test_pending_tasks_extraction_and_opening() {
    let fixture = TestFixture::new();
    let mut app = fixture.create_app();

    // Switch to PendingTasks
    app.handle_key(KeyEvent::from(KeyCode::Char('2')));
    assert_eq!(app.panel, ActivePanel::PendingTasks);

    // Ensure pending tasks were extracted without completed ones
    // Note 1 has: "- [ ] Implementar modo leitura" (pending)
    // Note 2 has: "- [ ] Revisar testes de regressão" (pending), "- [ ] Validar conformidade de empacotamento" (pending)
    // Completed tasks "- [x]" are excluded by TaskStateFilter::Pending
    assert_eq!(app.pending_tasks.len(), 3);
    assert!(app
        .pending_tasks
        .iter()
        .any(|t| t.text == "Implementar modo leitura"));
    assert!(app
        .pending_tasks
        .iter()
        .any(|t| t.text == "Revisar testes de regressão"));
    assert!(app
        .pending_tasks
        .iter()
        .any(|t| t.text == "Validar conformidade de empacotamento"));

    // Enter opens the task's parent note to be worked on: since Fase 5.0D.2
    // that is the native editor, with no second key.
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.focus, Focus::Editor);
    assert!(app.current_note.is_some());
    assert!(app.draft.is_some());
}

#[test]
fn test_trash_listing_and_preview() {
    let fixture = TestFixture::new();
    let mut app = fixture.create_app();

    // Switch to Trash
    app.handle_key(KeyEvent::from(KeyCode::Char('3')));
    assert_eq!(app.panel, ActivePanel::Trash);
    assert_eq!(app.trash_items.len(), 1);
    assert_eq!(app.trash_items[0].note_id, fixture.trash_id);
    assert_eq!(app.trash_items[0].label, "Nota Descartada");

    // Open trash note preview
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.focus, Focus::Reader);
}

#[test]
fn test_quick_search_delegates_to_core() {
    let fixture = TestFixture::new();
    let mut app = fixture.create_app();

    // Press '/' to activate search
    app.handle_key(KeyEvent::from(KeyCode::Char('/')));
    assert_eq!(app.focus, Focus::Search);
    assert_eq!(app.search_query, "");

    // Type query "arquitetura"
    for c in "arquitetura".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    assert_eq!(app.search_query, "arquitetura");

    // Must return match for Note 1
    assert_eq!(app.search_results.len(), 1);
    assert_eq!(app.search_results[0].note_id, fixture.note1_id);

    // Press Enter to select and open matching note, which opens it for work.
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(app.current_note_id, Some(fixture.note1_id));
}

#[test]
fn test_rendered_buffer_contains_all_blocks_via_test_backend() {
    let fixture = TestFixture::new();
    let mut app = fixture.create_app();

    // Ensure Note 1 is selected and loaded
    app.load_note(fixture.note1_id);

    let backend = TestBackend::new(120, 80);
    let mut terminal = Terminal::new(backend).unwrap();

    app.draw(&mut terminal).unwrap();
    let buffer_text = buffer_to_string(&terminal);

    // 1. Headings
    assert!(
        buffer_text.contains("# Guia de Arquitetura"),
        "Buffer must contain H1"
    );
    assert!(
        buffer_text.contains("## Visão Geral"),
        "Buffer must contain H2"
    );
    assert!(
        buffer_text.contains("### Tarefas da Arquitetura"),
        "Buffer must contain H3"
    );

    // 2. Lists
    assert!(
        buffer_text.contains("• Item Alpha"),
        "Buffer must contain bullet item Alpha"
    );
    assert!(
        buffer_text.contains("• Item Beta"),
        "Buffer must contain bullet item Beta"
    );
    assert!(
        buffer_text.contains("1. Primeiro passo"),
        "Buffer must contain numbered item 1"
    );
    assert!(
        buffer_text.contains("2. Segundo passo"),
        "Buffer must contain numbered item 2"
    );

    // 3. Task Checkboxes
    assert!(
        buffer_text.contains("☐ Implementar modo leitura"),
        "Buffer must contain unchecked task"
    );
    assert!(
        buffer_text.contains("☑ Construir portão de fronteira"),
        "Buffer must contain completed task"
    );
    // Ensure HTML comment is never shown
    assert!(
        !buffer_text.contains("note-it:completed_at"),
        "Buffer must not contain note-it HTML comments"
    );

    // 4. Blockquotes and All 5 GFM Alerts
    assert!(
        buffer_text.contains("│ Citação simples"),
        "Buffer must contain standard blockquote"
    );
    assert!(
        buffer_text.contains("[NOTA]"),
        "Buffer must contain [NOTA] callout label"
    );
    assert!(
        buffer_text.contains("[DICA]"),
        "Buffer must contain [DICA] callout label"
    );
    assert!(
        buffer_text.contains("[IMPORTANTE]"),
        "Buffer must contain [IMPORTANTE] callout label"
    );
    assert!(
        buffer_text.contains("[AVISO]"),
        "Buffer must contain [AVISO] callout label"
    );
    assert!(
        buffer_text.contains("[CUIDADO]"),
        "Buffer must contain [CUIDADO] callout label"
    );

    // 5. Code Block
    assert!(
        buffer_text.contains("┌── [rust]"),
        "Buffer must contain code fence header"
    );
    assert!(
        buffer_text.contains("│ fn execute_task()"),
        "Buffer must contain code block line"
    );
    assert!(
        buffer_text.contains("└─────"),
        "Buffer must contain code fence footer"
    );

    // 6. Math lines rendered as plain text without calculation
    assert!(
        buffer_text.contains("= 100 * 2.5"),
        "Buffer must contain raw math line"
    );
    assert!(
        buffer_text.contains("taxa := 0.15"),
        "Buffer must contain raw declaration line"
    );
}

#[test]
fn test_dump_all_block_types_snapshot() {
    let fixture = TestFixture::new();
    let mut app = fixture.create_app();
    app.load_note(fixture.note1_id);

    let backend = TestBackend::new(100, 65);
    let mut terminal = Terminal::new(backend).unwrap();
    app.draw(&mut terminal).unwrap();
    let text = buffer_to_string(&terminal);

    println!("=== FULL BLOCKS RENDER DUMP (Height 50) ===");
    println!("{text}");
    println!("=== END FULL BLOCKS DUMP ===");

    assert!(text.contains("[NOTA]"));
    assert!(text.contains("[DICA]"));
    assert!(text.contains("[IMPORTANTE]"));
    assert!(text.contains("[AVISO]"));
    assert!(text.contains("[CUIDADO]"));
    assert!(text.contains("┌── [rust]"));
    assert!(text.contains("= 100 * 2.5"));
}
