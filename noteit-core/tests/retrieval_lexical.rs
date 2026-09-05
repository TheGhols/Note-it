//! BM25 inside the Context Engine, against a real store.
//!
//! `lexical.rs` proves the arithmetic; this file proves what the arithmetic is
//! allowed to do. The two questions are different, and the second is the one
//! with the contract in it: a score may decide the order of the candidates BM25
//! itself brought, and it may never move one that was already there.
//!
//! That protection is structural rather than numerical. A term candidate lives
//! in class 2 and a phrase candidate in class 1, so no score, however large,
//! reaches past one — there is no scale on which it could. The tests below
//! build the largest score the corpus allows and watch it stay put.

use noteit_core::chrono::{DateTime, TimeZone, Utc};
use noteit_core::context::{retrieve, Candidate, ContextRequest, Reason, MAX_CANDIDATES};
use noteit_core::filter::NoteFilter;
use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use noteit_core::{NoteItCore, StorageManager, StorePaths, Uuid};
use std::path::Path;
use tempfile::{tempdir, TempDir};

// ------------------------------------------------------------------ harness

struct Store {
    _tmp: TempDir,
    core: NoteItCore,
}

impl Store {
    fn new() -> Self {
        let tmp = tempdir().expect("tempdir");
        let core = open(tmp.path());
        core.storage().ensure_directories().expect("ensure dirs");
        Self { _tmp: tmp, core }
    }

    fn note(&self, id: Uuid, body: &str) -> Uuid {
        self.write(id, body, &[], &[], at(0))
    }

