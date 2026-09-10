//! Fase 5.0D.2 — o painel direito edita a nota, sem um segundo `e`.
//!
//! Estes testes descrevem o que um leitor vê e o que o store passa a conter.
//! São escritos contra a superfície pública da `App` — teclas entram, buffer e
//! Core saem — para que continuem válidos independentemente de como o editor é
//! estruturado por dentro.
//!
//! As notas são sintéticas e vivem em diretórios temporários. Nenhuma nota real
//! do usuário é lida, escrita ou copiada para cá.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use noteit_core::{
    authority::perform_at,
    write::{NoteDraft, NoteMutation, WriteOperation},
    NoteItCore, StorePaths, Uuid,
};
use noteit_tui::app::App;
use ratatui::{backend::TestBackend, Terminal};
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use support::{cleanup_coordination, store, Tui};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Harness em memória
// ---------------------------------------------------------------------------

struct Fixture {
    _root: TempDir,
    paths: StorePaths,
    id: Uuid,
    app: App,
    terminal: Terminal<TestBackend>,
}

impl Fixture {
    fn new(body: &str) -> Self {
        Self::sized(body, 120, 40)
    }

    fn sized(body: &str, columns: u16, rows: u16) -> Self {
        let root = tempfile::tempdir().unwrap();
        let paths = StorePaths::from_custom_paths(
            root.path().join("notes"),
            root.path().join("config"),
            root.path().join("state/note-it"),
            root.path().join("runtime"),
        );
        let created = perform_at(
            &paths,
            &WriteOperation::CreateNote {
                draft: NoteDraft {
                    content: body.into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let app = App::new_at(paths.clone(), Arc::new(AtomicBool::new(false)));
        Self {
            _root: root,
            paths,
            id: created.outcome.note_id,
            app,
            terminal: Terminal::new(TestBackend::new(columns, rows)).unwrap(),
        }
    }

    fn key(&mut self, code: KeyCode) {
        self.app.handle_key(KeyEvent::from(code));
    }

    fn ctrl(&mut self, character: char) {
        self.app.handle_key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        ));
    }

    fn shift(&mut self, code: KeyCode) {
        self.app
            .handle_key(KeyEvent::new(code, KeyModifiers::SHIFT));
    }

    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            if character == '\n' {
                self.key(KeyCode::Enter);
            } else {
                self.key(KeyCode::Char(character));
            }
        }
    }

    /// Enter on the selected note is how a note is opened to be worked on.
    fn open(&mut self) {
        self.key(KeyCode::Enter);
    }

    fn screen(&mut self) -> String {
        self.app.draw(&mut self.terminal).unwrap();
        let buffer = self.terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    /// Only the right-hand pane. The list panel spells a note's label too, so
    /// "the editor no longer shows this text" has to be asked of the editor.
    fn pane(&mut self) -> String {
        let screen = self.screen();
        let start = screen
            .lines()
            .find_map(|row| {
                ["╭ Edição", "╭ Leitura", "╭ Visualização"]
                    .iter()
                    .find_map(|title| row.find(title))
                    .map(|byte| row[..byte].chars().count())
            })
            .unwrap_or(0);
        screen
            .lines()
            .map(|row| row.chars().skip(start).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The pane without its title row, which carries the note's own label and
    /// would otherwise answer questions asked about the text being edited.
    fn body(&mut self) -> String {
        self.pane()
            .lines()
            .skip_while(|row| !row.starts_with('╭'))
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn stored(&self) -> String {
        NoteItCore::open_read_only_at(self.paths.clone())
            .read_note(&self.id)
            .unwrap()
            .content
    }

    /// A write by somebody else, exactly as the desktop or the CLI would do it.
    fn external_write(&self, body: &str) {
        let core = NoteItCore::open_read_only_at(self.paths.clone());
        let document = core.read_note(&self.id).unwrap();
        perform_at(
            &self.paths,
            &WriteOperation::MutateNote {
                selector: self.id.to_string(),
                expected_revision: Some(noteit_core::write::revision_of(&document).unwrap()),
                mutation: NoteMutation::ReplaceBody { body: body.into() },
            },
        )
        .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        cleanup_coordination(&self.paths);
    }
}

// ---------------------------------------------------------------------------
// Entrada no modo de edição
// ---------------------------------------------------------------------------

#[test]
fn opening_a_note_starts_editing_without_a_second_key() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    // The caret opens at the start of the note, the way opening a file does.
    fixture.type_text("X");
    assert!(
        fixture.pane().contains("Xoriginal"),
        "o caractere digitado não chegou ao painel:\n{}",
        fixture.pane()
    );

    fixture.key(KeyCode::End);
    fixture.type_text("Y");
    assert!(fixture.pane().contains("XoriginalY"), "{}", fixture.pane());
}

#[test]
fn the_editor_announces_itself_and_its_shortcuts() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    let screen = fixture.screen();

    assert!(
        screen.contains("Edição:"),
        "o painel não se identifica como edição:\n{screen}"
    );
    for shortcut in ["[Ctrl+S] Salvar", "[Ctrl+Z]", "[Esc]"] {
        assert!(
            screen.contains(shortcut),
            "atalho {shortcut:?} não está visível:\n{screen}"
        );
    }
}

#[test]
fn selecting_in_the_list_still_previews_the_rendered_note() {
    // Selecting is not the same as opening: the reader — and with it every
    // Markdown fidelity guarantee of 5.0D.1 — is what a selection shows.
    let mut fixture = Fixture::new("# t<span data-note-it-color=\"#DC2626\">ítulo</span>");
    let screen = fixture.screen();
    assert!(screen.contains("# título"), "{screen}");
    assert!(!screen.contains("<span"), "{screen}");
    assert!(!screen.contains("Edição:"), "{screen}");
}

#[test]
fn escape_steps_out_of_editing_into_reading_and_then_into_the_list() {
    let mut fixture = Fixture::new("# título");
    fixture.open();
    assert!(fixture.screen().contains("Edição:"));

    fixture.key(KeyCode::Esc);
    let reading = fixture.screen();
    assert!(
        reading.contains("Leitura:") && !reading.contains("Edição:"),
        "Esc sem alterações deve voltar à leitura:\n{reading}"
    );

    fixture.key(KeyCode::Esc);
    assert!(!fixture.app.should_quit, "Esc na leitura volta à lista");
}

// ---------------------------------------------------------------------------
// Edição do texto
// ---------------------------------------------------------------------------

#[test]
fn typing_inserts_unicode_and_multiline_text() {
    let mut fixture = Fixture::new("");
    fixture.open();
    fixture.type_text("café à noite\nação — ótimo\n日本語 🇧🇷");

    let pane = fixture.pane();
    for expected in ["café à noite", "ação — ótimo"] {
        assert!(pane.contains(expected), "falta {expected:?}:\n{pane}");
    }
    // A wide grapheme owns two cells, so the drawn row reads `日 本 語`. What
    // matters is that every one of them is on screen and that what the note
    // will hold is exactly what was typed.
    for glyph in ['日', '本', '語', '🇧'] {
        assert!(pane.contains(glyph), "falta {glyph:?}:\n{pane}");
    }
    fixture.ctrl('s');
    assert_eq!(fixture.stored(), "café à noite\nação — ótimo\n日本語 🇧🇷");
}

#[test]
fn backspace_and_delete_remove_one_character_and_join_lines() {
    let mut fixture = Fixture::new("");
    fixture.open();
    fixture.type_text("ação");
    fixture.key(KeyCode::Backspace);
    assert!(fixture.screen().contains("açã"), "{}", fixture.screen());

    fixture.type_text("\nsegunda");
    fixture.key(KeyCode::Home);
    fixture.key(KeyCode::Backspace);
    let screen = fixture.screen();
    assert!(
        screen.contains("açãsegunda"),
        "backspace no início junta as linhas:\n{screen}"
    );
}

#[test]
fn undo_and_redo_walk_the_edit_history_within_an_explicit_limit() {
    let mut fixture = Fixture::new("base");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" um");
    assert!(fixture.pane().contains("base um"));

    fixture.ctrl('z');
    let undone = fixture.pane();
    assert!(
        undone.contains("base") && !undone.contains("base um"),
        "Ctrl+Z não desfez:\n{undone}"
    );

    fixture.ctrl('y');
    assert!(
        fixture.pane().contains("base um"),
        "Ctrl+Y não refez:\n{}",
        fixture.pane()
    );
}

