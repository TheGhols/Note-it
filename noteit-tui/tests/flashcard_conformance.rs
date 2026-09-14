//! Checkpoint 5.0D.5.C — flashcards, held to the graphical editor's own
//! extraction.
//!
//! `tests/fixtures/flashcard-conformance.json` was generated from
//! `ui/src/flashcards/extract.ts` over Markdown notes, and is asserted from
//! both sides. The two implementations read different things — the graphical
//! one reads its ProseMirror document, this one reads the lossless projection —
//! and that is exactly why they need a shared written-down expectation rather
//! than a comparison with each other.

#[path = "support/json.rs"]
mod json;

use noteit_tui::flashcards::{count, extract, review_items};

fn fixture() -> json::Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/flashcard-conformance.json"
    );
    let text = std::fs::read_to_string(path).expect("the cross-conformance fixture");
    json::parse(&text)
}

#[test]
fn the_fixture_covers_the_vocabulary() {
    assert!(fixture().get("notes").expect("notes").array().len() > 20);
}

#[test]
fn every_note_extracts_exactly_the_cards_the_graphical_editor_extracts() {
    for note in fixture().get("notes").expect("notes").array() {
        let markdown = note.get("markdown").unwrap().string().unwrap();
        let cards = extract(markdown);
        let expected = note.get("cards").expect("cards").array();

        assert_eq!(
            cards.len(),
            expected.len(),
            "card count for {markdown:?}: got {cards:?}"
        );

        for (index, (card, wanted)) in cards.iter().zip(expected).enumerate() {
            assert_eq!(
                card.front,
                wanted.get("front").unwrap().string().unwrap(),
                "front of card {index} in {markdown:?}"
            );
            assert_eq!(
                card.back,
                wanted.get("back").unwrap().string().unwrap(),
                "back of card {index} in {markdown:?}"
            );
            assert_eq!(
                card.mode.name(),
                wanted.get("mode").unwrap().string().unwrap(),
                "mode of card {index} in {markdown:?}"
            );
            assert_eq!(
                card.form.name(),
                wanted.get("form").unwrap().string().unwrap(),
                "form of card {index} in {markdown:?}"
            );
        }
    }
}

#[test]
fn the_counts_agree_about_cards_and_about_reviews() {
    for note in fixture().get("notes").expect("notes").array() {
        let markdown = note.get("markdown").unwrap().string().unwrap();
        let cards = extract(markdown);
        let counts = count(&cards);
        let wanted = note.get("counts").expect("counts");

        assert_eq!(
            counts.cards as f64,
            wanted.get("cards").unwrap().number().unwrap(),
            "card count for {markdown:?}"
        );
        assert_eq!(
            counts.reviews as f64,
            wanted.get("reviews").unwrap().number().unwrap(),
            "review count for {markdown:?} — a reversible card is two questions"
        );
    }
}

#[test]
fn the_review_items_run_in_the_same_order_and_the_same_directions() {
    for note in fixture().get("notes").expect("notes").array() {
        let markdown = note.get("markdown").unwrap().string().unwrap();
        let items = review_items(&extract(markdown));
        let expected = note.get("reviews").expect("reviews").array();

        assert_eq!(items.len(), expected.len(), "review count for {markdown:?}");
        for (index, (item, wanted)) in items.iter().zip(expected).enumerate() {
            assert_eq!(
                item.direction.name(),
                wanted.get("direction").unwrap().string().unwrap(),
                "direction of review {index} in {markdown:?}"
            );
            assert_eq!(
                item.source as f64,
                wanted.get("source").unwrap().number().unwrap(),
                "source card of review {index} in {markdown:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Properties the fixture cannot state
// ---------------------------------------------------------------------------

#[test]
fn extraction_never_writes_a_byte() {
    // The design in one assertion: a card is a projection. There is no parallel
    // file, no identifier smuggled into a comment, and no rewriting of the
    // note. Change the words and the card changes; delete the delimiter and the
    // card stops existing.
    for markdown in [
        "Pergunta :: Resposta",
        "Termo ::: Definição",
        "Pergunta\n\n::\n\nResposta",
        "nada aqui",
    ] {
        let before = markdown.to_owned();
        let _ = review_items(&extract(markdown));
        assert_eq!(markdown, before, "{markdown:?} was modified");
    }
}

#[test]
fn a_delimiter_inside_protected_source_is_not_a_delimiter() {
    // The reason this reads the projection rather than the text: only the
    // projection knows these are not places a card can live.
    for markdown in [
        "```\nPergunta :: Resposta\n```",
        "`Pergunta :: Resposta`",
        "<custom>Pergunta :: Resposta</custom>",
        "[rótulo](https://example.com/a :: b)",
        "<!-- Pergunta :: Resposta -->",
    ] {
        assert!(
            extract(markdown).is_empty(),
            "{markdown:?} should hold no card, got {:?}",
            extract(markdown)
        );
    }
}

#[test]
fn editing_a_note_changes_its_cards_with_it() {
    // No stale state: the cards are derived every time from the text in hand.
    let cards = extract("Pergunta :: Resposta");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].back, "Resposta");

    let edited = extract("Pergunta :: Outra resposta");
    assert_eq!(edited[0].back, "Outra resposta");

    let removed = extract("Pergunta Resposta");
    assert!(
        removed.is_empty(),
        "removing the delimiter removes the card"
    );
}

#[test]
fn unicode_and_multiline_sides_survive_intact() {
    let cards = extract("Acentuação ç 日本語 👨\u{200D}👩\u{200D}👧\u{200D}👦 :: resposta ã");
    assert_eq!(cards.len(), 1);
    assert!(cards[0].front.contains('ç'));
    assert!(cards[0].front.contains("日本語"));
    assert!(cards[0].front.contains('👨'));
    assert_eq!(cards[0].back, "resposta ã");
}

#[test]
fn malformed_and_hostile_notes_extract_without_panicking() {
    for markdown in [
        ":::::::::::",
        "::",
        ":",
        " :: ",
        "a\u{0}b :: c",
        "<x a=\" :: ",
        "```sem fecho\n:: dentro",
        &":: ".repeat(500),
        "",
    ] {
        let cards = extract(markdown);
        let _ = count(&cards);
        let _ = review_items(&cards);
    }
}
