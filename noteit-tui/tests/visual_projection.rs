//! Gate 5.0D.4B.1 — lossless projection foundation.
//!
//! Every property here is one of the machine-checked invariants listed in
//! `docs/tui.md` §26.15. They are written against the *physical* partition —
//! `Lexeme` — because that is the only layer the losslessness contract binds:
//! nodes may nest and overlap by ancestry, runs are derived, but the lexemes
//! are a flat partition of the bytes and nothing else is.
//!
//! There is no UI, no cursor and no grapheme here on purpose. B.1 is the layer
//! below all three, and a test that needed a terminal to prove a byte range
//! would be proving something else.

use noteit_tui::projection::{project, Classification, LexemeKind, NodeKind};
use noteit_tui::source_map::{Generation, SourceRange};

// ---------------------------------------------------------------------------
// The six coverage properties (§26.15.1 - §26.15.6)
// ---------------------------------------------------------------------------

/// Asserts the whole physical contract at once, for any source at all.
///
/// Returning the projection lets a caller go on to assert something specific
/// about the same value it just proved well formed.
#[track_caller]
fn assert_lossless(source: &str) {
    let projection = project(source, Generation::first());
    let lexemes = projection.lexemes();

    assert_eq!(
        projection.source_len(),
        source.len(),
        "projection must record the byte length it was built from"
    );

    let mut previous_end = 0usize;
    let mut rebuilt = String::with_capacity(source.len());

    for lexeme in lexemes {
        let range = lexeme.source;
        let (start, end) = (range.start(), range.end());

        // 1. ordered, and 2. non-overlapping, and 3. gapless are one statement
        // when checked against the running end: anything but `start == previous`
        // is a gap, an overlap or a step backwards.
        assert_eq!(
            start, previous_end,
            "lexeme ranges must be ordered, disjoint and gapless: {range:?} after {previous_end}"
        );
        assert!(start <= end, "range must not be inverted: {range:?}");

        // 6. every boundary is a valid UTF-8 boundary.
        assert!(
            source.is_char_boundary(start),
            "range start {start} is not a UTF-8 boundary"
        );
        assert!(
            source.is_char_boundary(end),
            "range end {end} is not a UTF-8 boundary"
        );

        rebuilt.push_str(&source[start..end]);
        previous_end = end;
    }

    // 4. the partition covers exactly `0..source_len`.
    assert_eq!(
        previous_end,
        source.len(),
        "lexemes must cover exactly 0..source_len"
    );

    // 5. concatenating the slices reproduces the original bytes.
    assert_eq!(
        rebuilt.as_bytes(),
        source.as_bytes(),
        "concatenated lexeme slices must be byte-identical to the source"
    );
}

#[test]
fn the_empty_source_is_covered_by_nothing_and_round_trips() {
    assert_lossless("");
    let projection = project("", Generation::first());
    assert_eq!(projection.source_len(), 0);
    assert!(projection.lexemes().is_empty());
}

#[test]
fn plain_text_paragraphs_are_covered_byte_exactly() {
    for source in [
        "abc",
        "abc\n",
        "\n",
        "\n\n\n",
        "um parágrafo\n\noutro parágrafo\n",
        "trailing spaces   \n\ttab indented\n",
    ] {
        assert_lossless(source);
    }
}

#[test]
fn every_recognised_construction_stays_byte_exact() {
    for source in [
        "# Meu título\n",
        "###### H6\n",
        "####### not a heading\n",
        "#no space\n",
        "**abc**",
        "*abc*",
        "***abc***",
        "~~abc~~",
        "<u>abc</u>",
        "`code`",
        "``code with ` tick``",
        "[a **bold** label](https://example.com/a_(b))",
        "- item\n- outro\n",
        "1. um\n2. dois\n",
        "- [ ] pendente\n- [x] feita\n",
        "> citação\n> continua\n",
        "> [!NOTE]\n> corpo\n",
        "---\n",
        "```rust\nfn main() {}\n```\n",
        "```sem fecho\nresto\n",
        "<!-- comentário -->\n",
        "<!-- sem fecho\nresto\n",
        "&amp; &lt; &gt; &quot; &apos; &nbsp;",
        "&desconhecida; fica literal",
        "\\*escapado\\*",
        "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">x</span>",
        "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">y</mark>",
    ] {
        assert_lossless(source);
    }
}