#[test]
fn a_selection_is_visible_and_replaced_by_what_is_typed_next() {
    let mut fixture = Fixture::new("apagar isto");
    fixture.open();
    fixture.ctrl('a');
    fixture.type_text("novo");

    let body = fixture.body();
    assert!(body.contains("novo"), "{body}");
    assert!(!body.contains("apagar isto"), "{body}");
}

#[test]
fn shift_arrows_extend_a_selection_that_backspace_removes_whole() {
    let mut fixture = Fixture::new("");
    fixture.open();
    fixture.type_text("abcdef");
    fixture.shift(KeyCode::Left);
    fixture.shift(KeyCode::Left);
    fixture.key(KeyCode::Backspace);
    assert!(fixture.pane().contains("abcd"), "{}", fixture.pane());
}

// ---------------------------------------------------------------------------
// Salvamento transacional
// ---------------------------------------------------------------------------

#[test]
fn saving_writes_through_the_core_with_the_revision_that_was_read() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" e mais");
    assert_eq!(
        fixture.stored(),
        "original",
        "nada é escrito antes de salvar"
    );

    fixture.ctrl('s');
    assert_eq!(fixture.stored(), "original e mais");
    assert!(
        fixture.screen().contains("Edição salva"),
        "{}",
        fixture.screen()
    );
}

