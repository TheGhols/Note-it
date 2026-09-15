//! What the wikilink parser promises beyond the corpus: totality,
//! losslessness, determinism, independence from the store, and a cost that
//! stays linear when the document is hostile.
//!
//! The adversarial sizes here are chosen so that a quadratic parser fails by
//! not finishing rather than by missing an assertion. Two hundred thousand
//! openers is a few milliseconds of linear work and something near 10^10
//! operations of quadratic work, so "this test completed" is itself the
//! measurement. Wall-clock thresholds are left to the ignored release harness
//! in `link_parser_performance.rs`, where a slow shared runner cannot turn a
//! correct parser red.

use noteit_core::link::{parse_references, NoteReference, ReferenceKind, MAX_COMPONENT_SCALARS};
use noteit_core::{NoteDocument, NoteItCore, StorageManager};

// ------------------------------------------------------------------ helpers

/// Everything ADR-065 requires of a parse, asserted at once.
fn assert_contract<'a>(source: &'a str, what: &str) -> Vec<NoteReference<'a>> {
    let references = parse_references(source);
    let mut rebuilt = String::with_capacity(source.len());
    let mut cursor = 0;

    for reference in &references {
        assert!(
            reference.start_byte >= cursor,
            "{what}: references overlap or leave source order"
        );
        assert!(
            reference.start_byte < reference.end_byte && reference.end_byte <= source.len(),
            "{what}: a reference has an impossible span"
        );
        assert!(
            source.is_char_boundary(reference.start_byte)
                && source.is_char_boundary(reference.end_byte),
            "{what}: an offset splits a UTF-8 scalar"
        );
        assert_eq!(
            reference.raw,
            &source[reference.start_byte..reference.end_byte],
            "{what}: raw is not the slice the offsets name"
        );
        // A reference always names something: a note, or a section or block of
        // the note it is written in.
        assert!(
            reference.note.is_some() || reference.section.is_some() || reference.block.is_some(),
            "{what}: a reference points at nothing"
        );
        if reference.kind == ReferenceKind::Embed {
            assert!(
                reference.display.is_none(),
                "{what}: an embed carries a display text"
            );
        }
        for value in [&reference.note, &reference.section, &reference.display]
            .into_iter()
            .flatten()
        {
            assert!(!value.is_empty(), "{what}: an empty component survived");
            assert!(
                value.chars().count() <= MAX_COMPONENT_SCALARS,
                "{what}: a component is over the ceiling"
            );
            assert_eq!(
                value.trim(),
                value.as_ref(),
                "{what}: a component kept its padding"
            );
        }

        rebuilt.push_str(&source[cursor..reference.start_byte]);
        rebuilt.push_str(reference.raw);
        cursor = reference.end_byte;
    }

    rebuilt.push_str(&source[cursor..]);
    assert_eq!(
        rebuilt.as_bytes(),
        source.as_bytes(),
        "{what}: parsing did not reproduce its input"
    );
    references
}

/// xorshift64*, so the same seed is the same thousand documents on every
/// machine. A fuzz run nobody can reproduce is an anecdote.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, ceiling: usize) -> usize {
        (self.next() % ceiling as u64) as usize
    }
}

/// The characters that decide something: every piece of structural punctuation
/// the grammar knows, the constructs that outrank it, and scalars that are not
/// one byte wide.
const ALPHABET: [&str; 29] = [
    "[", "]", "!", "#", "^", "|", "\\", "`", "<", ">", "(", ")", "-", "a", "é", "血", " ", "\t",
    "\n", "\r", "~", "=", ":", "\"", "'", "/", "\u{2028}", "\u{7}", "```",
];

// ------------------------------------------------------------- totality

#[test]
fn no_document_makes_the_parser_panic_or_lose_a_byte() {
    let mut rng = Rng(0x6A1D_5EED_C0FF_EE01);
    for case in 0..4000 {
        let length = rng.below(120);
        let mut source = String::new();
        for _ in 0..length {
            source.push_str(ALPHABET[rng.below(ALPHABET.len())]);
        }
        assert_contract(&source, &format!("random case {case}"));
    }
}