#[test]
fn line_endings_are_physical_lexemes_and_are_never_normalised() {
    for source in ["a\r\nb", "a\nb", "a\r\nb\nc\r\n", "\r\n"] {
        assert_lossless(source);
    }

    // §26.3: LF is one byte, CRLF is two, and neither becomes the other.
    let projection = project("a\r\nb\nc", Generation::first());
    let endings: Vec<&str> = projection
        .lexemes()
        .iter()
        .filter(|lexeme| lexeme.kind == LexemeKind::LineEnding)
        .map(|lexeme| &"a\r\nb\nc"[lexeme.source.start()..lexeme.source.end()])
        .collect();
    assert_eq!(endings, vec!["\r\n", "\n"]);
}

#[test]
fn unicode_is_preserved_and_never_split() {
    for source in [
        "café",             // NFC
        "cafe\u{0301}",     // NFD: combining acute
        "👨‍👩‍👧‍👦 família",       // ZWJ sequence
        "🇧🇷 bandeira",      // regional indicators
        "👍🏽 skin tone",     // modifier
        "日本語のテキスト", // CJK
        "a\u{200B}b",       // zero width space
        "**ç**",
        "# título com ç e ã\n",
    ] {
        assert_lossless(source);
    }
}

// ---------------------------------------------------------------------------
// The normative HTML boundary table (§26.10)
// ---------------------------------------------------------------------------

/// The single Opaque/Protected span the table demands, as `(start, end)`.
#[track_caller]
fn opaque_span(source: &str) -> Option<(usize, usize)> {
    assert_lossless(source);
    let projection = project(source, Generation::first());
    projection
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::Opaque)
        .map(|node| (node.coverage.start(), node.coverage.end()))
}

#[test]
fn the_normative_html_boundary_table_holds_exactly() {
    // Each row is (source, expected opaque span) straight out of §26.10.
    let rows: &[(&str, (usize, usize))] = &[
        ("<x>abc</x>", (0, 10)),
        ("<x>a\nb</x>", (0, 10)),
        ("<x>a\n# h", (0, 8)),
        ("<x><y>z</y></x>", (0, 15)),
        ("<x><y></x>", (0, 10)),
        ("<x a=\">\">ok", (0, 11)),
        ("<x>**b**</x>", (0, 12)),
        ("pré <x>ç **b**</x> pós", (5, 20)),
        ("<span data-note-it-color=\"#DC2626\">x", (0, 36)),
    ];

    for (source, expected) in rows {
        assert_eq!(
            opaque_span(source),
            Some(*expected),
            "§26.10 span for {source:?}"
        );
    }

    // The three rows whose end is stated as `source_len` really are EOF.
    for source in ["<x>a\n# h", "<x><y></x>", "<x a=\">\">ok"] {
        let (_, end) = opaque_span(source).expect("an opaque region");
        assert_eq!(end, source.len(), "{source:?} must protect to EOF");
    }
}

#[test]
fn an_unclosed_tag_protects_to_eof_and_the_text_after_it_is_not_editable() {
    // §26.10: the paragraph on the second line is *not* a block of its own.
    for source in [
        "<custom-widget>hello\nparagraph",
        "<span data-note-it-color=\"#DC2626\">hello\nparagraph",
    ] {
        let span = opaque_span(source).expect("an opaque region");
        assert_eq!(span, (0, source.len()), "{source:?}");

        let projection = project(source, Generation::first());
        assert!(
            projection
                .nodes()
                .iter()
                .all(|node| node.kind != NodeKind::Paragraph || node.coverage.start() >= span.0),
            "no paragraph may be recognised outside the protected region"
        );
    }
}

