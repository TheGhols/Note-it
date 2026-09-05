//! The ranking contract as the engine had it *before* BM25, frozen.
//!
//! Phase 4.3A.R1.2 derived the admission and precedence policy from the
//! measured behaviour of `noteit-core/src/context.rs` rather than from what
//! would be convenient to code, and froze ten scenarios for 4.3B to run
//! **before** writing a line of BM25 — so that the change is measured against
//! them and not against anybody's memory of what the engine used to do
//! (`docs/semantic-retrieval.md` §27, "A matriz que a 4.3B congela ANTES de
//! escrever o BM25").
//!
//! Every assertion here is written to hold on both sides of that change. Where
//! the frozen fact is "this reason is present", it is asserted with
//! `contains`; where the frozen fact is "these are the only reasons" — which is
//! true exactly when the request carries no query, because a request with no
//! query can produce neither a term nor a semantic signal — it is asserted with
//! equality. What is never relaxed is **position**: the order of the identifiers
//! coming out of each scenario is the thing 4.3B may not disturb.
//!
//! Nothing in this file can reach the store on this machine: every scenario
//! builds a throwaway store in a temporary directory, with identifiers and
//! timestamps it chooses itself, so a tie is decided by data rather than by
//! whatever order the filesystem happened to hand back.

use noteit_core::chrono::{DateTime, TimeZone, Utc};
use noteit_core::context::{retrieve, Candidate, ContextRequest, Reason};
use noteit_core::filter::NoteFilter;
use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use noteit_core::{NoteItCore, StorageManager, StorePaths, Uuid};
use std::path::Path;
use tempfile::{tempdir, TempDir};

// ------------------------------------------------------------------ harness

/// A store whose identifiers and timestamps the test chooses.
///
/// `Store::note` in `context_engine.rs` lets the document invent both, which is
/// right for a scenario about content. It is wrong for a scenario about
/// *order*: a random identifier makes the final tie-break unreproducible, and
/// `Utc::now()` on every note makes recency a race. Both are arguments here.
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

    /// Writes one note, exactly as asked.
    fn put(
        &self,
        id: Uuid,
        body: &str,
        tags: &[&str],
        properties: &[(&str, &str)],
        updated_at: Option<DateTime<Utc>>,
    ) -> Uuid {
        let mut document = NoteDocument::new_empty();
        document.metadata.id = id;
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

/// An identifier a test can name twice and a reader can order by eye.
///
/// `Uuid::from_u128` and not `new_v4`: the last rule of the published order is
/// `note_id`, so a scenario about ties has to be able to say which identifier
/// is smaller.
fn id(nth: u8) -> Uuid {
    Uuid::from_u128(0x4e6f_7465_4974_0000_0000_0000_0000_0000u128 | nth as u128)
}

/// A fixed instant, `minutes` along. Later means more recent.
fn at(minutes: i64) -> Option<DateTime<Utc>> {
    Some(
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("a real instant")
            + noteit_core::chrono::Duration::minutes(minutes),
    )
}

fn ask(store: &Store, request: &ContextRequest) -> Vec<Candidate> {
    let answer = retrieve(&store.core, request).expect("the request must be answered");
    for candidate in &answer.candidates {
        assert_reasons_are_in_published_order(candidate);
    }
    answer.candidates
}

fn ids(candidates: &[Candidate]) -> Vec<Uuid> {
    candidates
        .iter()
        .map(|candidate| candidate.note_id)
        .collect()
}

fn only(candidates: &[Candidate], note: Uuid) -> Candidate {
    let found: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| candidate.note_id == note)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "a note appears once, however many channels admitted it"
    );
    found[0].clone()
}

/// The reasons of one candidate are strictly increasing in [`Reason`]'s
/// declared order, and carry no repeats.
///
/// The order is a contract and not an accident of how the signal functions
/// happen to run, so it is checked on every candidate every scenario produces
/// rather than in one test of its own.
fn assert_reasons_are_in_published_order(candidate: &Candidate) {
    assert!(
        candidate.reasons.windows(2).all(|pair| pair[0] < pair[1]),
        "reasons must be published in the declared order, without repeats: {:?}",
        candidate.reasons
    );
}

