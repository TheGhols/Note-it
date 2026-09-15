//! The ADR-065 corpus, run against the Core's parser.
//!
//! `docs/link-syntax-corpus.json` is the contract, and it is read from `docs/`
//! rather than copied next to this file. A fixture that has been duplicated is
//! a fixture that can drift, and the one thing this harness exists to prevent
//! is the parser and the specification disagreeing while both look green.
//!
//! Every case is asserted twice. Once for meaning — kind, note, section, block
//! and display, in source order — and once for losslessness, because ADR-065
//! makes `raw`, the raw components and `[start_byte, end_byte)` obligatory for
//! **every** reference of **every** case, not only the four the JSON spells out
//! in `lossless_examples`. Those four are asserted byte for byte on top, as the
//! fixed point the materialised assertions are calibrated against.

use noteit_core::link::{parse_references, NoteReference, ReferenceKind};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root is the crate's parent")
        .join("docs")
        .join("link-syntax-corpus.json")
}

fn corpus() -> Value {
    let raw =
        std::fs::read_to_string(corpus_path()).expect("the corpus travels with the repository");
    serde_json::from_str(&raw).expect("the corpus is JSON")
}

// ------------------------------------------------------------- expectations

#[derive(Debug, Clone, PartialEq, Eq)]
struct Expected {
    kind: ReferenceKind,
    note: Option<String>,
    section: Option<String>,
    block: Option<String>,
    display: Option<String>,
}

/// A component the JSON either spells out or describes as a repetition.
///
/// `{"text": "a", "count": 512}` keeps a 512-character name out of the file
/// without making it a different name.
fn component(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        Value::Object(map) => {
            let unit = map
                .get("text")
                .or_else(|| map.get("decoded"))
                .and_then(Value::as_str)
                .expect("a described component names its unit");
            let count = map["count"].as_u64().expect("a described component counts") as usize;
            Some(unit.repeat(count))
        }
        other => panic!("a component is a string, an object or null, not {other}"),
    }
}

fn expected_of(value: &Value) -> Expected {
    Expected {
        kind: match value["kind"].as_str().expect("a kind") {
            "link" => ReferenceKind::Link,
            "embed" => ReferenceKind::Embed,
            other => panic!("unknown kind {other}"),
        },
        note: component(&value["note"]),
        section: component(&value["section"]),
        block: component(&value["block"]),
        display: component(&value["display"]),
    }
}

fn actual_of(reference: &NoteReference<'_>) -> Expected {
    Expected {
        kind: reference.kind,
        note: reference.note.as_deref().map(str::to_owned),
        section: reference.section.as_deref().map(str::to_owned),
        block: reference.block.as_deref().map(str::to_owned),
        display: reference.display.as_deref().map(str::to_owned),
    }
}

// ----------------------------------------------------------------- the cases

struct Case {
    id: String,
    source: String,
    expected: Option<Vec<Expected>>,
    expected_count: Option<usize>,
    expected_raw_interior: Option<usize>,
}

/// The source of a case the JSON describes rather than spells out.
///
/// A two-megabyte line and ten thousand links do not belong in a file meant to
/// be read, so the corpus states how to build them and the harness builds them.
fn generated(id: &str, generator: &Value, cases: &[Value]) -> String {
    if let Some(kind) = generator.get("kind").and_then(Value::as_str) {
        return match kind {
            "escaped_components" => {
                let part = |key: &str| {
                    let described = &generator[key];
                    let decoded = described["decoded"].as_str().expect("a decoded unit");
                    let count = described["count"].as_u64().expect("a count") as usize;
                    format!("\\{decoded}").repeat(count)
                };
                format!(
                    "[[{}{}{}{}{}]]",
                    part("note"),
                    generator["separator"].as_str().expect("a separator"),
                    part("section"),
                    generator["display_separator"]
                        .as_str()
                        .expect("a display separator"),
                    part("display"),
                )
            }
            "escaped_components_plus_raw" => {
                let base_id = generator["base_case"].as_str().expect("a base case");
                let base = cases
                    .iter()
                    .find(|case| case["id"] == *base_id)
                    .unwrap_or_else(|| panic!("{id} names a base case {base_id} that is missing"));
                let built = generated(base_id, &base["generator"], cases);
                let suffix = generator["suffix_before_close"].as_str().expect("a suffix");
                let interior = built
                    .strip_prefix("[[")
                    .and_then(|rest| rest.strip_suffix("]]"))
                    .expect("the base case is a candidate");
                format!("[[{interior}{suffix}]]")
            }
            other => panic!("unknown generator kind {other}"),
        };
    }

    if let Some(prefix) = generator.get("prefix").and_then(Value::as_str) {
        let text = generator["text"].as_str().expect("a unit");
        let count = generator["count"].as_u64().expect("a count") as usize;
        return format!("{prefix}{}", text.repeat(count));
    }

    let template = generator["template"].as_str().expect("a template");
    if let Some(repeat) = generator.get("repeat").and_then(Value::as_u64) {
        let separator = generator
            .get("separator")
            .and_then(Value::as_str)
            .unwrap_or("");
        return (0..repeat)
            .map(|index| template.replace("{index}", &index.to_string()))
            .collect::<Vec<_>>()
            .join(separator);
    }

    let text = generator["text"].as_str().expect("a unit");
    let count = generator["count"].as_u64().expect("a count") as usize;
    template.replace("{text}", &text.repeat(count))
}