#[test]
fn literal_comparison_operators_never_become_html() {
    // §26.15.25 — the whole text stays literal and there is no Opaque at all.
    let source = "2 < 3 and 4 > 1";
    assert_eq!(opaque_span(source), None);

    let projection = project(source, Generation::first());
    let text: String = projection
        .lexemes()
        .iter()
        .filter(|lexeme| lexeme.kind == LexemeKind::Text)
        .map(|lexeme| &source[lexeme.source.start()..lexeme.source.end()])
        .collect();
    assert_eq!(text, source, "every byte must be ordinary text");
}

#[test]
fn a_lone_orphan_close_tag_is_malformed_to_eof() {
    // §26.10 step 6.
    let source = "</x> resto";
    assert_eq!(opaque_span(source), Some((0, source.len())));
}

#[test]
fn void_and_self_closing_tags_need_no_close() {
    for source in ["<br>depois", "<img src=\"a.png\">depois", "<x/>depois"] {
        let span = opaque_span(source).expect("an opaque region");
        assert!(
            span.1 < source.len(),
            "{source:?} is self-contained and must not reach EOF: {span:?}"
        );
    }
}

#[test]
fn markdown_inside_an_opaque_region_is_never_reclassified() {
    let source = "<x>**b** # h\n- item</x>";
    assert_eq!(opaque_span(source), Some((0, source.len())));

    let projection = project(source, Generation::first());
    for node in projection.nodes() {
        assert!(
            !matches!(
                node.kind,
                NodeKind::Strong | NodeKind::Heading(_) | NodeKind::ListItem
            ),
            "no Markdown node may be built inside an Opaque region, found {:?}",
            node.kind
        );
    }
}

#[test]
fn an_unknown_ancestor_makes_every_descendant_protected() {
    // §26.15.10 — ancestor dominance, including over canonical descendants.
    let source =
        "<custom-widget foo=\"bar\"><span data-note-it-color=\"#DC2626\">hello</span></custom-widget>";
    assert_eq!(opaque_span(source), Some((0, source.len())));

    let projection = project(source, Generation::first());
    let opaque = projection
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::Opaque)
        .expect("an opaque node");

    // Walk the region's own subtree. The paragraph that *contains* it is its
    // parent, not its descendant, and dominance runs downwards only.
    let mut frontier = opaque.children.clone();
    let mut seen = 0;
    while let Some(id) = frontier.pop() {
        seen += 1;
        assert_eq!(
            projection.effective_classification(id),
            Classification::OPAQUE,
            "descendant {:?} must inherit Opaque",
            projection.node(id).kind
        );
        frontier.extend(projection.node(id).children.iter().copied());
    }
    assert_eq!(
        projection.effective_classification(opaque.id),
        Classification::OPAQUE
    );

    // The canonical `span` inside must not have been recognised as a node of
    // its own: inside an unknown ancestor there is nothing to recognise.
    assert_eq!(seen, 0, "an opaque region has no interpreted children");
    assert!(projection
        .nodes()
        .iter()
        .all(|node| !matches!(node.kind, NodeKind::Color(_))));
}

#[test]
fn the_nesting_budget_is_enforced_without_losing_a_byte() {
    // 33 levels deep: past the budget of 32, so the outermost unresolved
    // ancestor becomes Opaque — and nothing is dropped.
    let deep = "<a>".repeat(33) + &"</a>".repeat(33);
    assert_lossless(&deep);
}

// ---------------------------------------------------------------------------
// Deterministic property tests (§26.15, "property tests geram UTF-8")
// ---------------------------------------------------------------------------

