//! Regressões comportamentais da Fase 5.0D.3 contra a baseline 5.0D.2.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use noteit_core::{
    authority::perform_at,
    write::{NoteDraft, WriteOperation},
    NoteItCore, StorePaths, Uuid,
};
use noteit_tui::app::{ActivePanel, App, EditorPrompt, Focus};
use ratatui::{backend::TestBackend, Terminal};
use std::sync::{atomic::AtomicBool, Arc};

fn store(root: &std::path::Path, runtime: &std::path::Path, body: &str) -> (StorePaths, Uuid) {
    let paths = StorePaths::from_custom_paths(
        root.join("notes"),
        root.join("config"),
        root.join("state"),
        runtime.to_path_buf(),
    );
    let written = perform_at(
        &paths,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: body.into(),
                ..Default::default()
            },
        },
    )
    .unwrap();
    (paths, written.outcome.note_id)
}

#[test]
fn product_header_has_no_obsolete_phase_label() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "# Nota\n\nTexto");
    let app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    app.draw(&mut terminal).unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();

    assert!(rendered.contains("NOTE-IT — Interface de Terminal"));
    assert!(!rendered.contains("Fase 5.0B / 5.0C"));
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn alt_f(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT));
}
fn enter(app: &mut App) {
    app.handle_key(KeyEvent::from(KeyCode::Enter));
}
fn down(app: &mut App) {
    app.handle_key(KeyEvent::from(KeyCode::Down));
}

fn choose_red(app: &mut App) {
    alt_f(app);
    enter(app);
    down(app);
    down(app);
    enter(app);
}
fn choose_blue(app: &mut App) {
    alt_f(app);
    enter(app);
    for _ in 0..6 {
        down(app);
    }
    enter(app);
}
fn choose_yellow_highlight(app: &mut App) {
    alt_f(app);
    down(app);
    enter(app);
    down(app);
    enter(app);
}

#[test]
fn mouse_tabs_and_rows_use_the_keyboard_navigation_state() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "primeira");
    perform_at(
        &paths,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "segunda".into(),
                ..Default::default()
            },
        },
    )
    .unwrap();
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);

    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 45, 1), area);
    assert_eq!(app.panel, ActivePanel::Trash);
    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 1), area);
    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 4), area);
    assert_eq!(app.focus, Focus::Editor);
}

#[test]
fn dirty_draft_mouse_navigation_asks_before_switching() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "original");
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    app.handle_key(KeyEvent::from(KeyCode::Char('X')));
    app.handle_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), 45, 1),
        ratatui::layout::Rect::new(0, 0, 80, 24),
    );

    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(app.editor_prompt, Some(EditorPrompt::Pending));
    assert_eq!(app.panel, ActivePanel::RecentNotes);
}

#[test]
fn formatting_palette_authors_gui_compatible_markup_and_undoes() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, id) = store(root.path(), &runtime, "ação 日本語");
    let mut app = App::new_at(paths.clone(), Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    app.handle_key(KeyEvent::from(KeyCode::Down));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.pending_text().unwrap().contains(
        "<span data-note-it-color=\"#64748B\" style=\"color:#64748B\">ação 日本語</span>"
    ));

    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    assert!(app.pending_text().is_none());
    app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(NoteItCore::open_read_only_at(paths)
        .read_note(&id)
        .unwrap()
        .content
        .contains("data-note-it-color"));
}

#[test]
fn existing_selection_highlight_applies_immediately_without_changing_future_style() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "linha um\nlinha dois");
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    enter(&mut app);
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    choose_yellow_highlight(&mut app);
    assert!(app.pending_text().unwrap().contains(
        "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">linha um\nlinha dois</mark>"
    ));
    assert_eq!(app.active_highlight, None);
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    assert!(app.pending_text().is_none());
}

#[test]
fn future_styled_typing_undoes_and_redoes_as_normal_draft_history() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "");
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    enter(&mut app);
    choose_red(&mut app);
    for c in "ação".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    let styled = app.pending_text().unwrap().to_owned();
    app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    assert!(app.pending_text().is_none());
    app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
    assert_eq!(app.pending_text(), Some(styled));
}

