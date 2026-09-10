//! Fase 5.0D.1 — fidelidade de Markdown do leitor da TUI.
//!
//! O leitor precisa mostrar o que a nota diz, e não como ela está escrita. O
//! subconjunto de HTML que o editor gráfico persiste — cor de texto,
//! marca-texto, sublinhado, comentários e entidades — é reconhecido
//! deliberadamente e vira estilo; qualquer outra coisa continua texto inerte.
//!
//! As fixturas são sintéticas. Nenhuma nota real do usuário é lida, escrita ou
//! copiada para cá: os casos reproduzem apenas as *formas* observadas no
//! diagnóstico visual.

use noteit_core::{chrono::Utc, model::NoteDocument, StorePaths, Uuid};
use noteit_tui::app::App;
use noteit_tui::markdown::render_markdown;
use ratatui::{
    backend::TestBackend,
    style::{Color, Modifier, Style},
    text::Line,
    Terminal,
};
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The plain text a reader sees, one rendered line per entry.
fn text_lines(markdown: &str) -> Vec<String> {
    render_markdown(markdown)
        .iter()
        .map(Line::to_string)
        .collect()
}

/// Everything the reader sees for `markdown`, joined by newlines.
fn text(markdown: &str) -> String {
    text_lines(markdown).join("\n")
}

/// The style of the first span whose content is exactly `needle`.
fn style_of(markdown: &str, needle: &str) -> Style {
    let lines = render_markdown(markdown);
    for line in &lines {
        for span in &line.spans {
            if span.content == needle {
                return span.style;
            }
        }
    }
    panic!(
        "no span reads exactly {needle:?}; rendered spans were {:?}",
        lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|s| s.content.to_string()))
            .collect::<Vec<_>>()
    );
}

const HIGHLIGHT_FG: Color = Color::Rgb(0x1E, 0x29, 0x3B);

/// Every spelling of Note-it's storage that must never reach the screen.
const STORAGE_SPELLINGS: &[&str] = &[
    "<span",
    "</span>",
    "<mark",
    "</mark>",
    "<u>",
    "</u>",
    "<!--",
    "-->",
    "&nbsp;",
    "data-note-it-color",
    "data-note-it-highlight",
    "data-note-it-font-size",
    "background-color",
    "note-it:completed_at",
];

fn assert_no_storage_syntax(rendered: &str, context: &str) {
    for spelling in STORAGE_SPELLINGS {
        assert!(
            !rendered.contains(spelling),
            "{context} leaks {spelling:?}:\n{rendered}"
        );
    }
}

// ---------------------------------------------------------------------------
// A. Títulos
// ---------------------------------------------------------------------------

#[test]
fn every_heading_level_has_its_own_visual_identity() {
    let md = "# um\n## dois\n### três\n#### quatro\n##### cinco\n###### seis";
    let lines = render_markdown(md);
    assert_eq!(lines.len(), 6);

    let styles: Vec<Style> = lines
        .iter()
        .map(|line| line.spans[0].style)
        .collect::<Vec<_>>();

    // Six levels, six foregrounds. A level whose colour repeats another's is
    // not a level a reader can tell apart.
    for (a, first) in styles.iter().enumerate() {
        for (b, second) in styles.iter().enumerate() {
            if a == b {
                continue;
            }
            assert_ne!(
                first.fg,
                second.fg,
                "H{} and H{} share a foreground",
                a + 1,
                b + 1
            );
        }
        assert!(first.fg.is_some(), "H{} has no explicit foreground", a + 1);
    }

    // The identity is not colour alone: the marker and the emphasis carry it
    // in a terminal with a poor palette.
    for (index, style) in styles.iter().enumerate() {
        assert!(
            style.add_modifier.contains(Modifier::BOLD),
            "H{} is not bold",
            index + 1
        );
    }
    assert_eq!(styles.len(), 6);
    assert!(
        styles[0].add_modifier != styles[5].add_modifier,
        "H1 and H6 must differ by more than colour"
    );
}