#[test]
fn parsing_the_same_document_twice_gives_the_same_answer() {
    let mut rng = Rng(0x0D15_EA5E_D00D_2024);
    for _ in 0..1500 {
        let length = rng.below(160);
        let mut source = String::new();
        for _ in 0..length {
            source.push_str(ALPHABET[rng.below(ALPHABET.len())]);
        }
        let once = parse_references(&source);
        let twice = parse_references(&source);
        assert_eq!(once, twice, "parsing is not a function of its input");
    }
}

// ----------------------------------------------------- syntax is not identity

#[test]
fn what_the_store_holds_cannot_change_how_a_document_parses() {
    let source = "[[AVC]] e [[Cardiologia#Tratamento]] e ![[Neurologia^abc123]]";
    let from_nothing = parse_references(source);

    let root = tempfile::tempdir().expect("tempdir");
    let storage = StorageManager::with_custom_paths(
        root.path().join("notes"),
        root.path().join("config"),
        root.path().join("state"),
        root.path().join("runtime"),
    )
    .expect("isolated store");
    let core = NoteItCore::from_storage(storage);
    for name in ["AVC", "Cardiologia", "Neurologia"] {
        let mut note = NoteDocument::new_empty();
        note.set_title(Some(name)).expect("title");
        core.storage().save_note_atomic(&note).expect("save");
    }

    assert_eq!(
        parse_references(source),
        from_nothing,
        "the parser read the store: a body must mean the same thing on every machine"
    );
    // And what it produced is text, never an identity.
    assert_eq!(from_nothing[0].note.as_deref(), Some("AVC"));
}

// --------------------------------------------------------------- adversarial

#[test]
fn an_opener_with_no_closer_in_two_mebibytes_is_text() {
    let source = format!("[[{}", "a".repeat(2 * 1024 * 1024));
    let references = assert_contract(&source, "2 MiB unclosed opener");
    assert!(references.is_empty());
}

#[test]
fn ten_thousand_links_are_ten_thousand_results_in_source_order() {
    let source = (0..10_000)
        .map(|index| format!("[[Nota {index}]]"))
        .collect::<Vec<_>>()
        .join(" ");
    let references = assert_contract(&source, "10 000 links");
    assert_eq!(references.len(), 10_000);
    for (index, reference) in references.iter().enumerate() {
        assert_eq!(
            reference.note.as_deref(),
            Some(format!("Nota {index}").as_str())
        );
        assert_eq!(reference.kind, ReferenceKind::Link);
    }
}

/// Shapes whose cost is the point. A parser that re-reads a refused candidate,
/// or restarts inside one, does not return from these.
#[test]
fn hostile_repetition_stays_linear() {
    let sizes = [200_000usize];
    for size in sizes {
        for (what, source) in [
            ("openers", "[".repeat(size)),
            ("wikilink openers", "[[".repeat(size)),
            ("closers", "]".repeat(size)),
            ("bangs", "![".repeat(size)),
            ("escapes", "\\[".repeat(size)),
            ("backticks", "`".repeat(size)),
            ("angle brackets", "<a".repeat(size)),
            ("empty candidates", "[[]]".repeat(size)),
            ("nested openers", "[[[".repeat(size)),
            ("unclosed inline links", "[a](".repeat(size)),
        ] {
            assert_contract(&source, what);
        }
    }
}

#[test]
fn excess_brackets_after_a_reference_are_text() {
    for (source, expected) in [("[[a]]]", "[[a]]"), ("[[a]]]]", "[[a]]")] {
        let references = assert_contract(source, source);
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].raw, expected);
        assert_eq!(references[0].start_byte, 0);
        assert_eq!(references[0].end_byte, expected.len());
    }
}

#[test]
fn a_refused_candidate_is_never_read_again_as_a_smaller_one() {
    // The inner `[[a]]` is inside a candidate that failed on its unescaped
    // bracket. Re-scanning from inside it would invent a reference nobody
    // wrote.
    for source in ["[[[a]]", "[[a[b]]", "[[a]b]]", "[[[[a]]"] {
        assert!(
            assert_contract(source, source).is_empty(),
            "{source}: a refused candidate was re-read"
        );
    }
}