#[test]
fn representative_terminal_sizes_never_panic_with_format_menu() {
    for (width, height) in [(40, 12), (80, 24), (140, 34)] {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join("runtime");
        let (paths, _) = store(
            root.path(),
            &runtime,
            &format!("# Título\n\n{}\n日本語 👩‍💻", "palavra ".repeat(80)),
        );
        let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        app.draw(&mut terminal).unwrap();
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT));
        app.draw(&mut terminal).unwrap();
    }
}

#[test]
fn reader_selection_overlay_preserves_semantic_foreground_and_background() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let body = "<mark data-note-it-highlight=\"#FDE68A\"><span data-note-it-color=\"#DC2626\">X</span></mark>";
    let (paths, _) = store(root.path(), &runtime, body);
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    app.focus = Focus::Reader;
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    app.draw(&mut terminal).unwrap();
    let cell = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .find(|cell| {
            cell.symbol() == "X" && cell.bg == ratatui::style::Color::Rgb(0xFD, 0xE6, 0x8A)
        })
        .unwrap();
    assert_eq!(cell.fg, ratatui::style::Color::Rgb(0xDC, 0x26, 0x26));
    assert_eq!(cell.bg, ratatui::style::Color::Rgb(0xFD, 0xE6, 0x8A));
    assert!(cell.modifier.contains(ratatui::style::Modifier::REVERSED));
}

#[test]
fn formatting_without_selection_configures_future_typing() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "");
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    app.handle_key(KeyEvent::from(KeyCode::Down));
    app.handle_key(KeyEvent::from(KeyCode::Down));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    app.handle_key(KeyEvent::from(KeyCode::Char('a')));
    app.handle_key(KeyEvent::from(KeyCode::Char('b')));
    app.handle_key(KeyEvent::from(KeyCode::Char('c')));
    assert!(app
        .pending_text()
        .unwrap()
        .contains("<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">abc</span>"));
}

#[test]
fn formatting_cancel_is_a_clean_noop_and_restores_context_help() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "texto");
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT));
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.pending_text().is_none());
    let mut terminal = Terminal::new(TestBackend::new(70, 20)).unwrap();
    app.draw(&mut terminal).unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(rendered.contains("Formatar"));
    assert!(!rendered.contains("Selecione texto"));
}

#[test]
fn task_panel_space_toggles_selected_task_without_losing_other_content() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, id) = store(
        root.path(),
        &runtime,
        "- [ ] **comprar pão**\n\nconteúdo intacto",
    );
    let mut app = App::new_at(paths.clone(), Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Char('2')));
    app.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    let stored = NoteItCore::open_read_only_at(paths)
        .read_note(&id)
        .unwrap()
        .content;
    assert!(stored.contains("- [x] **comprar pão**"));
    assert!(stored.contains("conteúdo intacto"));
}

#[test]
fn intermediate_width_editor_footer_keeps_save_format_and_exit_whole() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "texto");
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Enter));
    for width in [50, 60, 70, 80, 100, 120] {
        let mut terminal = Terminal::new(TestBackend::new(width, 20)).unwrap();
        app.draw(&mut terminal).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(rendered.contains("Salvar"), "{width}: {rendered}");
        assert!(rendered.contains("Formatar"), "{width}: {rendered}");
        assert!(rendered.contains("Sair"), "{width}: {rendered}");
    }
}

