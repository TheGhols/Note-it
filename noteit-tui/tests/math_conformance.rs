//! Checkpoint 5.0D.5.B — the maths engine, held to the graphical editor's own
//! expectations.
//!
//! `tests/fixtures/math-conformance.json` was generated from `ui/src/math/`,
//! the canonical implementation, and is asserted from both sides: `ui/tests/
//! math_conformance.test.ts` there and this file here. Neither implementation
//! is compared to the other — that would only prove they agree — and both are
//! compared to a written-down expectation, so either one drifting fails its own
//! suite with the line that drifted.
//!
//! A note that computes `10 km em m` must show `10000 m` in the terminal and
//! `10000 m` in the window, or the number in a note means nothing.

use noteit_tui::math::{classify_line, evaluate_note, LineResult};

/// One line's expectation, as the fixture spells it.
struct Expected {
    source: String,
    kind: String,
    result_kind: String,
    text: Option<String>,
    code: Option<String>,
}

#[path = "support/json.rs"]
mod json;

fn fixture() -> json::Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/math-conformance.json"
    );
    let text = std::fs::read_to_string(path).expect("the cross-conformance fixture");
    json::parse(&text)
}

fn expectations() -> Vec<Expected> {
    fixture()
        .get("lines")
        .expect("lines")
        .array()
        .iter()
        .map(|entry| {
            let result = entry.get("result").expect("a result");
            Expected {
                source: entry.get("source").unwrap().string().unwrap().to_owned(),
                kind: entry.get("kind").unwrap().string().unwrap().to_owned(),
                result_kind: result.get("kind").unwrap().string().unwrap().to_owned(),
                text: result
                    .get("text")
                    .and_then(|v| v.string())
                    .map(str::to_owned),
                code: result
                    .get("code")
                    .and_then(|v| v.string())
                    .map(str::to_owned),
            }
        })
        .collect()
}

fn shape(result: &LineResult) -> (&'static str, Option<String>, Option<String>) {
    match result {
        LineResult::None => ("none", None, None),
        LineResult::Value { text, .. } => ("value", Some(text.clone()), None),
        LineResult::Error(error) => ("error", None, Some(error.code().to_owned())),
    }
}

#[test]
fn the_fixture_is_present_and_substantial() {
    let fixture = fixture();
    assert!(
        fixture.get("lines").expect("lines").array().len() > 40,
        "the fixture should cover the vocabulary, not a sample of it"
    );
    assert!(fixture.get("notes").expect("notes").array().len() > 5);
}

#[test]
fn every_line_is_classified_exactly_as_the_graphical_editor_classifies_it() {
    for expected in expectations() {
        assert_eq!(
            classify_line(Some(&expected.source)).name(),
            expected.kind,
            "classification of {:?}",
            expected.source
        );
    }
}

#[test]
fn every_line_evaluates_exactly_as_the_graphical_editor_evaluates_it() {
    for expected in expectations() {
        let results = evaluate_note(&[Some(&expected.source)]);
        assert_eq!(results.len(), 1);
        let (kind, text, code) = shape(&results[0]);

        assert_eq!(
            kind, expected.result_kind,
            "result kind of {:?}",
            expected.source
        );
        if expected.result_kind == "value" {
            assert_eq!(text, expected.text, "value of {:?}", expected.source);
        }
        if expected.result_kind == "error" {
            assert_eq!(code, expected.code, "error of {:?}", expected.source);
        }
    }
}