#[test]
fn components_at_the_ceiling_are_accepted_and_one_scalar_past_it_is_not() {
    for (count, expected) in [(MAX_COMPONENT_SCALARS, 1), (MAX_COMPONENT_SCALARS + 1, 0)] {
        for shape in ["[[{}]]", "[[a#{}]]", "[[a|{}]]"] {
            let source = shape.replace("{}", &"é".repeat(count));
            let references = assert_contract(&source, &source[..16]);
            assert_eq!(
                references.len(),
                expected,
                "a component of {count} scalars in {shape}"
            );
        }
    }
}

#[test]
fn a_block_identifier_is_accepted_only_between_six_and_sixty_four_characters() {
    for (id, expected) in [
        ("abc12", 0),
        ("abc123", 1),
        ("a".repeat(64).as_str(), 1),
        ("a".repeat(65).as_str(), 0),
        ("abc-123", 1),
        ("-abc123", 0),
        ("abc123-", 0),
        ("Abc123", 0),
        ("abç123", 0),
    ] {
        let source = format!("[[Nota^{id}]]");
        assert_eq!(
            assert_contract(&source, &source).len(),
            expected,
            "block id {id:?}"
        );
    }
}

#[test]
fn unicode_names_keep_their_bytes_and_their_offsets() {
    let source = "antes [[血圧管理#経過観察|よう]] e ![[血圧管理^abc123]] depois";
    let references = assert_contract(source, source);
    assert_eq!(references.len(), 2);

    let link = &references[0];
    assert_eq!(link.kind, ReferenceKind::Link);
    assert_eq!(link.note.as_deref(), Some("血圧管理"));
    assert_eq!(link.section.as_deref(), Some("経過観察"));
    assert_eq!(link.display.as_deref(), Some("よう"));
    assert_eq!(link.start_byte, "antes ".len());
    assert_eq!(link.raw, "[[血圧管理#経過観察|よう]]");
    assert_eq!(link.note_raw, Some("血圧管理"));

    let embed = &references[1];
    assert_eq!(embed.kind, ReferenceKind::Embed);
    assert_eq!(embed.block.as_deref(), Some("abc123"));
    assert!(embed.display.is_none());
    // The `!` belongs to the span it opens.
    assert_eq!(&source[embed.start_byte..embed.start_byte + 3], "![[");
}

/// ADR-065 §8: a transclusion has nothing to substitute, so an embed carrying
/// a display is refused whole rather than accepted with the field ignored.
#[test]
fn an_embed_may_not_carry_a_display_text() {
    for source in ["![[a|b]]", "![[血圧管理#経過観察|よう]]", "![[a^abc123|b]]"] {
        assert!(
            assert_contract(source, source).is_empty(),
            "{source}: an embed was allowed a display text"
        );
    }
}

#[test]
fn a_document_of_only_text_produces_nothing_and_costs_one_pass() {
    let source = "texto sem referência\n".repeat(131_072);
    assert!(assert_contract(&source, "131 072 plain lines").is_empty());
}

// ------------------------------------------- readings the corpus implies

/// CR and LF end a candidate; every other whitespace scalar is a character a
/// name may carry. This is ADR-065's third review round, which is the only
/// reason U+2028 and U+2029 have a settled answer at all.
#[test]
fn only_cr_and_lf_end_a_candidate() {
    assert_eq!(
        assert_contract("linha\r\n[[AVC]]\r\nfim", "crlf document")[0]
            .note
            .as_deref(),
        Some("AVC")
    );
    assert!(assert_contract("[[A\r\nB]]", "crlf inside").is_empty());
    assert!(assert_contract("[[A\nB]]", "lf inside").is_empty());

    // A format character is text; a control character is not.
    assert_eq!(
        assert_contract("[[a\u{200D}b]]", "zero-width joiner")[0]
            .note
            .as_deref(),
        Some("a\u{200D}b")
    );
    assert_eq!(
        assert_contract("[[a\u{2029}b]]", "paragraph separator")[0]
            .note
            .as_deref(),
        Some("a\u{2029}b"),
        "U+2029 is a scalar inside a name, not a Markdown line break"
    );
    assert!(assert_contract("[[a\u{7}b]]", "bell").is_empty());
    assert!(assert_contract("[[\u{a0}]]", "only a no-break space").is_empty());
}