fn with_tags(query: &str, tags: &[&str]) -> ContextRequest {
    ContextRequest {
        query: query.to_string(),
        filter: NoteFilter::new(
            tags.iter().map(|tag| (*tag).to_string()).collect(),
            Vec::new(),
        ),
        ..ContextRequest::default()
    }
}

fn with_properties(query: &str, properties: &[(&str, &str)]) -> ContextRequest {
    ContextRequest {
        query: query.to_string(),
        filter: NoteFilter::new(
            Vec::new(),
            properties
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        ),
        ..ContextRequest::default()
    }
}

// ------------------------------------------------- 1. exact phrase matching

#[test]
fn scenario_1_an_exact_phrase_is_admitted_and_says_so() {
    let store = Store::new();
    let note = store.put(
        id(1),
        "Apneia obstrutiva do sono\n\nRonco alto e pausas respiratórias durante a noite.",
        &[],
        &[],
        at(0),
    );
    store.put(id(2), "Anemia ferropriva e ferritina.", &[], &[], at(1));

    let candidates = ask(
        &store,
        &ContextRequest::with_query("apneia obstrutiva do sono"),
    );

    assert_eq!(ids(&candidates), vec![note]);
    assert!(candidates[0].reasons.contains(&Reason::TextMatch));
    assert_eq!(
        candidates[0].matched_text.as_deref(),
        Some("Apneia obstrutiva do sono"),
        "the occurrence travels as the note spells it"
    );
}

// ------------------------------------------------------- 2. tag, no query

#[test]
fn scenario_2_a_tag_without_a_query_admits_and_is_the_only_reason() {
    let store = Store::new();
    let tagged = store.put(id(1), "Hipertensão resistente.", &["cardio"], &[], at(0));
    store.put(id(2), "Insônia do plantonista.", &["sono"], &[], at(1));

    let candidates = ask(&store, &with_tags("", &["cardio"]));

    assert_eq!(ids(&candidates), vec![tagged]);
    assert_eq!(
        candidates[0].reasons,
        vec![Reason::SharedTag],
        "no query means no term and no embedding: the tag is the whole answer"
    );
    assert!(candidates[0].matched_text.is_none());
}

// -------------------------------------------------- 3. property, no query

#[test]
fn scenario_3_a_property_without_a_query_admits_and_is_the_only_reason() {
    let store = Store::new();
    let sourced = store.put(
        id(1),
        "Diretriz de hipertensão.",
        &[],
        &[("fonte", "diretriz")],
        at(0),
    );
    store.put(id(2), "Anotação de aula.", &[], &[("fonte", "aula")], at(1));

    let candidates = ask(&store, &with_properties("", &[("fonte", "diretriz")]));

    assert_eq!(ids(&candidates), vec![sourced]);
    assert_eq!(candidates[0].reasons, vec![Reason::PropertyMatch]);
}

// -------------------------------------------------------- 4. query + tag

#[test]
fn scenario_4_a_query_and_a_tag_order_by_how_many_declared_signals_matched() {
    let store = Store::new();
    let both = store.put(
        id(1),
        "Hipertensão arterial no idoso.",
        &["cardio"],
        &[],
        at(0),
    );
    let text_only = store.put(
        id(2),
        "Hipertensão arterial resistente ao tratamento.",
        &[],
        &[],
        // More recent than `both`, so that recency cannot be what put them in
        // this order: the reason count has to be doing the work.
        at(10),
    );

    let candidates = ask(&store, &with_tags("hipertensão", &["cardio"]));

    assert_eq!(
        ids(&candidates),
        vec![both, text_only],
        "two declared signals outrank one, and recency only breaks a tie"
    );
    let first = only(&candidates, both);
    assert!(first.reasons.contains(&Reason::TextMatch));
    assert!(first.reasons.contains(&Reason::SharedTag));
    assert!(only(&candidates, text_only)
        .reasons
        .contains(&Reason::TextMatch));
}