/// A tiny deterministic generator.
///
/// No framework and no randomness that varies between runs: a failing seed
/// here is a failing seed on the next machine too, which is the only kind of
/// property failure worth having in a gate.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        // Numerical Recipes constants; any full-period LCG would do.
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    fn pick<'a>(&mut self, options: &[&'a str]) -> &'a str {
        options[(self.next() % options.len() as u64) as usize]
    }
}

#[test]
fn generated_sources_are_always_lossless() {
    // Fragments chosen to collide: delimiters beside tags, tags beside
    // entities, Unicode beside every one of them.
    const FRAGMENTS: &[&str] = &[
        "a",
        "ç",
        "日",
        "👨‍👩‍👧‍👦",
        " ",
        "\t",
        "\n",
        "\r\n",
        "*",
        "**",
        "***",
        "~~",
        "_",
        "`",
        "\\",
        "\\*",
        "<",
        ">",
        "</",
        "<x>",
        "</x>",
        "<x",
        "<u>",
        "</u>",
        "<br>",
        "<x/>",
        "<span data-note-it-color=\"#DC2626\">",
        "</span>",
        "<mark data-note-it-highlight=\"#FDE68A\">",
        "</mark>",
        "<!--",
        "-->",
        "&amp;",
        "&nbsp;",
        "&",
        ";",
        "&naoexiste;",
        "#",
        "# ",
        "- ",
        "1. ",
        "> ",
        "```",
        "[",
        "]",
        "(",
        ")",
        "!",
        "[label](https://example.com/a_(b))",
        "2 < 3",
        "a=\">\"",
    ];

    let mut rng = Lcg(0x5EED_1234_ABCD_0001);
    for _case in 0..4000 {
        let length = (rng.next() % 12) as usize;
        let mut source = String::new();
        for _ in 0..length {
            source.push_str(rng.pick(FRAGMENTS));
        }
        assert_lossless(&source);
    }
}

#[test]
fn every_prefix_of_a_hostile_document_is_lossless() {
    // Truncation is where a boundary algorithm goes wrong: a tag, an entity or
    // a fence cut in half must still partition cleanly and still fail closed.
    let document = concat!(
        "# Título\n\n",
        "Um parágrafo com **negrito**, `código` e &amp; entidade.\n\n",
        "<span data-note-it-color=\"#DC2626\">colorido</span> e 2 < 3.\n\n",
        "```rust\nfn main() {}\n```\n\n",
        "<!-- comentário -->\n",
        "- [ ] tarefa\n",
        "> [!NOTE]\n> corpo\n",
        "<custom foo=\"a > b\">opaco</custom>\n",
    );

    for end in 0..=document.len() {
        if document.is_char_boundary(end) {
            assert_lossless(&document[..end]);
        }
    }
}

#[test]
fn projection_is_independent_of_any_viewport() {
    // §26.15.21 — there is no width, height or scroll input at all, so the
    // same source must produce an identical partition every time.
    let source = "# t\n\npar **a** <x>b</x>\n";
    let first = project(source, Generation::first());
    let second = project(source, Generation::first());

    let ranges = |p: &noteit_tui::projection::Projection| -> Vec<SourceRange> {
        p.lexemes().iter().map(|lexeme| lexeme.source).collect()
    };
    assert_eq!(ranges(&first), ranges(&second));
}

#[test]
fn a_projection_remembers_the_generation_it_was_built_from() {
    // §26.15.20 needs an identity to reject a stale object by; B.1 establishes
    // it, and B.2 is what starts refusing on it.
    let generation = Generation::first().next();
    let projection = project("abc", generation);
    assert_eq!(projection.generation(), generation);
    assert_ne!(projection.generation(), Generation::first());
}

// ---------------------------------------------------------------------------
// Regressions for the independent review's findings (docs/tui.md §27)
// ---------------------------------------------------------------------------