/// The closer is the first `]]` that is not escaped, which is what lets a name
/// end in a bracket.
#[test]
fn an_escaped_bracket_does_not_close_a_candidate() {
    assert!(assert_contract("[[a\\]]", "escaped closer, nothing after").is_empty());
    // `\]` is escaped, so the candidate closes on the *first* unescaped `]]`
    // and the bracket left over is text.
    let references = assert_contract("[[a\\]]]]", "escaped closer then a real one");
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].note.as_deref(), Some("a]"));
    assert_eq!(references[0].raw, "[[a\\]]]");
    assert_eq!(references[0].end_byte, 7);
}

/// `!` is an embed only when it is joined to the opener.
#[test]
fn only_the_bang_touching_the_opener_makes_an_embed() {
    for (source, kind, start) in [
        ("![[a]]", ReferenceKind::Embed, 0),
        ("!![[a]]", ReferenceKind::Embed, 1),
        ("!!![[a]]", ReferenceKind::Embed, 2),
        ("! [[a]]", ReferenceKind::Link, 2),
        ("\\![[a]]", ReferenceKind::Link, 2),
    ] {
        let references = assert_contract(source, source);
        assert_eq!(references.len(), 1, "{source}");
        assert_eq!(references[0].kind, kind, "{source}");
        assert_eq!(references[0].start_byte, start, "{source}");
    }
}

/// A canonical wrapper is structure; what it wraps is still Markdown. Anything
/// else is opaque.
#[test]
fn canonical_wrappers_keep_their_content_and_unknown_html_does_not() {
    let references = assert_contract("<mark>[[A]]</mark> <u>[[B]]</u>", "canonical wrappers");
    assert_eq!(references.len(), 2);
    assert_eq!(references[0].note.as_deref(), Some("A"));
    assert_eq!(references[1].note.as_deref(), Some("B"));

    assert!(assert_contract("<div>[[A]]</div>", "unknown element").is_empty());
    assert!(assert_contract("<div>[[A]]", "unclosed unknown element").is_empty());
    // A void element holds nothing, so it ends where its tag does.
    assert_eq!(
        assert_contract("a<br>[[A]]", "void element")[0]
            .note
            .as_deref(),
        Some("A")
    );
}

/// Where a wikilink is still a wikilink: the ADR's "sim" column.
#[test]
fn structure_that_only_marks_a_line_leaves_its_inlines_alone() {
    for source in [
        "# Título com [[AVC]]",
        "> citação [[AVC]]",
        "- [ ] fazer [[AVC]]",
        "- [x] feito [[AVC]]",
        "**[[AVC]]**",
        "> [!NOTE]\n> Veja [[AVC]]",
        "[[AVC]] :: pressão elevada",
        "```\nx\n```\n\n[[AVC]]",
    ] {
        let references = assert_contract(source, source);
        assert_eq!(references.len(), 1, "{source}");
        assert_eq!(references[0].note.as_deref(), Some("AVC"), "{source}");
    }
}

/// Four spaces are code where a paragraph could not be continuing, and prose
/// where one is.
#[test]
fn an_indented_line_is_code_only_where_a_block_could_start() {
    assert!(assert_contract("    [[AVC]]", "indented at the start").is_empty());
    assert!(assert_contract("\t[[AVC]]", "tab indented").is_empty());
    assert_eq!(
        assert_contract("texto\n    [[AVC]]", "lazy continuation")[0]
            .note
            .as_deref(),
        Some("AVC"),
        "four spaces inside a paragraph continue it"
    );
}