    fn write(
        &self,
        id: Uuid,
        body: &str,
        tags: &[&str],
        properties: &[(&str, &str)],
        updated_at: Option<DateTime<Utc>>,
    ) -> Uuid {
        let mut document = NoteDocument::new_empty();
        document.metadata.id = id;
        document.metadata.created_at = updated_at;
        document.metadata.updated_at = updated_at;
        document.content = body.to_string();
        document.user_metadata = NoteMetadata::try_new(
            tags.iter()
                .map(|tag| (*tag).to_string())
                .collect::<Vec<_>>(),
            properties
                .iter()
                .map(|(key, value)| NoteProperty {
                    key: (*key).to_string(),
                    value: (*value).to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .expect("metadata");
        self.core
            .storage()
            .save_note_atomic(&document)
            .expect("save");
        id
    }
}

fn open(root: &Path) -> NoteItCore {
    let paths = StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        root.join("runtime/note-it"),
    );
    NoteItCore::from_storage(StorageManager::from_paths(paths).expect("open storage"))
}

/// Every note shares one instant unless a test says otherwise, so recency never
/// silently decides what the score was supposed to.
fn at(minutes: i64) -> Option<DateTime<Utc>> {
    Some(
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("a real instant")
            + noteit_core::chrono::Duration::minutes(minutes),
    )
}

fn id(nth: u16) -> Uuid {
    Uuid::from_u128(0x4c65_7869_6361_6c00_0000_0000_0000_0000u128 | nth as u128)
}

fn ask(store: &Store, query: &str) -> Vec<Candidate> {
    let request = ContextRequest {
        limit: Some(MAX_CANDIDATES),
        ..ContextRequest::with_query(query)
    };
    retrieve(&store.core, &request)
        .expect("the request must be answered")
        .candidates
}

fn ids(candidates: &[Candidate]) -> Vec<Uuid> {
    candidates
        .iter()
        .map(|candidate| candidate.note_id)
        .collect()
}

// ---------------------------------------------------------- what admits

#[test]
fn a_term_admits_a_note_the_phrase_never_could() {
    let store = Store::new();
    let scattered = store.note(id(1), "arterial e depois, muito depois, hipertensão");

    let candidates = ask(&store, "hipertensão arterial");

    assert_eq!(ids(&candidates), vec![scattered]);
    assert_eq!(
        candidates[0].reasons,
        vec![Reason::TermMatch],
        "the words are there and the phrase is not"
    );
}

#[test]
fn the_phrase_is_still_its_own_signal() {
    let store = Store::new();
    let phrase = store.note(id(1), "hipertensão arterial sistêmica");
    let scattered = store.note(id(2), "arterial e, muito depois, hipertensão");

    let candidates = ask(&store, "hipertensão arterial");

    assert_eq!(
        ids(&candidates),
        vec![phrase, scattered],
        "the phrase is class 1 and the loose words are class 2"
    );
    assert!(candidates[0].reasons.contains(&Reason::TextMatch));
    assert_eq!(candidates[1].reasons, vec![Reason::TermMatch]);
}

#[test]
fn a_note_with_none_of_the_words_is_not_a_candidate() {
    let store = Store::new();
    store.note(id(1), "insônia e plantão");
    assert!(ask(&store, "metformina").is_empty());
}

// --------------------------------------------- the exact hit is protected

/// The structural guarantee, built to be attacked.
#[test]
fn a_score_of_any_size_never_crosses_a_phrase_match() {
    let store = Store::new();
    // One mention, in a long note, of a term that half the store also has: the
    // smallest BM25 score the corpus can produce.
    let exact = store.note(id(1), &format!("hipertensão {}", "palavra ".repeat(400)));
    // And the largest: a note that is nothing but the rarest term, repeated.
    let huge = store.note(id(2), &"hipertensao ".repeat(60));
    for nth in 3..=20u16 {
        store.note(id(nth), &format!("hipertensao mencionada na nota {nth}"));
    }

    // `hipertensão` folds to `hipertensao`, so the phrase occurs in `exact` and
    // the term occurs everywhere.
    let candidates = ask(&store, "hipertensão");

    assert_eq!(candidates[0].note_id, exact, "the phrase match must lead");
    assert!(candidates[0].reasons.contains(&Reason::TextMatch));
    assert!(
        ids(&candidates).contains(&huge),
        "and the loud one is still an answer, just not the first"
    );
    assert!(
        candidates
            .iter()
            .position(|c| c.note_id == huge)
            .expect("present")
            > 0
    );
}

#[test]
fn a_tag_with_no_query_words_still_outranks_the_best_term_match() {
    let store = Store::new();
    // The identifiers are the wrong way round on purpose: if the classes were
    // not doing the work, the final tie-break would put `scoring` first.
    let scoring = store.note(id(1), &"arritmia ".repeat(50));
    let tagged = store.write(id(2), "nada em comum", &["cardio"], &[], at(0));

    // Two words, so the phrase never occurs and `scoring` is a pure class-2
    // candidate with the best score the store can produce.
    let request = ContextRequest {
        query: "arritmia grave".to_string(),
        filter: NoteFilter::new(vec!["cardio".to_string()], Vec::new()),
        limit: Some(MAX_CANDIDATES),
        ..ContextRequest::default()
    };
    let candidates = retrieve(&store.core, &request).expect("answers").candidates;

    assert_eq!(
        ids(&candidates),
        vec![tagged, scoring],
        "class 1 is the admission set the engine already had; BM25 sits below it"
    );
    assert_eq!(candidates[0].reasons, vec![Reason::SharedTag]);
    assert_eq!(candidates[1].reasons, vec![Reason::TermMatch]);
}

#[test]
fn a_term_match_never_reorders_the_candidates_that_already_existed() {
    let store = Store::new();
    // Two class-1 candidates. `two` has more declared signals, so it leads —
    // and it must go on leading even though `one` scores better on BM25, which
    // it does: it says the word many times in a short note.
    // Again the identifiers are reversed, so `declared` is the only thing that
    // can produce the expected order.
    let one = store.note(id(1), &"arritmia ".repeat(40));
    let two = store.write(id(2), "arritmia uma vez", &["cardio"], &[], at(0));

    let request = ContextRequest {
        query: "arritmia".to_string(),
        filter: NoteFilter::new(vec!["cardio".to_string()], Vec::new()),
        limit: Some(MAX_CANDIDATES),
        ..ContextRequest::default()
    };
    let candidates = retrieve(&store.core, &request).expect("answers").candidates;

    assert_eq!(ids(&candidates), vec![two, one]);
    assert!(candidates[0].reasons.contains(&Reason::SharedTag));
}

// ------------------------------------------------ how class 2 is ordered

#[test]
fn a_rare_word_beats_a_common_one() {
    let store = Store::new();
    let rare = store.note(id(1), "raro");
    let common = store.note(id(2), "comum");
    for nth in 3..=12u16 {
        store.note(id(nth), "comum");
    }

    let candidates = ask(&store, "comum raro");

    assert_eq!(
        candidates[0].note_id, rare,
        "one document in eleven outweighs ten in eleven"
    );
    assert!(ids(&candidates).contains(&common));
}

#[test]
fn a_short_note_beats_a_long_one_that_says_the_same_thing() {
    let store = Store::new();
    let brief = store.note(id(1), "arritmia");
    let sprawling = store.note(id(2), &format!("arritmia {}", "outra palavra ".repeat(200)));

    let candidates = ask(&store, "arritmia");

    assert_eq!(ids(&candidates), vec![brief, sprawling]);
}

#[test]
fn saying_it_twice_beats_saying_it_once() {
    let store = Store::new();
    let twice = store.note(id(1), "arritmia arritmia outra outra");
    let once = store.note(id(2), "arritmia outra outra outra");

    assert_eq!(ids(&ask(&store, "arritmia")), vec![twice, once]);
}

#[test]
fn two_notes_the_score_cannot_separate_are_separated_by_identifier() {
    let store = Store::new();
    let low = store.note(id(1), "arritmia e taquicardia");
    let middle = store.note(id(2), "arritmia e taquicardia");
    let high = store.note(id(3), "arritmia e taquicardia");
    assert!(low < middle && middle < high);

    for _ in 0..5 {
        assert_eq!(
            ids(&ask(&store, "arritmia bradicardia")),
            vec![low, middle, high]
        );
    }
}

#[test]
fn repeating_a_word_in_the_question_does_not_weigh_it_twice() {
    let store = Store::new();
    let sleep = store.note(id(1), "sono sono sono");
    let shift = store.note(id(2), "turno turno turno turno");

    // If the query's own repetition counted, `sono` would gain a weight it did
    // not earn and the order would flip.
    let once = ids(&ask(&store, "sono turno"));
    let twice = ids(&ask(&store, "sono sono turno"));
    assert_eq!(once, twice);
    assert!(once.contains(&sleep) && once.contains(&shift));
}

// ------------------------------------------------- what the reader sees

#[test]
fn the_evidence_is_the_first_word_of_the_question_that_occurs() {
    let store = Store::new();
    // The note mentions the second query word first. The evidence still follows
    // the question's order, not the note's.
    let note = store.note(id(1), "Beta aparece antes, e depois vem Alfa.");

    let candidates = ask(&store, "alfa beta");

    assert_eq!(ids(&candidates), vec![note]);
    assert_eq!(
        candidates[0].matched_text.as_deref(),
        Some("Alfa"),
        "and it is spelled the way the note spells it"
    );
    assert!(candidates[0].snippet.contains("Alfa"));
}

#[test]
fn the_evidence_survives_folding_and_keeps_the_notes_accents() {
    let store = Store::new();
    let note = store.note(id(1), "Uma nota sobre CORAÇÃO e nada mais");

    let candidates = ask(&store, "coracao pulmao");

    assert_eq!(ids(&candidates), vec![note]);
    assert_eq!(candidates[0].matched_text.as_deref(), Some("CORAÇÃO"));
}

#[test]
fn a_phrase_match_keeps_its_own_evidence() {
    let store = Store::new();
    store.note(id(1), "Hipertensão Arterial e outras coisas");

    let candidates = ask(&store, "hipertensão arterial");

    assert!(candidates[0].reasons.contains(&Reason::TextMatch));
    assert_eq!(
        candidates[0].matched_text.as_deref(),
        Some("Hipertensão Arterial"),
        "the phrase, not the first term of it"
    );
}

// --------------------------------------- what does not produce a term

#[test]
fn a_request_with_no_query_never_produces_a_term_match() {
    let store = Store::new();
    store.write(
        id(1),
        "cardio e arritmia",
        &["cardio"],
        &[("fonte", "aula")],
        at(0),
    );

    for request in [
        ContextRequest::empty(),
        ContextRequest {
            filter: NoteFilter::new(vec!["cardio".to_string()], Vec::new()),
            ..ContextRequest::default()
        },
        ContextRequest {
            filter: NoteFilter::new(Vec::new(), vec![("fonte".to_string(), "aula".to_string())]),
            ..ContextRequest::default()
        },
    ] {
        let candidates = retrieve(&store.core, &request).expect("answers").candidates;
        for candidate in &candidates {
            assert!(
                !candidate.reasons.contains(&Reason::TermMatch),
                "there is no term to match: {:?}",
                candidate.reasons
            );
        }
    }
}

#[test]
fn a_question_with_no_words_in_it_finds_nothing() {
    let store = Store::new();
    store.note(id(1), "hipertensão arterial e 2025");

    for question in ["!!! ???", "…", "🙂🙂🙂", "心血管", "\u{0301}\u{0302}"] {
        assert!(
            ask(&store, question).is_empty(),
            "{question:?} produced candidates"
        );
    }
}

#[test]
fn digits_are_words_too() {
    let store = Store::new();
    let note = store.note(id(1), "protocolo de sepse 2025");
    assert_eq!(ids(&ask(&store, "sepse 2025")), vec![note]);
    assert_eq!(ids(&ask(&store, "2025")), vec![note]);
}

#[test]
fn a_hyphen_separates_two_words() {
    let store = Store::new();
    let note = store.note(id(1), "a fórmula CKD-EPI estima a filtração");
    // The phrase matches too, but the point is that the halves are terms.
    assert_eq!(ids(&ask(&store, "ckd epi")), vec![note]);
}

// -------------------------------------------------- the corpus is the store

#[test]
fn a_note_in_the_trash_is_not_part_of_the_corpus_and_not_a_candidate() {
    let store = Store::new();
    let live = store.note(id(1), "arritmia viva");
    let deleted = store.note(id(2), "arritmia apagada");
    store
        .core
        .storage()
        .move_note_to_trash(&deleted)
        .expect("trash");

    assert_eq!(ids(&ask(&store, "arritmia")), vec![live]);
}

#[test]
fn a_note_that_cannot_be_read_is_a_warning_and_never_a_document() {
    let store = Store::new();
    let good = store.note(id(1), "arritmia legível");
    let notes = store.core.storage().paths().notes_dir.clone();
    std::fs::write(notes.join(format!("{}.md", id(2))), b"\xff\xfe not utf8")
        .expect("write a broken note");

    let answer = retrieve(
        &store.core,
        &ContextRequest {
            limit: Some(MAX_CANDIDATES),
            ..ContextRequest::with_query("arritmia")
        },
    )
    .expect("answers");

    assert_eq!(ids(&answer.candidates), vec![good]);
    assert_eq!(answer.warnings.len(), 1);
}

// ------------------------------------------------------------ adversarial

#[test]
fn a_note_that_is_one_word_ten_thousand_times_is_still_a_bounded_candidate() {
    let store = Store::new();
    let repetitive = store.note(id(1), &"arritmia ".repeat(10_000));
    store.note(id(2), "arritmia uma vez");

    let candidates = ask(&store, "arritmia");

    assert!(ids(&candidates).contains(&repetitive));
    for candidate in &candidates {
        assert!(candidate.snippet.chars().count() <= 242);
        assert!(candidate
            .matched_text
            .as_ref()
            .is_none_or(|text| text.chars().count() <= 242));
    }
}

#[test]
fn a_question_of_five_hundred_distinct_words_is_answered() {
    let store = Store::new();
    let note = store.note(id(1), "alfa beta gama delta");
    let question: String = (0..120).map(|nth| format!("w{nth} ")).collect::<String>() + "alfa";
    assert!(question.chars().count() <= 512);

    assert_eq!(ids(&ask(&store, &question)), vec![note]);
}

#[test]
fn every_note_being_identical_is_answered_in_identifier_order() {
    let store = Store::new();
    let notes: Vec<Uuid> = (1..=30u16)
        .map(|nth| store.note(id(nth), "todas as notas dizem exatamente isto"))
        .collect();

    let candidates = ask(&store, "exatamente");

    assert_eq!(ids(&candidates), notes);
}

#[test]
fn a_store_of_empty_notes_produces_no_channel_and_no_panic() {
    let store = Store::new();
    for nth in 1..=5u16 {
        store.note(id(nth), "");
    }
    assert!(ask(&store, "qualquer").is_empty());
}

#[test]
fn hostile_unicode_is_a_candidate_like_any_other() {
    let store = Store::new();
    let note = store.note(
        id(1),
        "arritmia \u{202e}txet desrever\u{202c} e \u{0000}nulo e 🙂 emoji",
    );

    let candidates = ask(&store, "arritmia");

    assert_eq!(ids(&candidates), vec![note]);
    assert!(std::str::from_utf8(candidates[0].snippet.as_bytes()).is_ok());
}

#[test]
fn the_same_question_answers_the_same_way_every_time() {
    let store = Store::new();
    for nth in 1..=25u16 {
        store.write(
            id(nth),
            &format!("nota {nth} sobre arritmia e taquicardia e outras coisas"),
            if nth % 3 == 0 { &["cardio"] } else { &[] },
            &[],
            at(i64::from(nth % 5)),
        );
    }

    let first = ask(&store, "arritmia bradicardia");
    for _ in 0..8 {
        assert_eq!(first, ask(&store, "arritmia bradicardia"));
    }
}