#[test]
fn b2_a_fence_holding_an_unclosed_tag_does_not_protect_the_rest_of_the_note() {
    // §27.3 step 0. Before the fix, `<div class="card">` inside a ```html
    // fence made every byte to EOF Opaque — in B.1, before any capability.
    let source = "# Notas\n\n```html\n<div class=\"card\">\n```\n\nResto da nota.\n";
    assert_lossless(source);
    assert_eq!(
        opaque_span(source),
        None,
        "a fence must create no Opaque region"
    );

    let projection = project(source, Generation::first());
    let code = projection
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::CodeBlock)
        .expect("a fenced code block");
    assert!(
        code.coverage.end() < source.len(),
        "the fence must end at its closing line, not at EOF"
    );

    // The paragraph after the fence is still its own editable block.
    let after = projection
        .nodes()
        .iter()
        .find(|node| {
            node.kind == NodeKind::Paragraph && node.coverage.start() > code.coverage.end()
        })
        .expect("a paragraph after the fence");
    assert_eq!(
        projection.effective_classification(after.id),
        Classification::EDITABLE
    );
}

#[test]
fn b2_an_inline_code_span_holding_a_tag_does_not_protect_the_rest_of_the_line() {
    let source = "Use `<div>` para o cartão.";
    assert_lossless(source);
    assert_eq!(opaque_span(source), None);

    let projection = project(source, Generation::first());
    assert!(
        projection
            .nodes()
            .iter()
            .any(|node| node.kind == NodeKind::InlineCode),
        "the code span must be recognised"
    );
}

#[test]
fn b2_an_escaped_bracket_starts_no_candidate() {
    let source = "\\<div> continua texto";
    assert_lossless(source);
    assert_eq!(opaque_span(source), None);
}

#[test]
fn b2_an_unbalanced_backtick_leaves_the_tag_a_candidate() {
    // §27.3 is explicit: a code span whose delimiter does not close on the
    // same line protects nothing, and the `<` is a candidate again.
    let source = "Use `<div> sem fechar";
    assert_eq!(opaque_span(source), Some((5, source.len())));
}

#[test]
fn b2_a_close_tag_inside_a_fence_never_closes_an_element() {
    // §26.10 step 4 said fences have no meaning for HTML matching; with step 0
    // that is one rule. The `</x>` in the fence must not balance the opener.
    let source = "<x>abc\n\n```\n</x>\n```\n";
    assert_eq!(opaque_span(source), Some((0, source.len())));
}

#[test]
fn m2_a_slash_ending_an_unquoted_attribute_does_not_close_the_tag() {
    // Before the fix this was fail-OPEN: the element was never opened, so
    // `hello` became editable text inside an unknown wrapper.
    let source = "<custom-widget data-src=a/>hello</custom-widget>";
    assert_eq!(opaque_span(source), Some((0, source.len())));

    let projection = project(source, Generation::first());
    let opaque = projection
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::Opaque)
        .expect("an opaque node");
    assert_eq!(
        projection.effective_classification(opaque.id),
        Classification::OPAQUE
    );

    // A genuine self-closing tag still is one.
    assert_eq!(opaque_span("<custom-widget />after"), Some((0, 17)));
}

#[test]
fn m16_an_ambiguous_delimiter_run_has_exactly_one_reading() {
    // The two typos the R2 procedure could not decide. §27.15 gives each one
    // answer, and neither is a node: a run is *maximal*, so after one fails to
    // pair the scan resumes at the end of that run. The second asterisk of a
    // `**` is therefore never read as the opener of a width-1 pair.
    /// One recognised mark: what it is, and the bytes it covers.
    type Mark = (NodeKind, usize, usize);

    let cases: [(&str, Vec<Mark>); 2] = [("*a**b*", vec![]), ("**a*b**", vec![])];
    for (source, expected) in cases {
        assert_lossless(source);
        let projection = project(source, Generation::first());
        let marks: Vec<_> = projection
            .nodes()
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    NodeKind::Strong | NodeKind::Emphasis | NodeKind::StrongEmphasis
                )
            })
            .map(|node| {
                (
                    node.kind.clone(),
                    node.coverage.start(),
                    node.coverage.end(),
                )
            })
            .collect();
        assert_eq!(marks, expected, "{source:?} must have exactly one reading");
    }

    // And an unambiguous one is still recognised.
    let projection = project("**abc**", Generation::first());
    assert!(projection
        .nodes()
        .iter()
        .any(|node| node.kind == NodeKind::Strong));
}