#[test]
fn saving_twice_in_a_row_uses_the_revision_the_first_save_produced() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" um");
    fixture.ctrl('s');
    fixture.type_text(" dois");
    fixture.ctrl('s');
    assert_eq!(fixture.stored(), "original um dois");
}

#[test]
fn a_canonical_no_op_writes_nothing_at_all() {
    let mut fixture = Fixture::new("original");
    let before = fixture.stored();
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text("x");
    fixture.key(KeyCode::Backspace);
    fixture.ctrl('s');
    assert_eq!(fixture.stored(), before);
    assert!(
        fixture.screen().contains("Sem alteração canônica"),
        "{}",
        fixture.screen()
    );
}

// ---------------------------------------------------------------------------
// Alterações pendentes
// ---------------------------------------------------------------------------

#[test]
fn leaving_with_pending_changes_asks_before_anything_is_lost() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text("X");
    fixture.key(KeyCode::Esc);

    let screen = fixture.screen();
    assert!(
        screen.contains("Alterações não salvas"),
        "Esc com pendências precisa confirmar:\n{screen}"
    );
    assert!(
        screen.contains("Edição:"),
        "a confirmação não pode já ter saído do editor:\n{screen}"
    );

    // Continuar editando mantém o texto.
    fixture.key(KeyCode::Esc);
    assert!(fixture.pane().contains("originalX"), "{}", fixture.pane());
}

#[test]
fn the_pending_prompt_can_save_and_leave() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text("X");
    fixture.key(KeyCode::Esc);
    fixture.key(KeyCode::Char('s'));

    assert_eq!(fixture.stored(), "originalX");
    let screen = fixture.screen();
    assert!(
        screen.contains("Leitura:") && !screen.contains("Edição:"),
        "{screen}"
    );
}

#[test]
fn the_pending_prompt_can_discard_and_leave() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text("X");
    fixture.key(KeyCode::Esc);
    assert!(
        fixture.screen().contains("Alterações não salvas"),
        "o descarte só existe depois da confirmação:\n{}",
        fixture.screen()
    );
    fixture.key(KeyCode::Char('d'));

    assert_eq!(fixture.stored(), "original");
    let pane = fixture.pane();
    assert!(pane.contains("Leitura:"), "{pane}");
    assert!(!pane.contains("originalX"), "{pane}");
}

#[test]
fn navigation_keys_are_text_while_editing_so_nothing_leaves_silently() {
    // Tab, `/`, `d` and `q` navigate elsewhere in the application. Inside the
    // editor they are characters, which is what makes "leaving with pending
    // changes" a single guarded door rather than five.
    let mut fixture = Fixture::new("");
    fixture.open();
    fixture.type_text("q/d");
    fixture.key(KeyCode::Tab);
    fixture.type_text("fim");

    let screen = fixture.screen();
    assert!(
        screen.contains("Edição:"),
        "o editor foi abandonado:\n{screen}"
    );
    assert!(screen.contains("q/d"), "{screen}");
    assert!(screen.contains("fim"), "{screen}");
    assert!(!fixture.app.should_quit);
}

// ---------------------------------------------------------------------------
// Ctrl+C: uma pessoa pedindo para sair, não um sinal
// ---------------------------------------------------------------------------

#[test]
fn ctrl_c_with_a_pending_draft_asks_instead_of_leaving() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" pendente");

    fixture.ctrl('c');

    assert!(
        !fixture.app.should_quit,
        "Ctrl+C com texto por salvar não pode encerrar sem perguntar"
    );
    let screen = fixture.screen();
    assert!(
        screen.contains("Alterações não salvas"),
        "a mesma pergunta de qualquer outra saída:\n{screen}"
    );
    assert!(
        fixture.pane().contains("original pendente"),
        "o rascunho continua no editor:\n{}",
        fixture.pane()
    );
    assert_eq!(fixture.stored(), "original", "nada foi escrito");
}