#[test]
fn a_heading_carries_colour_highlight_and_emphasis_without_showing_them() {
    let md = "#### tes<span data-note-it-color=\"#64748B\" style=\"color:#64748B\"><mark data-note-it-highlight=\"#BBF7D0\" style=\"background-color:#BBF7D0\">te H4</mark></span>";
    let rendered = text(md);
    assert_no_storage_syntax(&rendered, "H4 with colour and highlight");
    assert!(rendered.contains("#### teste H4"), "got {rendered:?}");

    let inner = style_of(md, "te H4");
    assert_eq!(inner.bg, Some(Color::Rgb(0xBB, 0xF7, 0xD0)));
    assert_eq!(inner.fg, Some(HIGHLIGHT_FG));
    assert!(inner.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn colour_and_highlight_reach_every_heading_level() {
    for level in 1..=6usize {
        let hashes = "#".repeat(level);
        let coloured = format!(
            "{hashes} a<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">cor</span>"
        );
        let marked = format!(
            "{hashes} a<mark data-note-it-highlight=\"#BFDBFE\" style=\"background-color:#BFDBFE\">marca</mark>"
        );

        let rendered = text(&coloured);
        assert_no_storage_syntax(&rendered, &format!("H{level} with colour"));
        assert_eq!(rendered, format!("{hashes} acor"));
        assert_eq!(
            style_of(&coloured, "cor").fg,
            Some(Color::Rgb(0xDC, 0x26, 0x26)),
            "H{level} lost the colour the note asked for"
        );

        let rendered = text(&marked);
        assert_no_storage_syntax(&rendered, &format!("H{level} with highlight"));
        assert_eq!(rendered, format!("{hashes} amarca"));
        let style = style_of(&marked, "marca");
        assert_eq!(style.bg, Some(Color::Rgb(0xBF, 0xDB, 0xFE)));
        assert_eq!(style.fg, Some(HIGHLIGHT_FG));
    }
}

#[test]
fn emphasis_inside_a_heading_is_rendered_not_spelled() {
    // A heading is already bold, so `**` inside one adds nothing a reader can
    // see — what matters is that it is consumed rather than spelled out.
    assert_eq!(text("## um **dois** três"), "## um dois três");
    assert_eq!(text("## um ~~dois~~ três"), "## um dois três");
    assert_eq!(text("## um *dois* três"), "## um dois três");

    // The emphasis that *is* distinguishable composes with the level's style.
    let italic = style_of("## um *dois* três", "dois");
    assert!(italic.add_modifier.contains(Modifier::ITALIC));
    assert!(
        italic.add_modifier.contains(Modifier::BOLD),
        "the heading's own weight survives the emphasis inside it"
    );
    assert_eq!(italic.fg, style_of("## um", "## ").fg);

    let struck = style_of("## um ~~dois~~ três", "dois");
    assert!(struck.add_modifier.contains(Modifier::CROSSED_OUT));
}

// ---------------------------------------------------------------------------
// B. HTML canônico do Note-it
// ---------------------------------------------------------------------------

#[test]
fn a_colour_span_becomes_a_foreground() {
    let md =
        "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">teste na nota preta</span>";
    assert_eq!(text(md), "teste na nota preta");
    assert_eq!(
        style_of(md, "teste na nota preta").fg,
        Some(Color::Rgb(0xDC, 0x26, 0x26))
    );
}

#[test]
fn a_colour_span_carrying_only_style_is_still_a_colour() {
    let md = "<span style=\"color:#2563EB\">azul</span>";
    assert_eq!(text(md), "azul");
    assert_eq!(style_of(md, "azul").fg, Some(Color::Rgb(0x25, 0x63, 0xEB)));
}

#[test]
fn a_highlight_mark_becomes_a_readable_background() {
    let md = "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">marcado</mark>";
    assert_eq!(text(md), "marcado");
    let style = style_of(md, "marcado");
    assert_eq!(style.bg, Some(Color::Rgb(0xFD, 0xE6, 0x8A)));
    assert_eq!(
        style.fg,
        Some(HIGHLIGHT_FG),
        "highlighted text keeps the GUI's dark foreground so it stays readable"
    );
}

#[test]
fn a_highlight_mark_carrying_only_style_is_still_a_highlight() {
    let md = "<mark style=\"background-color:#BFDBFE\">marcado</mark>";
    assert_eq!(text(md), "marcado");
    assert_eq!(
        style_of(md, "marcado").bg,
        Some(Color::Rgb(0xBF, 0xDB, 0xFE))
    );
}

#[test]
fn a_bare_mark_from_a_legacy_note_is_still_a_highlight() {
    let md = "<mark>legado</mark>";
    assert_eq!(text(md), "legado");
    let style = style_of(md, "legado");
    assert!(style.bg.is_some(), "a bare <mark> is a highlight");
    assert_eq!(style.fg, Some(HIGHLIGHT_FG));
}

#[test]
fn a_span_containing_a_mark_keeps_both_the_background_and_a_readable_text() {
    let md = "<span data-note-it-color=\"#64748B\" style=\"color:#64748B\"><mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">teste de verdade</mark></span>";
    assert_eq!(text(md), "teste de verdade");
    let style = style_of(md, "teste de verdade");
    assert_eq!(style.bg, Some(Color::Rgb(0xFD, 0xE6, 0x8A)));
    assert_eq!(style.fg, Some(HIGHLIGHT_FG));
}

#[test]
fn a_mark_containing_a_span_lets_the_inner_colour_win() {
    let md = "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\"><span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">vermelho</span></mark>";
    assert_eq!(text(md), "vermelho");
    let style = style_of(md, "vermelho");
    assert_eq!(style.bg, Some(Color::Rgb(0xFD, 0xE6, 0x8A)));
    assert_eq!(style.fg, Some(Color::Rgb(0xDC, 0x26, 0x26)));
}

#[test]
fn underline_is_a_modifier_and_never_a_tag() {
    let md = "<u>teste de sublinhado</u>";
    assert_eq!(text(md), "teste de sublinhado");
    assert!(style_of(md, "teste de sublinhado")
        .add_modifier
        .contains(Modifier::UNDERLINED));
}

#[test]
fn closing_a_tag_restores_exactly_the_style_that_was_open() {
    let md = "<span data-note-it-color=\"#DC2626\">a<span data-note-it-color=\"#2563EB\">b</span>c</span>d";
    assert_eq!(text(md), "abcd");
    assert_eq!(style_of(md, "a").fg, Some(Color::Rgb(0xDC, 0x26, 0x26)));
    assert_eq!(style_of(md, "b").fg, Some(Color::Rgb(0x25, 0x63, 0xEB)));
    assert_eq!(
        style_of(md, "c").fg,
        Some(Color::Rgb(0xDC, 0x26, 0x26)),
        "the inner span must not erase the outer one"
    );
    assert_ne!(style_of(md, "d").fg, Some(Color::Rgb(0xDC, 0x26, 0x26)));
}

#[test]
fn colour_composes_with_every_emphasis() {
    let md = "<span data-note-it-color=\"#DC2626\">**n** *i* ~~r~~ <u>s</u> `c`</span>";
    assert_eq!(text(md), "n i r s c");
    let red = Color::Rgb(0xDC, 0x26, 0x26);

    let bold = style_of(md, "n");
    assert_eq!(bold.fg, Some(red));
    assert!(bold.add_modifier.contains(Modifier::BOLD));

    let italic = style_of(md, "i");
    assert_eq!(italic.fg, Some(red));
    assert!(italic.add_modifier.contains(Modifier::ITALIC));

    let struck = style_of(md, "r");
    assert_eq!(struck.fg, Some(red));
    assert!(struck.add_modifier.contains(Modifier::CROSSED_OUT));

    let underlined = style_of(md, "s");
    assert_eq!(underlined.fg, Some(red));
    assert!(underlined.add_modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn a_font_size_span_keeps_its_text_and_shows_no_tag() {
    let md = "<span data-note-it-font-size=\"32\" style=\"font-size:32px\">grande</span>";
    assert_eq!(text(md), "grande");
}

// ---------------------------------------------------------------------------
// C. Markdown inline
// ---------------------------------------------------------------------------

#[test]
fn every_recognised_delimiter_is_consumed() {
    assert_eq!(text("**negrito**"), "negrito");
    assert_eq!(text("__negrito__"), "negrito");
    assert_eq!(text("*italico*"), "italico");
    assert_eq!(text("_italico_"), "italico");
    assert_eq!(text("~~riscado~~"), "riscado");
    assert_eq!(text("`codigo`"), "codigo");
    assert_eq!(text("***negrito com italico***"), "negrito com italico");

    assert!(style_of("**negrito**", "negrito")
        .add_modifier
        .contains(Modifier::BOLD));
    assert!(style_of("__negrito__", "negrito")
        .add_modifier
        .contains(Modifier::BOLD));
    assert!(style_of("*italico*", "italico")
        .add_modifier
        .contains(Modifier::ITALIC));
    assert!(style_of("_italico_", "italico")
        .add_modifier
        .contains(Modifier::ITALIC));
    assert!(style_of("~~riscado~~", "riscado")
        .add_modifier
        .contains(Modifier::CROSSED_OUT));

    let both = style_of("***negrito com italico***", "negrito com italico").add_modifier;
    assert!(both.contains(Modifier::BOLD) && both.contains(Modifier::ITALIC));
}

#[test]
fn an_unpaired_delimiter_stays_the_character_it_is() {
    assert_eq!(text("= 100 * 2.5"), "= 100 * 2.5");
    assert_eq!(text("3 * 4 * 5"), "3 * 4 * 5");
    assert_eq!(text("a ** b ** c"), "a ** b ** c");
    assert_eq!(
        text("note_it_config e snake_case"),
        "note_it_config e snake_case"
    );
    assert_eq!(
        text("caminho ~/Downloads e ~5 min"),
        "caminho ~/Downloads e ~5 min"
    );
    assert_eq!(text("**sem fecho"), "**sem fecho");
    assert_eq!(text("~~sem fecho"), "~~sem fecho");
    assert_eq!(text("`sem fecho"), "`sem fecho");
}

// ---------------------------------------------------------------------------
// D. Blocos
// ---------------------------------------------------------------------------

#[test]
fn every_block_kind_carries_inline_formatting_without_leaking_it() {
    let coloured = "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">vermelho</span>";
    let md = format!(
        "paragrafo com {coloured} e **negrito**\n\n\
         - item com {coloured}\n\
         1. numerado com **negrito**\n\
         - [ ] tarefa com {coloured}\n\
         - [x] concluida com **negrito** <!-- note-it:completed_at=2026-09-06T10:00:00Z -->\n\n\
         > citacao com {coloured}\n\n\
         > [!NOTE]\n> nota com {coloured}\n\n\
         > [!TIP]\n> dica com **negrito**\n\n\
         > [!IMPORTANT]\n> importante com <u>sublinhado</u>\n\n\
         > [!WARNING]\n> aviso com ~~riscado~~\n\n\
         > [!CAUTION]\n> cuidado com `codigo`\n"
    );

    let rendered = text(&md);
    assert_no_storage_syntax(&rendered, "all block kinds");
    for expected in [
        "paragrafo com vermelho e negrito",
        "• item com vermelho",
        "1. numerado com negrito",
        "☐ tarefa com vermelho",
        "☑ concluida com negrito",
        "│ citacao com vermelho",
        "▍ nota com vermelho",
        "▍ dica com negrito",
        "▍ importante com sublinhado",
        "▍ aviso com riscado",
        "▍ cuidado com codigo",
    ] {
        assert!(
            rendered.contains(expected),
            "missing {expected:?} in:\n{rendered}"
        );
    }
    assert!(rendered.contains("[NOTA]"));
    assert!(rendered.contains("[DICA]"));
    assert!(rendered.contains("[IMPORTANTE]"));
    assert!(rendered.contains("[AVISO]"));
    assert!(rendered.contains("[CUIDADO]"));
}

#[test]
fn a_hidden_comment_still_leaves_its_lines_where_the_cursor_expects_them() {
    // The reading cursor addresses stored lines, so a block that shows nothing
    // still has to occupy the lines it stores: blank, never absent.
    let stored = "antes\n<!-- primeira\nsegunda -->\ndepois";
    let rendered = noteit_tui::markdown::render_with_sources(stored);
    for source in 0..stored.lines().count() {
        assert!(
            rendered.sources.contains(&source),
            "stored line {source} has no rendered line the cursor can land on"
        );
    }
    assert_eq!(rendered.lines[1].to_string(), "");
    assert_eq!(rendered.lines[2].to_string(), "");
    assert_eq!(rendered.lines[3].to_string(), "depois");
}

#[test]
fn a_comment_opener_never_swallows_a_code_block() {
    let stored = "<!-- sem fecho\n```md\num --> dois\n```\ndepois";
    let rendered = text(stored);
    assert!(
        rendered.contains("sem fecho"),
        "text was swallowed: {rendered:?}"
    );
    assert!(
        rendered.contains("um --> dois"),
        "code is literal: {rendered:?}"
    );
    assert!(rendered.contains("depois"));
    assert!(
        !rendered.contains("<!--"),
        "the opener leaked: {rendered:?}"
    );
}

#[test]
fn a_fenced_block_is_source_and_stays_spelled_exactly() {
    let md = "```html\n<span data-note-it-color=\"#DC2626\">x</span>\n**nao negrito** &nbsp; <!-- visivel -->\n```";
    let rendered = text(md);
    assert!(
        rendered.contains("<span data-note-it-color=\"#DC2626\">x</span>"),
        "code is literal:\n{rendered}"
    );
    assert!(rendered.contains("**nao negrito** &nbsp; <!-- visivel -->"));
    assert!(rendered.contains("┌── [html]"));
}

// ---------------------------------------------------------------------------
// E. Comentários e entidades
// ---------------------------------------------------------------------------

#[test]
fn a_comment_is_storage_and_never_shows() {
    assert_eq!(text("<!-- esse é um comentário de teste -->").trim(), "");
    assert_eq!(text("antes <!-- oculto --> depois"), "antes  depois");
    let block = text("<!-- primeira\nsegunda -->\ndepois");
    assert_no_storage_syntax(&block, "multi-line comment");
    assert!(block.contains("depois"));
    assert!(!block.contains("primeira"));
    assert!(!block.contains("segunda"));
}

#[test]
fn the_task_completion_comment_stays_invisible() {
    let md = "- [x] Construir portão <!-- note-it:completed_at=2026-09-06T12:00:00Z -->";
    let rendered = text(md);
    assert_no_storage_syntax(&rendered, "completed task");
    assert!(rendered.contains("☑ Construir portão"));
}

#[test]
fn entities_are_decoded_exactly_once() {
    assert_eq!(text("a&nbsp;b"), "a\u{00A0}b");
    assert_eq!(text("a &amp; b"), "a & b");
    assert_eq!(text("&lt;script&gt;"), "<script>");
    assert_eq!(text("&quot;aspas&quot;"), "\"aspas\"");
    assert_eq!(text("&#65;&#x42;"), "AB");
    // A single pass: `&amp;lt;` is the text `&lt;`, never `<`.
    assert_eq!(text("&amp;lt;"), "&lt;");
    assert_eq!(text("&naoexiste;"), "&naoexiste;");
}

// ---------------------------------------------------------------------------
// F. Entradas hostis e malformadas
// ---------------------------------------------------------------------------

#[test]
fn hostile_html_is_inert_and_costs_no_text() {
    let cases = [
        (
            "<span data-note-it-color=\"#DC2626\">sem fecho",
            "sem fecho",
        ),
        ("</span>orfao", "orfao"),
        ("<script>alerta()</script>", "alerta()"),
        ("<span onclick=\"alert(1)\">clique</span>", "clique"),
        (
            "<span data-note-it-color=\"vermelho\">invalida</span>",
            "invalida",
        ),
        (
            "<span style=\"color:url(javascript:alert(1))\">url</span>",
            "url",
        ),
        (
            "<span data-note-it-color=\"#DC2626\" data-inesperado=\"x\">extra</span>",
            "extra",
        ),
        (
            "<span style=\"color:#DC2626;position:fixed;z-index:9\">props</span>",
            "props",
        ),
        ("<img src=x onerror=alert(1)>imagem", "imagem"),
        ("<span><mark></span></mark>fora de ordem", "fora de ordem"),
    ];

    for (md, expected) in cases {
        let rendered = text(md);
        assert!(
            rendered.contains(expected),
            "{md:?} lost its text: {rendered:?}"
        );
        assert_no_storage_syntax(&rendered, md);
        assert!(
            !rendered.contains("onclick") && !rendered.contains("onerror"),
            "{md:?} leaked an event attribute: {rendered:?}"
        );
    }

    // A refused colour is refused, not guessed.
    assert_eq!(
        style_of(
            "<span data-note-it-color=\"vermelho\">invalida</span>",
            "invalida"
        )
        .fg,
        style_of("simples", "simples").fg
    );
    assert_eq!(
        style_of(
            "<span style=\"color:url(javascript:alert(1))\">url</span>",
            "url"
        )
        .fg,
        style_of("simples", "simples").fg
    );
    // The authorised property survives its unauthorised neighbours.
    assert_eq!(
        style_of(
            "<span style=\"color:#DC2626;position:fixed;z-index:9\">props</span>",
            "props"
        )
        .fg,
        Some(Color::Rgb(0xDC, 0x26, 0x26))
    );
}

#[test]
fn terminal_control_sequences_from_a_note_never_reach_the_screen() {
    let md = "antes \u{1b}[31mvermelho\u{1b}[0m \u{1b}]0;titulo\u{7}\u{7} depois \u{9b}31m";
    let rendered = text(md);
    for forbidden in ['\u{1b}', '\u{7}', '\u{9b}', '\u{0}'] {
        assert!(
            !rendered.contains(forbidden),
            "{forbidden:?} survived into the presentation: {rendered:?}"
        );
    }
    assert!(rendered.contains("antes"));
    assert!(rendered.contains("vermelho"));
    assert!(rendered.contains("depois"));

    // A numeric entity is not a back door to the same bytes.
    let entity = text("&#27;[31m e &#x1b;[0m");
    assert!(!entity.contains('\u{1b}'), "{entity:?}");
}

#[test]
fn unusual_but_legitimate_text_survives_untouched() {
    assert_eq!(text("café à noite — ação"), "café à noite — ação");
    assert_eq!(text("e\u{0301} combinado"), "e\u{0301} combinado");
    assert_eq!(text("emoji 🇧🇷 👨‍👩‍👧‍👦 ✅"), "emoji 🇧🇷 👨‍👩‍👧‍👦 ✅");

    let long = "á".repeat(20_000);
    let rendered = text(&long);
    assert_eq!(rendered.chars().count(), 20_000);

    let deep = format!(
        "{}fundo{}",
        "<span data-note-it-color=\"#DC2626\">".repeat(200),
        "</span>".repeat(200)
    );
    let rendered = text(&deep);
    assert!(rendered.contains("fundo"));
    assert_no_storage_syntax(&rendered, "deeply nested spans");
}

// ---------------------------------------------------------------------------
// G. Prova visual automatizada com TestBackend
// ---------------------------------------------------------------------------

/// A throwaway store holding one synthetic note built from the shapes the
/// diagnosis found in the graphical editor's output.
struct Fixture {
    _temp: TempDir,
    paths: StorePaths,
    note_id: Uuid,
}

const FIXTURE_NOTE: &str = "\
# tes<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">te H1</span>

## tes<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">te H2</mark>

#### tes<span data-note-it-color=\"#64748B\" style=\"color:#64748B\"><mark data-note-it-highlight=\"#BBF7D0\" style=\"background-color:#BBF7D0\">te H4</mark></span>

##### teste H5

###### teste H6

<span data-note-it-color=\"#DB2777\" style=\"color:#DB2777\">cores</span>

**teste de negrito**

***negrito com italico***

~~teste de rabisco~~

<u>teste de sublinhado</u>

&nbsp;

<!-- esse é um comentário de teste -->

- [ ] teste de tarefa
- [ ] **tarefa em negrito**
- [x] tarefa fechada <!-- note-it:completed_at=2026-09-06T10:00:00Z -->
";

impl Fixture {
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
            trash_dir,
            backups_dir: base.join("backup"),
            assets_dir: base.join("assets"),
            config_dir: base.join("config"),
            state_dir: base.join("state"),
            runtime_dir: base.join("runtime"),
        };

        let note_id = Uuid::new_v4();
        let mut doc = NoteDocument::new_with_id(note_id);
        doc.content = FIXTURE_NOTE.to_string();
        doc.metadata.created_at = Some(Utc::now());
        doc.metadata.updated_at = Some(Utc::now());
        fs::write(
            notes_dir.join(format!("{note_id}.md")),
            doc.serialize().unwrap(),
        )
        .unwrap();

        Self {
            _temp: temp,
            paths,
            note_id,
        }
    }

    fn app(&self) -> App {
        App::new_at(self.paths.clone(), Arc::new(AtomicBool::new(false)))
    }
}

fn buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_drawn_buffer_shows_words_and_not_storage() {
    let fixture = Fixture::new();
    let mut app = fixture.app();
    app.load_note(fixture.note_id);

    let mut terminal = Terminal::new(TestBackend::new(160, 60)).unwrap();
    app.draw(&mut terminal).unwrap();
    let drawn = buffer_to_string(&terminal);

    assert_no_storage_syntax(&drawn, "drawn reader buffer");
    for expected in [
        "teste H1",
        "teste H2",
        "teste H4",
        "cores",
        "teste de negrito",
        "negrito com italico",
        "teste de rabisco",
        "teste de sublinhado",
        "tarefa em negrito",
    ] {
        assert!(drawn.contains(expected), "buffer misses {expected:?}");
    }
    assert!(
        !drawn.contains("**"),
        "an interpreted delimiter reached the screen:\n{drawn}"
    );
    assert!(!drawn.contains("~~"));
}

#[test]
fn the_drawn_cells_carry_the_colours_the_note_asked_for() {
    let fixture = Fixture::new();
    let mut app = fixture.app();
    app.load_note(fixture.note_id);

    let mut terminal = Terminal::new(TestBackend::new(160, 60)).unwrap();
    app.draw(&mut terminal).unwrap();
    let buffer = terminal.backend().buffer().clone();

    let cell_at = |needle: &str, offset: u16| {
        for y in 0..buffer.area.height {
            let mut row = String::new();
            for x in 0..buffer.area.width {
                row.push_str(buffer[(x, y)].symbol());
            }
            if let Some(index) = row.find(needle) {
                let column = row[..index].chars().count() as u16 + offset;
                return buffer[(column, y)].clone();
            }
        }
        panic!("{needle:?} was never drawn");
    };

    // A colour the note asked for is on the cell, not in the words.
    let pink = cell_at("cores", 0);
    assert_eq!(pink.fg, Color::Rgb(0xDB, 0x27, 0x77));

    // The highlight paints the background and darkens the text.
    let highlighted = cell_at("te H2", 0);
    assert_eq!(highlighted.bg, Color::Rgb(0xFD, 0xE6, 0x8A));
    assert_eq!(highlighted.fg, HIGHLIGHT_FG);

    // Emphasis is a modifier on the cell.
    assert!(cell_at("teste de negrito", 0)
        .modifier
        .contains(Modifier::BOLD));
    assert!(cell_at("teste de rabisco", 0)
        .modifier
        .contains(Modifier::CROSSED_OUT));
    assert!(cell_at("teste de sublinhado", 0)
        .modifier
        .contains(Modifier::UNDERLINED));
}

#[test]
fn the_pending_tasks_panel_shows_task_text_and_not_its_markup() {
    let fixture = Fixture::new();
    let mut app = fixture.app();
    app.panel = noteit_tui::app::ActivePanel::PendingTasks;

    let mut terminal = Terminal::new(TestBackend::new(160, 60)).unwrap();
    app.draw(&mut terminal).unwrap();
    let drawn = buffer_to_string(&terminal);

    assert!(drawn.contains("tarefa em negrito"));
    assert!(
        !drawn.contains("**"),
        "the tasks panel spells a delimiter:\n{drawn}"
    );
}