fn cases() -> Vec<Case> {
    let corpus = corpus();
    let raw = corpus["cases"].as_array().expect("the cases").clone();
    raw.iter()
        .map(|case| {
            let id = case["id"].as_str().expect("an id").to_owned();
            let source = match case.get("source").and_then(Value::as_str) {
                Some(source) => source.to_owned(),
                None => generated(&id, &case["generator"], &raw),
            };
            Case {
                id,
                source,
                expected: case
                    .get("references")
                    .and_then(Value::as_array)
                    .map(|list| list.iter().map(expected_of).collect()),
                expected_count: case
                    .get("expected_reference_count")
                    .and_then(Value::as_u64)
                    .map(|count| count as usize),
                expected_raw_interior: case
                    .get("expected_raw_interior_scalars")
                    .and_then(Value::as_u64)
                    .map(|count| count as usize),
            }
        })
        .collect()
}

// ------------------------------------------------------------- the assertions

#[test]
fn the_corpus_is_the_one_the_adr_froze() {
    let corpus = corpus();
    assert_eq!(corpus["contract"], "ADR-065");
    assert_eq!(corpus["version"], 1);
    assert_eq!(
        corpus["cases"].as_array().expect("the cases").len(),
        110,
        "ADR-065 froze a corpus of 110 cases; a different count is a changed contract"
    );
}

/// No case may pass by being skipped.
///
/// A harness that silently ignores a case it cannot describe is worse than no
/// harness, because it reports green for work it never did. Every one of the
/// 110 has to arrive here carrying either a reference list or a count.
#[test]
fn every_case_is_actually_asserted() {
    let cases = cases();
    assert_eq!(cases.len(), 110);
    let mut spelled = 0;
    let mut counted = 0;
    for case in &cases {
        assert!(
            case.expected.is_some() || case.expected_count.is_some(),
            "{}: the harness has no expectation to check",
            case.id
        );
        spelled += usize::from(case.expected.is_some());
        counted += usize::from(case.expected_count.is_some());
    }
    assert_eq!(spelled, 105, "cases whose references are spelled out");
    assert_eq!(counted, 5, "cases the corpus describes by count alone");
}