#[test]
fn ctrl_c_then_continue_editing_cancels_the_exit() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" pendente");
    fixture.ctrl('c');
    fixture.key(KeyCode::Esc);

    assert!(
        !fixture.app.should_quit,
        "continuar editando cancela a saída"
    );
    assert!(
        fixture.pane().contains("Edição:"),
        "o editor continua em foco:\n{}",
        fixture.pane()
    );
    assert!(fixture.pane().contains("original pendente"));
    assert_eq!(fixture.stored(), "original");

    // E a saída continua disponível depois.
    fixture.ctrl('c');
    fixture.key(KeyCode::Char('d'));
    assert!(fixture.app.should_quit);
}

#[test]
fn ctrl_c_then_save_writes_through_the_core_and_leaves() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" pendente");
    fixture.ctrl('c');
    fixture.key(KeyCode::Char('s'));

    assert_eq!(fixture.stored(), "original pendente");
    assert!(
        fixture.app.should_quit,
        "salvar atende ao pedido de sair que abriu a pergunta"
    );
}

#[test]
fn ctrl_c_then_discard_leaves_the_note_as_it_was() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" pendente");
    fixture.ctrl('c');
    assert!(
        fixture.screen().contains("Alterações não salvas"),
        "o descarte só existe depois da pergunta:\n{}",
        fixture.screen()
    );
    assert!(!fixture.app.should_quit);

    fixture.key(KeyCode::Char('d'));
    assert_eq!(fixture.stored(), "original", "o descarte não escreve nada");
    assert!(fixture.app.should_quit);
    assert!(
        !fixture.pane().contains("original pendente"),
        "o rascunho foi conscientemente abandonado:\n{}",
        fixture.pane()
    );
}

#[test]
fn ctrl_c_with_a_conflict_keeps_the_draft_and_does_not_leave() {
    // Salvar pela pergunta de saída pode encontrar um conflito. Um conflito é
    // uma pergunta nova, e o pedido de sair não sobrevive a ela: nada é
    // sobrescrito, nada é descartado, e ninguém sai sem decidir.
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" da TUI");
    fixture.external_write("outro autor");

    fixture.ctrl('c');
    fixture.key(KeyCode::Char('s'));

    assert!(
        !fixture.app.should_quit,
        "um conflito não deixa a saída seguir"
    );
    let screen = fixture.screen();
    assert!(screen.contains("Conflito de revision"), "{screen}");
    assert_eq!(fixture.stored(), "outro autor");
    assert!(fixture.pane().contains("original da TUI"));
}

#[test]
fn ctrl_c_with_nothing_pending_still_leaves_at_once() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.ctrl('c');
    assert!(
        fixture.app.should_quit,
        "sem nada a perder, Ctrl+C continua encerrando direto"
    );
    assert_eq!(fixture.stored(), "original");
}

#[test]
fn ctrl_c_outside_the_editor_is_unchanged() {
    let mut fixture = Fixture::new("original");
    fixture.ctrl('c');
    assert!(
        fixture.app.should_quit,
        "na lista, Ctrl+C encerra como sempre"
    );
}

// ---------------------------------------------------------------------------
// Conflito de revision
// ---------------------------------------------------------------------------

#[test]
fn an_external_write_never_loses_and_never_overwrites() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" da TUI");
    fixture.external_write("outro autor");

    fixture.ctrl('s');
    assert_eq!(
        fixture.stored(),
        "outro autor",
        "a escrita externa não pode ser sobrescrita"
    );

    let screen = fixture.screen();
    assert!(screen.contains("Conflito de revision"), "{screen}");
    assert!(
        fixture.pane().contains("original da TUI"),
        "o rascunho continua no editor:\n{}",
        fixture.pane()
    );
    for option in ["[p]", "[r]", "[Esc]"] {
        assert!(
            screen.contains(option),
            "o conflito precisa oferecer {option}:\n{screen}"
        );
    }
}