// --------------------------------------------------- 5. query + property

#[test]
fn scenario_5_a_query_and_a_property_order_the_same_way() {
    let store = Store::new();
    let both = store.put(
        id(1),
        "Hipertensão arterial no idoso.",
        &[],
        &[("fonte", "diretriz")],
        at(0),
    );
    let text_only = store.put(
        id(2),
        "Hipertensão arterial resistente ao tratamento.",
        &[],
        &[],
        at(10),
    );

    let candidates = ask(
        &store,
        &with_properties("hipertensão", &[("fonte", "diretriz")]),
    );

    assert_eq!(ids(&candidates), vec![both, text_only]);
    assert!(only(&candidates, both)
        .reasons
        .contains(&Reason::PropertyMatch));
}

// ------------------------------------------------ 6/7. tasks and the flag

const TASK_NOTE: &str = "Revisão de cardiologia\n\n- [ ] reler hipertensão arterial\n- [ ] reler hipertensão pulmonar\n- [ ] estudar arritmia";

#[test]
fn scenario_6_a_matching_task_is_a_reason_even_when_tasks_were_not_asked_for() {
    let store = Store::new();
    let note = store.put(id(1), TASK_NOTE, &[], &[], at(0));

    let candidates = ask(&store, &ContextRequest::with_query("hipertensão"));

    let candidate = only(&candidates, note);
    assert!(
        candidate.reasons.contains(&Reason::TaskMatch),
        "`include_tasks` decides whether the tasks travel, never whether they count"
    );
    assert!(candidate.tasks.is_empty());
    assert!(!candidate.tasks_truncated);
    assert_eq!(candidate.omitted_task_count, 0);
}

#[test]
fn scenario_7_asking_for_tasks_changes_only_what_travels() {
    let store = Store::new();
    let note = store.put(id(1), TASK_NOTE, &[], &[], at(0));

    let without = only(
        &ask(&store, &ContextRequest::with_query("hipertensão")),
        note,
    );
    let with = only(
        &ask(
            &store,
            &ContextRequest {
                include_tasks: true,
                ..ContextRequest::with_query("hipertensão")
            },
        ),
        note,
    );

    assert_eq!(
        without.reasons, with.reasons,
        "the flag does not add or remove a reason"
    );
    assert_eq!(
        with.tasks.len(),
        2,
        "both matching tasks, in the note's order"
    );
    assert_eq!(with.tasks[0].text, "reler hipertensão arterial");
    assert_eq!(with.tasks[1].text, "reler hipertensão pulmonar");
    assert!(!with.tasks_truncated);
}

// ------------------------------------------------------- 8. empty request

#[test]
fn scenario_8_an_empty_request_is_recency_and_labels_itself_as_such() {
    let store = Store::new();
    let oldest = store.put(id(1), "Primeira nota.", &["cardio"], &[], at(0));
    let middle = store.put(id(2), "Segunda nota.", &[], &[("fonte", "aula")], at(5));
    let newest = store.put(id(3), "Terceira nota.", &[], &[], at(10));

    let candidates = ask(&store, &ContextRequest::empty());

    assert_eq!(ids(&candidates), vec![newest, middle, oldest]);
    for candidate in &candidates {
        assert_eq!(
            candidate.reasons,
            vec![Reason::Recent],
            "recency is never mixed with a factual signal"
        );
        assert!(candidate.matched_text.is_none());
    }
}

// ------------------------------------------------- 9. reasons accumulate