#[test]
fn every_normative_case_parses_as_the_corpus_says() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in cases() {
        let references = parse_references(&case.source);
        let actual: Vec<Expected> = references.iter().map(actual_of).collect();

        if let Some(expected) = &case.expected {
            checked += 1;
            if &actual != expected {
                failures.push(format!(
                    "{}: expected {:?}\n              got      {:?}",
                    case.id, expected, actual
                ));
            }
        }
        if let Some(count) = case.expected_count {
            checked += 1;
            if actual.len() != count {
                failures.push(format!(
                    "{}: expected {count} references, got {}",
                    case.id,
                    actual.len()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of the normative cases disagree with the parser:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(checked, 110, "every case contributes exactly one assertion");
}

/// The obligation ADR-065 puts on every reference of every case: real UTF-8
/// offsets, a `raw` that is exactly the slice they name, and raw components
/// that are the interior's own pieces rather than copies of the decoded text.
#[test]
fn every_reference_of_every_case_carries_its_source_exactly() {
    for case in cases() {
        let source = &case.source;
        let references = parse_references(source);
        let mut previous_end = 0;

        for reference in &references {
            let id = &case.id;
            assert!(
                reference.start_byte >= previous_end,
                "{id}: references overlap or run out of source order"
            );
            assert!(
                reference.end_byte <= source.len(),
                "{id}: a reference ends past the source"
            );
            assert!(
                source.is_char_boundary(reference.start_byte)
                    && source.is_char_boundary(reference.end_byte),
                "{id}: an offset is not on a UTF-8 boundary"
            );
            assert_eq!(
                reference.raw,
                &source[reference.start_byte..reference.end_byte],
                "{id}: raw is not the slice the offsets name"
            );

            // The raw components, laid back out with the punctuation the
            // grammar puts between them, must rebuild the candidate exactly.
            let opener = match reference.kind {
                ReferenceKind::Link => "[[",
                ReferenceKind::Embed => "![[",
            };
            let mut rebuilt = String::from(opener);
            rebuilt.push_str(reference.note_raw.unwrap_or(""));
            if let Some(section) = reference.section_raw {
                rebuilt.push('#');
                rebuilt.push_str(section);
            }
            if let Some(block) = reference.block_raw {
                rebuilt.push('^');
                rebuilt.push_str(block);
            }
            if let Some(display) = reference.display_raw {
                rebuilt.push('|');
                rebuilt.push_str(display);
            }
            rebuilt.push_str("]]");
            assert_eq!(
                rebuilt, reference.raw,
                "{id}: the raw components do not rebuild the candidate"
            );

            previous_end = reference.end_byte;
        }
    }
}

/// The central guarantee: the text between references, laid alongside the
/// references' own `raw`, is the input again — byte for byte.
#[test]
fn lossless_reconstruction_holds_for_the_corpus() {
    for case in cases() {
        let source = &case.source;
        let references = parse_references(source);
        let mut rebuilt = String::with_capacity(source.len());
        let mut cursor = 0;
        for reference in &references {
            rebuilt.push_str(&source[cursor..reference.start_byte]);
            rebuilt.push_str(reference.raw);
            cursor = reference.end_byte;
        }
        rebuilt.push_str(&source[cursor..]);
        assert_eq!(
            rebuilt.as_bytes(),
            source.as_bytes(),
            "{}: parsing lost or changed a byte",
            case.id
        );
    }
}

#[test]
fn the_lossless_examples_are_reproduced_field_by_field() {
    let corpus = corpus();
    let examples = corpus["lossless_examples"]
        .as_array()
        .expect("the lossless examples");
    assert_eq!(examples.len(), 4);

    for example in examples {
        let source = example["source"].as_str().expect("a source");
        let references = parse_references(source);
        let expected = example["references"].as_array().expect("references");
        assert_eq!(
            references.len(),
            expected.len(),
            "{source:?}: wrong number of references"
        );

        for (reference, wanted) in references.iter().zip(expected) {
            assert_eq!(
                reference.start_byte,
                wanted["start_byte"].as_u64().expect("a start") as usize,
                "{source:?}: start_byte"
            );
            assert_eq!(
                reference.end_byte,
                wanted["end_byte"].as_u64().expect("an end") as usize,
                "{source:?}: end_byte"
            );
            assert_eq!(reference.raw, wanted["raw"].as_str().expect("a raw"));
            assert_eq!(
                reference.kind,
                match wanted["kind"].as_str().expect("a kind") {
                    "link" => ReferenceKind::Link,
                    _ => ReferenceKind::Embed,
                }
            );
            for (name, actual) in [
                ("note_raw", reference.note_raw),
                ("section_raw", reference.section_raw),
                ("block_raw", reference.block_raw),
                ("display_raw", reference.display_raw),
            ] {
                assert_eq!(actual, wanted[name].as_str(), "{source:?}: {name}");
            }
            for (name, actual) in [
                ("note", reference.note.as_deref()),
                ("section", reference.section.as_deref()),
                ("block", reference.block.as_deref()),
                ("display", reference.display.as_deref()),
            ] {
                assert_eq!(actual, wanted[name].as_str(), "{source:?}: {name}");
            }
        }
    }
}

/// The two boundary cases the ADR's third review round added, checked for the
/// reason they exist rather than only for their outcome.
#[test]
fn the_raw_interior_ceiling_is_where_the_corpus_puts_it() {
    for case in cases() {
        let Some(expected) = case.expected_raw_interior else {
            continue;
        };
        let interior = case
            .source
            .strip_prefix("[[")
            .and_then(|rest| rest.strip_suffix("]]"))
            .expect("a boundary case is one candidate");
        assert_eq!(
            interior.chars().count(),
            expected,
            "{}: the harness built the wrong interior",
            case.id
        );
    }
}