#[test]
fn future_color_highlight_compose_switch_and_reset_in_canonical_runs() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "");
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    enter(&mut app);
    choose_red(&mut app);
    choose_yellow_highlight(&mut app);
    for c in "abc".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    choose_blue(&mut app);
    for c in "def".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    alt_f(&mut app);
    down(&mut app);
    down(&mut app);
    enter(&mut app); // limpar cor
    for c in "ghi".chars() {
        app.handle_key(KeyEvent::from(KeyCode::Char(c)));
    }
    alt_f(&mut app);
    for _ in 0..3 {
        down(&mut app);
    }
    enter(&mut app); // limpar marca
    app.handle_key(KeyEvent::from(KeyCode::Char('j')));
    let text = app.pending_text().unwrap();
    assert!(text.contains("data-note-it-color=\"#DC2626\"") && text.contains(">abc</span></mark>"));
    assert!(
        text.contains("data-note-it-color=\"#2563EB\"") && text.contains(">def</span>ghi</mark>"),
        "{text}"
    );
    assert!(text.contains("</span>ghi</mark>j"));
    assert!(!text.contains("</span><span data-note-it-color=\"#DC2626\""));
}

#[test]
fn active_style_indicator_tracks_both_styles_and_clears() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, _) = store(root.path(), &runtime, "");
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    enter(&mut app);
    choose_red(&mut app);
    choose_yellow_highlight(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(70, 20)).unwrap();
    app.draw(&mut terminal).unwrap();
    let screen = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(screen.contains("Cor: Vermelho") && screen.contains("Marca: Amarelo"));
    alt_f(&mut app);
    down(&mut app);
    down(&mut app);
    enter(&mut app);
    assert_eq!(app.active_text_color, None);
    assert_eq!(app.active_highlight, Some("#FDE68A"));
    alt_f(&mut app);
    for _ in 0..3 {
        down(&mut app);
    }
    enter(&mut app);
    assert_eq!(app.active_highlight, None);
}

#[test]
fn rendered_task_rows_hide_formatting_tokens() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let body = "- [ ] **bold** ***both*** ~~strike~~ <u>under</u> <span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">color</span> <mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">mark</mark> `code`";
    let (paths, _) = store(root.path(), &runtime, body);
    let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Char('2')));
    let mut terminal = Terminal::new(TestBackend::new(140, 24)).unwrap();
    app.draw(&mut terminal).unwrap();
    let screen = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    for leaked in [
        "<span",
        "</span>",
        "<mark",
        "</mark>",
        "<u>",
        "</u>",
        "**bold**",
        "~~strike~~",
        "`code`",
    ] {
        assert!(!screen.contains(leaked), "vazou {leaked}: {screen}");
    }
}

#[test]
fn task_checkbox_click_toggles_but_task_text_click_opens() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, id) = store(root.path(), &runtime, "- [ ] tarefa");
    let mut app = App::new_at(paths.clone(), Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Char('2')));
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 3, 4), area);
    assert!(NoteItCore::open_read_only_at(paths.clone())
        .read_note(&id)
        .unwrap()
        .content
        .contains("[x] tarefa"));
    app.reload_all();
    // A new pending task lets the row-text behavior be tested independently.
    perform_at(
        &paths,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "- [ ] outra".into(),
                ..Default::default()
            },
        },
    )
    .unwrap();
    app.reload_all();
    app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 8, 4), area);
    assert_eq!(app.focus, Focus::Editor);
}

#[test]
fn stale_task_toggle_preserves_external_edit_and_reports_conflict() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    let (paths, id) = store(root.path(), &runtime, "- [ ] tarefa");
    let mut app = App::new_at(paths.clone(), Arc::new(AtomicBool::new(false)));
    app.handle_key(KeyEvent::from(KeyCode::Char('2')));
    let old = NoteItCore::open_read_only_at(paths.clone())
        .read_note(&id)
        .unwrap();
    perform_at(
        &paths,
        &WriteOperation::MutateNote {
            selector: id.to_string(),
            expected_revision: Some(noteit_core::write::revision_of(&old).unwrap()),
            mutation: noteit_core::write::NoteMutation::Append {
                payload: "externo".into(),
            },
        },
    )
    .unwrap();
    app.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    let stored = NoteItCore::open_read_only_at(paths)
        .read_note(&id)
        .unwrap()
        .content;
    assert!(stored.contains("[ ] tarefa") && stored.contains("externo"));
    assert!(app.notice.contains("Conflito"));
}