#[test]
fn a_conflict_can_preserve_the_draft_to_a_recoverable_file() {
    let mut fixture = Fixture::new("original");
    let recovery = fixture._root.path().join("state/note-it/tui-recovery");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" da TUI");
    fixture.external_write("outro autor");
    fixture.ctrl('s');
    fixture.key(KeyCode::Char('p'));

    let saved: Vec<_> = std::fs::read_dir(&recovery)
        .expect("o diretório de recovery deve existir")
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(saved.len(), 1, "exatamente um rascunho preservado");
    assert_eq!(
        std::fs::read_to_string(&saved[0]).unwrap(),
        "original da TUI"
    );
    // Created by the application, from nothing: private in both directions.
    let mode = |path: &std::path::Path| {
        <std::fs::Metadata as std::os::unix::fs::MetadataExt>::mode(
            &std::fs::metadata(path).unwrap(),
        ) & 0o777
    };
    assert_eq!(mode(&recovery), 0o700, "o diretório nasce privado");
    assert_eq!(mode(&saved[0]), 0o600, "o rascunho nasce privado");
    assert_eq!(fixture.stored(), "outro autor");
    assert!(
        fixture.pane().contains("outro autor"),
        "após preservar, o editor mostra a nota relida:\n{}",
        fixture.pane()
    );
}

#[test]
fn a_conflict_can_reread_the_note_as_it_now_is() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" da TUI");
    fixture.external_write("outro autor");
    fixture.ctrl('s');
    fixture.key(KeyCode::Char('r'));

    let pane = fixture.pane();
    assert!(pane.contains("outro autor"), "{pane}");
    assert!(!pane.contains("original da TUI"), "{pane}");
    assert_eq!(fixture.stored(), "outro autor");

    // Relida, a nota volta a ser gravável sem novo conflito.
    fixture.key(KeyCode::End);
    fixture.type_text("!");
    fixture.ctrl('s');
    assert_eq!(fixture.stored(), "outro autor!");
}

#[test]
fn a_conflict_can_keep_the_draft_in_place() {
    let mut fixture = Fixture::new("original");
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text(" da TUI");
    fixture.external_write("outro autor");
    fixture.ctrl('s');
    fixture.key(KeyCode::Esc);

    assert!(
        fixture.pane().contains("original da TUI"),
        "{}",
        fixture.pane()
    );
    assert!(
        !fixture.screen().contains("Conflito de revision"),
        "{}",
        fixture.screen()
    );
    assert_eq!(fixture.stored(), "outro autor");
}

// ---------------------------------------------------------------------------
// Bordas
// ---------------------------------------------------------------------------

#[test]
fn an_empty_note_opens_editable_and_a_new_note_opens_editing() {
    let mut fixture = Fixture::new("");
    fixture.open();
    fixture.type_text("primeira letra");
    fixture.ctrl('s');
    assert_eq!(fixture.stored(), "primeira letra");
}

#[test]
fn a_note_taller_than_the_pane_scrolls_to_keep_the_cursor_visible() {
    let body: String = (1..=200)
        .map(|n| format!("linha {n}\n"))
        .collect::<String>();
    let mut fixture = Fixture::sized(&body, 100, 24);
    fixture.open();
    fixture.key(KeyCode::End);
    for _ in 0..250 {
        fixture.key(KeyCode::Down);
    }
    fixture.type_text("FIM");

    let body = fixture.body();
    assert!(body.contains("FIM"), "o cursor saiu da viewport:\n{body}");
    assert!(
        !body.contains("linha 1 "),
        "a viewport deveria ter rolado:\n{body}"
    );
}

#[test]
fn a_line_wider_than_the_pane_scrolls_horizontally_with_the_cursor() {
    let mut fixture = Fixture::sized(&"a".repeat(400), 80, 20);
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text("FIM");
    assert!(fixture.pane().contains("FIM"), "{}", fixture.pane());
}

#[test]
fn a_narrow_terminal_still_edits() {
    let mut fixture = Fixture::sized("original", 40, 12);
    fixture.open();
    fixture.key(KeyCode::End);
    fixture.type_text("X");
    fixture.ctrl('s');
    assert_eq!(fixture.stored(), "originalX");
}

#[test]
fn resizing_the_terminal_keeps_the_cursor_in_view() {
    let body: String = (1..=60).map(|n| format!("linha {n}\n")).collect();
    let mut fixture = Fixture::sized(&body, 100, 40);
    fixture.open();
    for _ in 0..60 {
        fixture.key(KeyCode::Down);
    }
    fixture.key(KeyCode::End);
    fixture.type_text("FIM");
    assert!(fixture.body().contains("FIM"), "{}", fixture.body());

    // The viewport is decided while drawing, so a new size is simply the next
    // frame's size: the cursor has to survive both directions.
    fixture.terminal.backend_mut().resize(70, 12);
    assert!(
        fixture.body().contains("FIM"),
        "após encolher:\n{}",
        fixture.body()
    );
    fixture.terminal.backend_mut().resize(160, 50);
    assert!(
        fixture.body().contains("FIM"),
        "após crescer:\n{}",
        fixture.body()
    );

    fixture.ctrl('s');
    assert!(fixture.stored().ends_with("FIM"));
}