#[test]
fn whole_notes_evaluate_top_to_bottom_exactly_as_the_graphical_editor_does() {
    // Contiguity, declaration order and what an aggregator sees are properties
    // of the *note*, not of a line, so they need their own scenarios.
    for note in fixture().get("notes").expect("notes").array() {
        let sources: Vec<Option<String>> = note
            .get("sources")
            .expect("sources")
            .array()
            .iter()
            .map(|value| value.string().map(str::to_owned))
            .collect();
        let borrowed: Vec<Option<&str>> = sources.iter().map(|line| line.as_deref()).collect();

        let results = evaluate_note(&borrowed);
        let expected = note.get("results").expect("results").array();
        assert_eq!(results.len(), expected.len(), "line count for {sources:?}");

        for (line, (actual, wanted)) in results.iter().zip(expected).enumerate() {
            let (kind, text, code) = shape(actual);
            let wanted_kind = wanted.get("kind").unwrap().string().unwrap();
            assert_eq!(kind, wanted_kind, "line {line} of {sources:?}");
            if wanted_kind == "value" {
                assert_eq!(
                    text.as_deref(),
                    wanted.get("text").and_then(|v| v.string()),
                    "line {line} of {sources:?}"
                );
            }
            if wanted_kind == "error" {
                assert_eq!(
                    code.as_deref(),
                    wanted.get("code").and_then(|v| v.string()),
                    "line {line} of {sources:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Security, asserted rather than assumed
// ---------------------------------------------------------------------------

#[test]
fn nothing_that_looks_like_a_runtime_reaches_anything() {
    // These are not filtered — they are unspellable. `constructor` is a name
    // with no value, and `window.location` stops at the dot, which is not a
    // token this grammar has.
    for source in [
        "= constructor",
        "= __proto__",
        "= window",
        "= globalThis",
        "= process",
        "= window.location",
        "= constructor.constructor(1)",
        "= fetch('http://example.com')",
        "= require('fs')",
        "= [].map",
        "= `x`",
        "= 1; rm -rf /",
        "= $(whoami)",
    ] {
        let results = evaluate_note(&[Some(source)]);
        assert!(
            matches!(results[0], LineResult::Error(_)),
            "{source:?} must be an error, got {:?}",
            results[0]
        );
    }
}

#[test]
fn an_expression_has_a_ceiling_on_its_size() {
    use noteit_tui::math::lexer::{MAX_EXPRESSION_LENGTH, MAX_TOKENS};

    let long = format!("= {}", "1+".repeat(MAX_EXPRESSION_LENGTH));
    assert!(matches!(
        evaluate_note(&[Some(&long)])[0],
        LineResult::Error(_)
    ));

    let many = format!("= {}1", "(".repeat(MAX_TOKENS / 2));
    assert!(matches!(
        evaluate_note(&[Some(&many)])[0],
        LineResult::Error(_)
    ));
}

#[test]
fn a_result_is_always_finite_or_an_error() {
    for source in [
        "= 1/0",
        "= 0/0",
        "= 99999999999999999999 * 99999999999999999999",
        "= -1/0",
    ] {
        match &evaluate_note(&[Some(source)])[0] {
            LineResult::Error(_) => {}
            LineResult::Value { value, .. } => {
                assert!(value.is_finite(), "{source:?} produced {value}")
            }
            LineResult::None => panic!("{source:?} should be a calculation"),
        }
    }
}

#[test]
fn an_error_message_never_echoes_the_note() {
    use noteit_tui::math::message_for;
    let hostile = "= <script>alert('xss')</script>";
    let LineResult::Error(error) = &evaluate_note(&[Some(hostile)])[0] else {
        panic!("should be an error");
    };
    let message = message_for(*error);
    assert!(!message.contains("script"), "the message echoed the note");
    assert_eq!(message, "expressão inválida");
}

#[test]
fn an_opaque_line_produces_nothing_and_breaks_the_block() {
    // `None` is how a caller says "this line exists but can never be a
    // calculation" — a fenced block, an inline code span, a heading.
    let results = evaluate_note(&[Some("= 1"), None, Some("= 2"), Some("= sum")]);
    assert!(matches!(results[1], LineResult::None));
    let LineResult::Value { value, .. } = &results[3] else {
        panic!("the sum should have a value");
    };
    assert_eq!(*value, 2.0, "the opaque line ended the block above it");
}