#[test]
fn scenario_9_every_applicable_signal_lands_on_one_candidate() {
    let store = Store::new();
    let note = store.put(
        id(1),
        "Hipertensão arterial\n\n- [ ] reler hipertensão no idoso",
        &["cardio"],
        &[("fonte", "diretriz")],
        at(0),
    );

    let request = ContextRequest {
        query: "hipertensão".to_string(),
        filter: NoteFilter::new(
            vec!["cardio".to_string()],
            vec![("fonte".to_string(), "diretriz".to_string())],
        ),
        include_tasks: true,
        limit: None,
    };
    let candidates = ask(&store, &request);

    let candidate = only(&candidates, note);
    for reason in [
        Reason::TextMatch,
        Reason::SharedTag,
        Reason::PropertyMatch,
        Reason::TaskMatch,
    ] {
        assert!(
            candidate.reasons.contains(&reason),
            "{reason:?} must be among the reasons"
        );
    }
    assert!(!candidate.reasons.contains(&Reason::Recent));
}

// ------------------------------------------------------ 10. the final tie

#[test]
fn scenario_10_a_full_tie_is_resolved_by_identifier_and_stays_resolved() {
    let store = Store::new();
    // Same reasons, same instant: only `note_id` is left, and it must decide.
    let low = store.put(id(1), "Hipertensão arterial.", &[], &[], at(0));
    let middle = store.put(id(2), "Hipertensão arterial.", &[], &[], at(0));
    let high = store.put(id(3), "Hipertensão arterial.", &[], &[], at(0));
    assert!(low < middle && middle < high);

    for _ in 0..5 {
        assert_eq!(
            ids(&ask(&store, &ContextRequest::with_query("hipertensão"))),
            vec![low, middle, high]
        );
    }
}

// ------------------------------------------------- the order is in the data

#[test]
fn the_order_never_comes_from_the_order_the_notes_were_written() {
    let notes: Vec<(Uuid, &str, i64)> = vec![
        (id(1), "Hipertensão arterial no idoso.", 0),
        (id(2), "Hipertensão arterial resistente.", 5),
        (id(3), "Hipertensão pulmonar.", 10),
        (id(4), "Insônia e plantão noturno.", 15),
    ];

    let forwards = Store::new();
    for (note, body, minutes) in &notes {
        forwards.put(*note, body, &[], &[], at(*minutes));
    }

    let backwards = Store::new();
    for (note, body, minutes) in notes.iter().rev() {
        backwards.put(*note, body, &[], &[], at(*minutes));
    }

    for request in [
        ContextRequest::with_query("hipertensão"),
        ContextRequest::empty(),
    ] {
        assert_eq!(
            ids(&ask(&forwards, &request)),
            ids(&ask(&backwards, &request)),
            "the same notes answer the same way whichever order they were laid down in"
        );
    }
}

// ------------------------------ filter-only and recency-only stay requests

#[test]
fn a_filter_with_no_query_is_a_request_and_not_a_mistake() {
    let store = Store::new();
    let tagged = store.put(id(1), "Hipertensão.", &["cardio"], &[], at(0));
    let sourced = store.put(id(2), "Diretriz.", &[], &[("fonte", "diretriz")], at(5));
    store.put(id(3), "Nada a ver.", &[], &[], at(10));

    assert_eq!(ids(&ask(&store, &with_tags("", &["cardio"]))), vec![tagged]);
    assert_eq!(
        ids(&ask(&store, &with_properties("", &[("fonte", "diretriz")]))),
        vec![sourced]
    );
}

#[test]
fn a_query_that_folds_to_nothing_answers_nothing_and_does_not_fall_back() {
    let store = Store::new();
    store.put(id(1), "Hipertensão arterial.", &[], &[], at(0));

    // Combining marks alone: `trim()` is not empty, so the request counts as
    // carrying a signal and `Recent` is not offered — and the fold drops them
    // all, so nothing can match. Measured in 4.3A.R1.2 and recorded as the
    // behaviour to preserve rather than to change by accident.
    let candidates = ask(&store, &ContextRequest::with_query("\u{0301}\u{0302}"));

    assert!(candidates.is_empty());
}