#[test]
fn page_keys_move_by_a_screenful_and_stop_at_the_edges() {
    let body: String = (1..=300).map(|n| format!("linha {n}\n")).collect();
    let mut fixture = Fixture::sized(&body, 100, 24);
    fixture.open();
    fixture.screen(); // a viewport only exists once something has been drawn
    fixture.key(KeyCode::PageDown);
    let after_one_page = fixture.body();
    assert!(
        !after_one_page.contains("│linha 1 "),
        "PageDown não avançou:\n{after_one_page}"
    );

    for _ in 0..500 {
        fixture.key(KeyCode::PageUp);
    }
    assert!(
        fixture.body().contains("│linha 1 "),
        "PageUp deve parar no início:\n{}",
        fixture.body()
    );
    for _ in 0..500 {
        fixture.key(KeyCode::PageDown);
    }
    assert!(
        fixture.body().contains("│linha 300"),
        "PageDown deve parar no fim:\n{}",
        fixture.body()
    );
}

#[test]
fn hostile_content_stays_inert_inside_the_editor() {
    let mut fixture = Fixture::new("antes\u{1b}[31m depois");
    fixture.open();
    let pane = fixture.pane();
    assert!(!pane.contains('\u{1b}'), "escape ativo no buffer");
    assert!(pane.contains("antes"), "{pane}");
    assert!(
        pane.contains('\u{00B7}'),
        "o controle vira um marcador visível:\n{pane}"
    );

    fixture.key(KeyCode::End);
    fixture.type_text("\u{7}fim");
    let pane = fixture.pane();
    assert!(!pane.contains('\u{7}'));
    assert!(pane.contains("fim"), "{pane}");
}

#[test]
fn a_trash_note_is_read_and_never_edited() {
    let mut fixture = Fixture::new("original");
    fixture.key(KeyCode::Char('3'));
    fixture.open();
    fixture.type_text("X");

    let screen = fixture.screen();
    assert!(
        !screen.contains("Edição:"),
        "a lixeira não é editável:\n{screen}"
    );
}

// ---------------------------------------------------------------------------
// Terminal real
// ---------------------------------------------------------------------------

#[test]
fn a_real_terminal_edits_saves_and_is_restored_exactly() {
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "");
    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("Notas Recentes");
    // Enter opens the note to be worked on: no second key.
    tui.input(b"\r");
    tui.wait_text("Edição:");
    // Accented and multi-byte text typed straight into the pane. What proves
    // it arrived is what the store ends up holding: Ratatui repaints only the
    // cells a keystroke changed, so a word being typed one key at a time never
    // crosses the wire whole. The drawn glyphs are asserted with TestBackend.
    tui.input("ação — 日本語".as_bytes());
    tui.input(b"\x13"); // Ctrl+S
    tui.wait_text("Edição salva");

    let stored = NoteItCore::open_read_only_at(paths.clone())
        .read_note(&id)
        .unwrap()
        .content;
    assert_eq!(stored, "ação — 日本語");

    tui.input(b"\x1b\x1b"); // sai da edição limpa e depois da leitura
    tui.finish();
    cleanup_coordination(&paths);
}

#[test]
fn the_external_editor_remains_available_as_an_alternative() {
    // The roadmap keeps `$EDITOR` as an alternative action, not a requirement
    // for everyday editing: `Enter` edits in place, `Esc` steps out to the
    // reader, and `e` there still hands the body to the configured program.
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "original");
    let program = root.path().join("fake-editor");
    std::fs::write(
        &program,
        "#!/bin/sh\nset -eu\nprintf 'vindo do editor externo' > \"$1\"\n",
    )
    .unwrap();
    std::fs::set_permissions(
        &program,
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
    )
    .unwrap();

    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("EDITOR", &program)
            .env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("original");
    tui.open_external_editor();
    tui.wait_text("Edição salva");

    let stored = NoteItCore::open_read_only_at(paths.clone())
        .read_note(&id)
        .unwrap()
        .content;
    assert_eq!(stored, "vindo do editor externo");

    tui.finish();
    cleanup_coordination(&paths);
}

#[test]
fn a_real_terminal_pastes_several_lines_at_once() {
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "");
    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("Notas Recentes");
    tui.input(b"\r");
    tui.wait_text("Edição:");
    tui.input(b"uma\rduas\rtres");
    tui.input(b"\x13");
    tui.wait_text("Edição salva");

    let stored = NoteItCore::open_read_only_at(paths.clone())
        .read_note(&id)
        .unwrap()
        .content;
    assert_eq!(stored, "uma\nduas\ntres");

    tui.input(b"\x1b\x1b");
    tui.finish();
    cleanup_coordination(&paths);
}