#[test]
fn m7_an_entity_is_one_atomic_grapheme() {
    let source = "a&amp;b";
    assert_lossless(source);

    let projection = project(source, Generation::first());
    let entity = projection
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::Entity)
        .expect("an entity node");
    assert_eq!(entity.coverage.start(), 1);
    assert_eq!(entity.coverage.end(), 6, "all five bytes belong to it");
    assert!(
        !projection
            .effective_classification(entity.id)
            .admits_interior_caret(),
        "an entity has no interior caret; it is removed whole"
    );
}

#[test]
fn m1_a_malformed_close_tag_is_an_orphan_to_eof() {
    let source = "x </b 3 y";
    assert_eq!(opaque_span(source), Some((2, source.len())));
}

#[test]
fn m3_the_nesting_budget_abandons_matching_and_protects_to_eof() {
    // 33 levels: past the budget, so no close is proved and the span is EOF —
    // and the bytes after the outermost close are inside it.
    let deep = "<a>".repeat(33) + &"</a>".repeat(33) + "depois";
    assert_lossless(&deep);
    assert_eq!(opaque_span(&deep), Some((0, deep.len())));

    // 32 levels is within budget and closes normally.
    let ok = "<a>".repeat(32) + &"</a>".repeat(32) + "depois";
    let (start, end) = opaque_span(&ok).expect("an opaque region");
    assert_eq!(start, 0);
    assert_eq!(end, ok.len() - "depois".len());
}

#[test]
fn m4_an_unterminated_comment_inside_a_candidate_reaches_eof() {
    // §27.5/M4: the `</x>` inside the unterminated comment must not pop.
    let source = "<x>a<!--b</x>c";
    assert_eq!(opaque_span(source), Some((0, source.len())));
}

#[test]
fn m2_lexemes_are_never_empty() {
    // §27.21/m2 — an empty lexeme would make "ordered" ambiguous and let
    // properties 1-5 pass vacuously.
    for source in [
        "",
        "a",
        "# t\n",
        "**a**",
        "<x>a</x>",
        "&amp;",
        "```\nx\n```\n",
    ] {
        let projection = project(source, Generation::first());
        for lexeme in projection.lexemes() {
            assert!(
                !lexeme.source.is_empty(),
                "empty lexeme {:?} in {source:?}",
                lexeme.source
            );
        }
    }
}

#[test]
fn n4_a_balanced_inner_pair_nests_instead_of_making_the_outer_literal() {
    // §28.5. Before the fix, `**a *b* c**` was literal — which would have made
    // applying emphasis inside a bold run produce source the projector could
    // no longer read back, contradicting §26.7's canonical order.
    let source = "**a *b* c**";
    assert_lossless(source);

    let projection = project(source, Generation::first());
    let strong = projection
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::Strong)
        .expect("a strong node");
    assert_eq!((strong.coverage.start(), strong.coverage.end()), (0, 11));

    let emphasis = projection
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::Emphasis)
        .expect("a nested emphasis node");
    assert_eq!((emphasis.coverage.start(), emphasis.coverage.end()), (4, 7));
    assert_eq!(
        emphasis.parent,
        Some(strong.id),
        "the emphasis must be a child of the strong, not a sibling"
    );

    // The mirror case nests the other way round.
    let projection = project("*a **b** c*", Generation::first());
    assert!(projection
        .nodes()
        .iter()
        .any(|node| node.kind == NodeKind::Emphasis));
    assert!(projection
        .nodes()
        .iter()
        .any(|node| node.kind == NodeKind::Strong));
}