#[test]
fn a_real_narrow_terminal_edits_and_restores() {
    let root = tempfile::tempdir().unwrap();
    let (paths, _id) = store(root.path(), &root.path().join("runtime"), "original");
    let mut tui = Tui::spawn_sized(
        root.path(),
        env!("CARGO_BIN_EXE_noteit-tui").as_ref(),
        44,
        14,
        |cmd| {
            cmd.env("XDG_RUNTIME_DIR", root.path().join("runtime"));
        },
    );
    tui.wait_text("Note-it");
    tui.input(b"\r");
    tui.wait_text("Edição");
    tui.input(b"X");
    tui.input(b"\x13");
    tui.wait_text("salva");
    tui.input(b"\x1b\x1b");
    tui.finish();
    cleanup_coordination(&paths);
}

#[test]
fn a_real_terminal_cancelling_an_edit_writes_nothing_and_restores() {
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "original");
    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("original");
    tui.input(b"\r");
    tui.wait_text("Edição:");
    tui.input(b"descartado ");
    // The prompt is itself the proof that the text registered: with nothing
    // pending, `Esc` would have stepped straight out to the reader.
    tui.input(b"\x1b");
    tui.wait_text("Alterações não salvas");
    tui.input(b"d");
    tui.wait_text("Leitura:");

    let stored = NoteItCore::open_read_only_at(paths.clone())
        .read_note(&id)
        .unwrap()
        .content;
    assert_eq!(stored, "original");

    tui.input(b"\x1b");
    tui.finish();
    cleanup_coordination(&paths);
}

/// Everything a signal must leave behind when a draft was never saved.
///
/// The three guarantees are separate and are checked separately: the note in
/// the store is untouched, the unsaved text exists somewhere it can be found,
/// and the terminal is the terminal the person started with. A restored
/// terminal is not evidence that the text survived.
fn signal_with_a_pending_draft(signal: libc::c_int, name: &str) {
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "");
    let recovery = root.path().join("state/note-it/tui-recovery");

    // Somebody else's recovery, already there. Preserving a new draft must add
    // to this directory, never write over what is in it.
    std::fs::create_dir_all(&recovery).unwrap();
    // Created the way the application itself creates it, so the check further
    // down asks whether preserving *loosened* it, not what this test's umask
    // happened to be.
    std::fs::set_permissions(&recovery, std::fs::Permissions::from_mode(0o700)).unwrap();
    let decoy = recovery.join("anterior.md");
    std::fs::write(&decoy, "recovery anterior").unwrap();

    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("Notas Recentes");
    tui.input(b"\r");
    tui.wait_text("Edição:");
    tui.input("rascunho pendente com acentuação".as_bytes());
    // The application itself says whether the draft is pending: `Esc` asks
    // this question only when there is something to lose, so the signal below
    // is delivered to a state the application has just confirmed is dirty.
    // (The `●` on the title says the same thing, but Ratatui repaints only the
    // cells a key changed, so neither it nor any later notice crosses the wire
    // as one string — the prompt is the one anchor that is a whole new line.)
    // The draft is untouched while the question is open, and a signal must not
    // care whether a question happens to be on screen.
    tui.input(b"\x1b");
    tui.wait_text("Alterações não salvas");

    tui.signal(signal);
    tui.wait_exit();

    // 1. The note in the store was never written.
    assert_eq!(
        NoteItCore::open_read_only_at(paths.clone())
            .read_note(&id)
            .unwrap()
            .content,
        "",
        "{name} não pode escrever a nota"
    );

    // 2. The unsaved text is recoverable, complete and byte-exact.
    let mut saved: Vec<_> = std::fs::read_dir(&recovery)
        .unwrap_or_else(|error| {
            panic!("{name} perdeu o rascunho: {recovery:?} não existe ({error})")
        })
        .map(|entry| entry.unwrap().path())
        .collect();
    saved.sort();
    assert_eq!(
        saved.len(),
        2,
        "{name}: o rascunho novo, ao lado do anterior"
    );
    assert_eq!(
        std::fs::read_to_string(&decoy).unwrap(),
        "recovery anterior",
        "{name}: um recovery alheio não pode ser sobrescrito"
    );
    let preserved = saved
        .iter()
        .find(|path| *path != &decoy)
        .expect("o rascunho preservado");
    assert_eq!(
        std::fs::read_to_string(preserved).unwrap(),
        "rascunho pendente com acentuação",
        "{name}: o texto recuperado tem de ser o rascunho inteiro"
    );
    assert!(
        preserved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&id.to_string()),
        "{name}: o recovery precisa dizer de qual nota é"
    );
    // The same privacy the external editor's recovery already guarantees.
    let mode = |path: &std::path::Path| {
        <std::fs::Metadata as std::os::unix::fs::MetadataExt>::mode(
            &std::fs::metadata(path).unwrap(),
        ) & 0o777
    };
    assert_eq!(mode(preserved), 0o600, "{name}: o rascunho é privado");
    assert_eq!(
        mode(&recovery),
        0o700,
        "{name}: preservar não pode afrouxar o diretório"
    );

    // 3. Somebody is told where it went, on the restored terminal.
    let transcript = String::from_utf8_lossy(&tui.output).to_string();
    assert!(
        transcript.contains("tui-recovery"),
        "{name}: o caminho preservado precisa ser informado:\n{transcript}"
    );

    // 4. The terminal is exactly the one the session started with.
    tui.assert_restored();
    assert!(tui.cooked(), "{name} deixou o terminal em raw/no-echo");

    cleanup_coordination(&paths);
}

#[test]
fn a_saved_draft_leaves_no_recovery_behind() {
    // Recovery exists for text the store refused or never received. A save
    // that worked must not also leave a file behind suggesting it did not:
    // a recovery directory is a claim that something needs rescuing.
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "");
    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("Notas Recentes");
    tui.input(b"\r");
    tui.wait_text("Edição:");
    tui.input(b"salvo antes de sair");
    tui.input(b"\x13");
    tui.wait_text("Edição salva");

    // The same signal path as the tests above, but with nothing pending: it
    // must find nothing to preserve and still restore the terminal.
    tui.signal(libc::SIGTERM);
    tui.wait_exit();
    tui.assert_restored();
    assert!(tui.cooked());

    assert_eq!(
        NoteItCore::open_read_only_at(paths.clone())
            .read_note(&id)
            .unwrap()
            .content,
        "salvo antes de sair"
    );
    assert!(
        !root.path().join("state/note-it/tui-recovery").exists(),
        "um salvamento confirmado não deixa recovery para trás"
    );
    cleanup_coordination(&paths);
}

#[test]
fn a_sigterm_with_a_pending_draft_preserves_it_and_restores_the_terminal() {
    signal_with_a_pending_draft(libc::SIGTERM, "SIGTERM");
}

#[test]
fn a_sigint_with_a_pending_draft_preserves_it_and_restores_the_terminal() {
    signal_with_a_pending_draft(libc::SIGINT, "SIGINT");
}

#[test]
fn a_sighup_with_a_pending_draft_preserves_it_and_restores_the_terminal() {
    // Closing the terminal window, or an ssh session dropping, is the most
    // ordinary way somebody loses text they never saved.
    signal_with_a_pending_draft(libc::SIGHUP, "SIGHUP");
}

#[test]
fn a_real_terminal_ctrl_c_asks_before_leaving_and_leaves_nothing_behind() {
    // The keyboard counterpart of the signal tests above. In raw mode `Ctrl+C`
    // is a key, so there is a screen to ask on: it must ask, and once it has
    // been answered consciously there is nothing left to rescue.
    let root = tempfile::tempdir().unwrap();
    let (paths, id) = store(root.path(), &root.path().join("runtime"), "");
    let mut tui = Tui::spawn(root.path(), |cmd| {
        cmd.env("XDG_RUNTIME_DIR", root.path().join("runtime"));
    });
    tui.wait_text("Notas Recentes");
    tui.input(b"\r");
    tui.wait_text("Edição:");
    tui.input(b"nao salvo");

    tui.input(b"\x03");
    // Still running, and asking.
    tui.wait_text("Alterações não salvas");
    assert!(
        tui.child.try_wait().unwrap().is_none(),
        "Ctrl+C com pendências não pode encerrar sem resposta"
    );

    // A conscious discard finishes the exit that asked.
    tui.input(b"d");
    tui.wait_exit();
    tui.assert_restored();
    assert!(tui.cooked());

    assert_eq!(
        NoteItCore::open_read_only_at(paths.clone())
            .read_note(&id)
            .unwrap()
            .content,
        "",
        "o descarte consciente não escreve nada"
    );
    assert!(
        !root.path().join("state/note-it/tui-recovery").exists(),
        "texto abandonado de propósito não vira um recovery enganoso"
    );
    cleanup_coordination(&paths);
}
