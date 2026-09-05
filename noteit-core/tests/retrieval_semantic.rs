//! The semantic channel, with no model anywhere near it.
//!
//! Every provider in this file is a dictionary of three words written down in
//! the test. That is deliberate and it is the whole reason 4.3B and 4.3C are
//! separate phases: with a real model fitted here, a wrong answer would have
//! two possible authors — the engine or the weights — and no way to tell them
//! apart. A provider that is a `match` on three strings cannot be the one at
//! fault, so what these tests fail on is the engine.
//!
//! What is under test is provenance, mostly. A vector is a claim about a note
//! at a version, and the only thing that makes it safe to publish is that the
//! claim is checked against the note as it is now, from the reading the engine
//! was going to do anyway. Everything below is a way for that claim to be
//! false: the note was edited, only its tag was edited, it went to the trash,
//! it never existed, the chunker changed underneath it.

use noteit_core::chrono::{TimeZone, Utc};
use noteit_core::chunking::{ChunkId, CHUNKER_VERSION};
use noteit_core::context::{
    retrieve, retrieve_with, Candidate, ContextRequest, Reason, RetrievalError, RetrievalMode,
    RetrievalOutcome, SemanticStatus,
};
use noteit_core::embedding::{
    ArtifactManifestV1, Embedding, EmbeddingRole, EmbeddingSpaceId, EmbeddingVector, SemanticError,
};
use noteit_core::filter::NoteFilter;
use noteit_core::hashing::sha256_hex;
use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use noteit_core::revision::NoteRevision;
use noteit_core::semantic::{
    index_document, EmbeddingProvider, EmbeddingRecord, InMemoryIndex, SemanticIndex,
    SemanticPolicy, SemanticRuntime,
};
use noteit_core::{NoteItCore, StorageManager, StorePaths, Uuid};
use std::cell::Cell;
use std::path::Path;
use tempfile::{tempdir, TempDir};

// -------------------------------------------------------------- the fakes

/// The axes the toy space is built on. Three words, and a text's vector is how
/// often each of them occurs.
const AXES: [&str; 3] = ["cardio", "sono", "estudo"];

fn digest(byte: u8) -> String {
    sha256_hex(&[byte])
}

fn space_named(provider: &str, model: &str, weights: u8) -> EmbeddingSpaceId {
    EmbeddingSpaceId {
        provider: provider.to_string(),
        model: model.to_string(),
        artifact: ArtifactManifestV1 {
            weights_sha256: digest(weights),
            tokenizer_sha256: digest(2),
            embedding_recipe_version: 1,
            normalization_version: 1,
        }
        .identity()
        .expect("identity"),
        dimension: AXES.len(),
        embedding_recipe: 1,
        normalization: 1,
    }
}

fn toy_space() -> EmbeddingSpaceId {
    space_named("test-dictionary", "three-words", 1)
}

fn values_for(text: &str) -> Vec<f32> {
    let lowered = text.to_lowercase();
    AXES.iter()
        // A floor on every axis, so no text ever produces the zero vector the
        // representation refuses — the fake must not be the thing that fails.
        .map(|axis| 0.01 + lowered.matches(axis).count() as f32)
        .collect()
}

/// How the fake provider misbehaves, when the test wants it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Misbehaviour {
    None,
    /// Answers nothing at all.
    Unavailable,
    /// Returns one vector fewer than the batch asked for.
    ShortBatch,
    /// Answers a query with a document-role vector.
    WrongRole,
    /// Answers from a space it did not advertise.
    WrongSpace,
}

struct Dictionary {
    space: EmbeddingSpaceId,
    misbehaviour: Misbehaviour,
    document_calls: Cell<usize>,
    query_calls: Cell<usize>,
}

impl Dictionary {
    fn new() -> Self {
        Self {
            space: toy_space(),
            misbehaviour: Misbehaviour::None,
            document_calls: Cell::new(0),
            query_calls: Cell::new(0),
        }
    }

    fn misbehaving(misbehaviour: Misbehaviour) -> Self {
        Self {
            misbehaviour,
            ..Self::new()
        }
    }

    fn in_space(space: EmbeddingSpaceId) -> Self {
        Self {
            space,
            ..Self::new()
        }
    }

    fn calls(&self) -> usize {
        self.document_calls.get() + self.query_calls.get()
    }

    fn embedding(&self, role: EmbeddingRole, text: &str) -> Embedding {
        let space = if self.misbehaviour == Misbehaviour::WrongSpace {
            space_named("test-dictionary", "three-words", 200)
        } else {
            self.space.clone()
        };
        Embedding::new(
            space,
            role,
            EmbeddingVector::new(values_for(text)).expect("the fake never makes a bad vector"),
        )
        .expect("the fake never makes a bad shape")
    }
}

impl EmbeddingProvider for Dictionary {
    fn space(&self) -> EmbeddingSpaceId {
        self.space.clone()
    }

    fn embed_document(&self, texts: &[String]) -> Result<Vec<Embedding>, SemanticError> {
        self.document_calls.set(self.document_calls.get() + 1);
        match self.misbehaviour {
            Misbehaviour::Unavailable => Err(SemanticError::Unavailable),
            Misbehaviour::ShortBatch => Ok(texts
                .iter()
                .skip(1)
                .map(|text| self.embedding(EmbeddingRole::Document, text))
                .collect()),
            _ => Ok(texts
                .iter()
                .map(|text| self.embedding(EmbeddingRole::Document, text))
                .collect()),
        }
    }

    fn embed_query(&self, text: &str) -> Result<Embedding, SemanticError> {
        self.query_calls.set(self.query_calls.get() + 1);
        match self.misbehaviour {
            Misbehaviour::Unavailable => Err(SemanticError::Unavailable),
            Misbehaviour::WrongRole => Ok(self.embedding(EmbeddingRole::Document, text)),
            _ => Ok(self.embedding(EmbeddingRole::Query, text)),
        }
    }
}

/// A provider that cannot be called without failing the test.
struct Forbidden;

impl EmbeddingProvider for Forbidden {
    fn space(&self) -> EmbeddingSpaceId {
        toy_space()
    }
    fn embed_document(&self, _: &[String]) -> Result<Vec<Embedding>, SemanticError> {
        panic!("the lexical path embedded a document");
    }
    fn embed_query(&self, _: &str) -> Result<Embedding, SemanticError> {
        panic!("the lexical path embedded a query");
    }
}

// ------------------------------------------------------------------ store

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

    /// Writes one note at a fixed instant.
    ///
    /// The instant is the same for every note on purpose. It makes recency a
    /// non-factor, so a tie falls through to `note_id` and the order of an
    /// answer is something the test chose rather than something the clock did —
    /// and it lets a scenario change a tag without the timestamp moving, which
    /// is the whole point of one of them.
    fn put(&self, id: Uuid, body: &str, tags: &[&str], properties: &[(&str, &str)]) -> Uuid {
        let stamp = Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("a real instant");
        let mut document = NoteDocument::new_empty();
        document.metadata.id = id;
        document.metadata.created_at = Some(stamp);
        document.metadata.updated_at = Some(stamp);
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

    fn document(&self, id: &Uuid) -> NoteDocument {
        self.core.read_note(id).expect("the note is readable")
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

fn id(nth: u8) -> Uuid {
    Uuid::from_u128(0x5365_6d61_6e74_6963_0000_0000_0000_0000u128 | nth as u128)
}

fn ask(
    store: &Store,
    request: &ContextRequest,
    provider: &dyn EmbeddingProvider,
    index: &mut dyn SemanticIndex,
) -> Vec<Candidate> {
    retrieve_with(
        &store.core,
        request,
        RetrievalMode::Semantic(SemanticRuntime::new(provider, index)),
    )
    .expect("the request must be answered")
    .result
    .candidates
}

fn ids(candidates: &[Candidate]) -> Vec<Uuid> {
    candidates
        .iter()
        .map(|candidate| candidate.note_id)
        .collect()
}

// ------------------------------------------ the default never reaches out

/// The most important negative test in the phase.
#[test]
fn the_default_path_cannot_call_a_provider_because_it_has_nowhere_to_put_one() {
    let store = Store::new();
    store.put(id(1), "cardio e sono", &[], &[]);
    let provider = Forbidden;
    let index = InMemoryIndex::new(toy_space());

    // Not "the provider was not configured" and not "a flag was off": the mode
    // `retrieve` uses carries no provider field at all, so there is nothing an
    // accident could switch on.
    let answer = retrieve(&store.core, &ContextRequest::with_query("cardio"))
        .expect("the lexical path answers");
    assert_eq!(answer.candidates.len(), 1);
    assert_eq!(index.vector_count(), 0);
    let _ = provider;
}

#[test]
fn the_lexical_path_leaves_the_index_untouched_even_when_one_exists() {
    let store = Store::new();
    let note = store.put(id(1), "cardio e sono", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");
    let after_indexing = provider.calls();

    retrieve(&store.core, &ContextRequest::with_query("estudo")).expect("answers");

    assert_eq!(
        provider.calls(),
        after_indexing,
        "the lexical path called the provider"
    );
    assert_eq!(
        index.vector_count(),
        1,
        "the lexical path changed the index"
    );
}

// ----------------------------------------------------------- the channel

#[test]
fn a_note_found_only_by_meaning_is_admitted_and_labelled() {
    let store = Store::new();
    // The note never uses the query's word, so nothing lexical can admit it.
    let note = store.put(id(1), "cardio cardio cardio", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");

    let candidates = ask(
        &store,
        &ContextRequest::with_query("cardio"),
        &provider,
        &mut index,
    );

    // "cardio" *is* a term of that note, so this note is class 1 by term. Use a
    // question whose words are absent instead.
    assert!(!candidates.is_empty());

    let quiet = ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );
    assert_eq!(ids(&quiet), vec![note]);
    assert_eq!(quiet[0].reasons, vec![Reason::SemanticMatch]);
    assert_eq!(
        quiet[0].matched_text, None,
        "there is no substring of this note anybody could honestly call matched"
    );
    assert!(!quiet[0].snippet.is_empty());
}

#[test]
fn a_semantic_candidate_shows_the_paragraph_the_index_pointed_at() {
    let store = Store::new();
    let body = "primeiro parágrafo sem nada\n\nsegundo parágrafo sobre cardio\n\nterceiro sem nada";
    let note = store.put(id(1), body, &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    assert_eq!(
        index_document(&store.document(&note), &provider, &mut index).expect("index"),
        3,
        "three paragraphs, three vectors"
    );

    let candidates = ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );

    assert_eq!(ids(&candidates), vec![note]);
    assert!(
        candidates[0]
            .snippet
            .contains("segundo parágrafo sobre cardio"),
        "the snippet must come from the winning chunk of the current reading: {}",
        candidates[0].snippet
    );
}

#[test]
fn a_note_with_many_chunks_is_still_one_candidate() {
    let store = Store::new();
    let body = (0..12)
        .map(|nth| format!("parágrafo {nth} sobre cardio"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let note = store.put(id(1), &body, &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    assert_eq!(
        index_document(&store.document(&note), &provider, &mut index).expect("index"),
        12
    );

    let candidates = ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );

    assert_eq!(
        ids(&candidates),
        vec![note],
        "twelve vectors must not become twelve candidates"
    );
}

#[test]
fn the_semantic_channel_may_only_add_so_many_strangers() {
    let store = Store::new();
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    for nth in 1..=8u8 {
        let note = store.put(id(nth), &format!("cardio número {nth}"), &[], &[]);
        index_document(&store.document(&note), &provider, &mut index).expect("index");
    }

    let policy = SemanticPolicy::default();
    let candidates = retrieve_with(
        &store.core,
        &ContextRequest::with_query("coracao"),
        RetrievalMode::Semantic(SemanticRuntime::new(&provider, &mut index).with_policy(policy)),
    )
    .expect("answers")
    .result
    .candidates;

    assert_eq!(
        candidates.len(),
        policy.max_semantic_only,
        "a nearest-neighbour search always has a nearest neighbour; the ceiling \
         is what keeps \"nothing matched your words\" legible"
    );
    for candidate in &candidates {
        assert_eq!(candidate.reasons, vec![Reason::SemanticMatch]);
    }
}

#[test]
fn similarity_however_large_never_crosses_a_lexical_candidate() {
    let store = Store::new();
    // `exact` is admitted by the phrase. `close` is the note the toy space
    // scores highest for the question, by a mile — and it still goes second.
    let exact = store.put(id(1), "coracao mencionado uma vez", &[], &[]);
    let close = store.put(id(2), "cardio cardio cardio cardio cardio", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    for note in [exact, close] {
        index_document(&store.document(&note), &provider, &mut index).expect("index");
    }

    let candidates = ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );

    assert_eq!(ids(&candidates), vec![exact, close]);
    assert!(candidates[0].reasons.contains(&Reason::TextMatch));
    assert_eq!(candidates[1].reasons, vec![Reason::SemanticMatch]);
}

#[test]
fn a_candidate_admitted_twice_appears_once_and_says_both() {
    let store = Store::new();
    let note = store.put(id(1), "cardio e coracao", &["cardio"], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");

    let request = ContextRequest {
        query: "coracao".to_string(),
        filter: NoteFilter::new(vec!["cardio".to_string()], Vec::new()),
        ..ContextRequest::default()
    };
    let candidates = ask(&store, &request, &provider, &mut index);

    assert_eq!(ids(&candidates), vec![note]);
    assert_eq!(
        candidates[0].reasons,
        vec![
            Reason::TextMatch,
            Reason::TermMatch,
            Reason::SharedTag,
            Reason::SemanticMatch
        ],
        "one candidate, every applicable reason, in the published order"
    );
}

// --------------------------------------------------------- provenance

/// The whole point of the phase, end to end.
#[test]
fn a_vector_about_a_version_that_no_longer_exists_is_discarded_and_forgotten() {
    let store = Store::new();
    let note = store.put(id(1), "cardio na versão A", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");
    assert_eq!(index.vector_count(), 1);

    // The note moves on. The index does not.
    store.put(id(1), "cardio na versão B, bem diferente", &[], &[]);

    let candidates = ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );

    assert!(
        candidates.is_empty(),
        "a record about a revision that no longer exists was published"
    );
    assert_eq!(
        index.vector_count(),
        0,
        "the stale record must be forgotten, not rediscovered on every query"
    );
    // And the note itself is exactly as the edit left it.
    assert_eq!(
        store.document(&note).content,
        "cardio na versão B, bem diferente"
    );
}

/// The regression for R1-002: `updated_at` is not a version token.
#[test]
fn a_change_to_a_tag_alone_is_still_a_change() {
    let store = Store::new();
    let note = store.put(id(1), "cardio e nada mais", &["antes"], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");

    let before = store.document(&note);
    // Only the metadata moves. The text is byte for byte what it was, so
    // `updated_at` has no reason to move and does not — which is exactly why it
    // cannot be what decides whether a vector is still good.
    store.put(id(1), "cardio e nada mais", &["depois"], &[]);
    let after = store.document(&note);
    assert_eq!(before.content, after.content);
    assert_eq!(before.metadata.updated_at, after.metadata.updated_at);

    let candidates = ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );

    assert!(
        candidates.is_empty(),
        "the revision moved and the record did not; it must not have been published"
    );
    assert_eq!(index.vector_count(), 0);
}

#[test]
fn a_record_about_a_note_that_is_gone_is_dropped_without_resurrecting_it() {
    let store = Store::new();
    let gone = store.put(id(1), "cardio que vai sumir", &[], &[]);
    let alive = store.put(id(2), "outra nota qualquer", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&gone), &provider, &mut index).expect("index");
    index_document(&store.document(&alive), &provider, &mut index).expect("index");

    store
        .core
        .storage()
        .move_note_to_trash(&gone)
        .expect("trash");

    let candidates = ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );

    assert!(
        !ids(&candidates).contains(&gone),
        "a note in the trash came back as memory"
    );
    assert_eq!(
        store.core.storage().list_trash().len(),
        1,
        "and it is still in the trash, untouched"
    );
    for candidate in &candidates {
        assert_ne!(candidate.note_id, gone);
    }
    // The orphan is forgotten; the live note's record survives.
    assert_eq!(index.vector_count(), 1);
}

#[test]
fn a_record_from_another_chunker_never_gets_into_the_index() {
    let store = Store::new();
    let body = "cardio e mais cardio";
    let note = store.put(id(1), body, &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");
    assert_eq!(index.vector_count(), 1);

    // A record cut by a chunker that is not the one running is refused at the
    // door rather than discovered by somebody searching. The boundaries would
    // have moved underneath it, so the position it names is not the position
    // its vector is about.
    let document = store.document(&note);
    let revision = NoteRevision::for_document(&document).expect("revision");
    let foreign = CHUNKER_VERSION + 1;
    let record = EmbeddingRecord {
        note_id: note,
        source_revision: revision.clone(),
        chunk_id: ChunkId::of(&note, &revision, 0, foreign, body).expect("chunk id"),
        chunker_version: foreign,
        space: toy_space(),
        vector: Embedding::new(
            toy_space(),
            EmbeddingRole::Document,
            EmbeddingVector::new(values_for(body)).expect("vector"),
        )
        .expect("embedding"),
    };

    assert_eq!(
        index.replace_note(&note, vec![record]),
        Err(SemanticError::ChunkerMismatch {
            expected: CHUNKER_VERSION,
            actual: foreign,
        })
    );
    assert_eq!(
        index.vector_count(),
        1,
        "the refused batch left the note exactly as it was"
    );
}

// ----------------------------------------------------------- the boundary

#[test]
fn a_batch_that_came_back_short_indexes_nothing() {
    let store = Store::new();
    let note = store.put(id(1), "um\n\ndois\n\ntrês\n\nquatro\n\ncinco", &[], &[]);
    let honest = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &honest, &mut index).expect("index");
    assert_eq!(index.vector_count(), 5);

    let short = Dictionary::misbehaving(Misbehaviour::ShortBatch);
    assert_eq!(
        index_document(&store.document(&note), &short, &mut index),
        Err(SemanticError::InvalidResponse)
    );
    assert_eq!(
        index.vector_count(),
        5,
        "a partial batch must leave the previous state exactly as it was"
    );
}

#[test]
fn a_provider_answering_from_another_space_is_caught_at_the_door() {
    let store = Store::new();
    let note = store.put(id(1), "cardio", &[], &[]);
    let liar = Dictionary::misbehaving(Misbehaviour::WrongSpace);
    let mut index = InMemoryIndex::new(toy_space());

    assert_eq!(
        index_document(&store.document(&note), &liar, &mut index),
        Err(SemanticError::SpaceMismatch)
    );
    assert_eq!(index.vector_count(), 0);
}

#[test]
fn an_index_refuses_a_provider_that_is_not_its_own() {
    let store = Store::new();
    let note = store.put(id(1), "cardio", &[], &[]);
    let provider = Dictionary::in_space(space_named("other", "three-words", 1));
    let mut index = InMemoryIndex::new(toy_space());

    assert_eq!(
        index_document(&store.document(&note), &provider, &mut index),
        Err(SemanticError::SpaceMismatch)
    );
}

#[test]
fn a_query_answered_with_a_document_vector_is_not_an_answer() {
    let store = Store::new();
    let note = store.put(id(1), "cardio", &[], &[]);
    let honest = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &honest, &mut index).expect("index");

    let confused = Dictionary::misbehaving(Misbehaviour::WrongRole);
    let error = retrieve_with(
        &store.core,
        &ContextRequest::with_query("coracao"),
        RetrievalMode::Semantic(SemanticRuntime::new(&confused, &mut index).requiring_semantics()),
    )
    .expect_err("a role that was not asked for is an invalid response");
    assert_eq!(
        error,
        RetrievalError::Semantic(SemanticError::InvalidResponse)
    );
}

#[test]
fn an_empty_note_holds_no_vectors_and_says_so() {
    let store = Store::new();
    let note = store.put(id(1), "cardio", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    assert_eq!(
        index_document(&store.document(&note), &provider, &mut index).expect("index"),
        1
    );

    store.put(id(1), "   \n\n  ", &[], &[]);
    assert_eq!(
        index_document(&store.document(&note), &provider, &mut index).expect("index"),
        0
    );
    assert_eq!(index.vector_count(), 0);
}

// ------------------------------------------------------------- fallback

#[test]
fn automatic_degrades_to_the_lexical_answer_and_records_unavailable_status() {
    let store = Store::new();
    let note = store.put(id(1), "cardio e sono", &[], &[]);
    let broken = Dictionary::misbehaving(Misbehaviour::Unavailable);
    let mut index = InMemoryIndex::new(toy_space());

    let outcome: RetrievalOutcome = retrieve_with(
        &store.core,
        &ContextRequest::with_query("cardio"),
        RetrievalMode::Semantic(SemanticRuntime::new(&broken, &mut index)),
    )
    .expect("automatic degrades rather than failing");

    assert_eq!(
        outcome.semantic_status,
        SemanticStatus::Unavailable,
        "the automatic fallback must record that the semantic channel failed"
    );
    assert_eq!(ids(&outcome.result.candidates), vec![note]);
    assert!(outcome.result.candidates[0]
        .reasons
        .contains(&Reason::TermMatch));
    assert!(!outcome.result.candidates[0]
        .reasons
        .contains(&Reason::SemanticMatch));
}

#[test]
fn healthy_provider_answers_with_succeeded_status() {
    let store = Store::new();
    let note = store.put(id(1), "cardio e sono", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");

    let outcome = retrieve_with(
        &store.core,
        &ContextRequest::with_query("cardio"),
        RetrievalMode::Semantic(SemanticRuntime::new(&provider, &mut index)),
    )
    .expect("retrieval succeeds");

    assert_eq!(outcome.semantic_status, SemanticStatus::Succeeded);
    assert_eq!(ids(&outcome.result.candidates), vec![note]);
    assert_eq!(provider.query_calls.get(), 1);
}

#[test]
fn lexical_only_mode_records_not_requested_and_never_calls_provider() {
    let store = Store::new();
    store.put(id(1), "cardio e sono", &[], &[]);

    let outcome = retrieve_with(
        &store.core,
        &ContextRequest::with_query("cardio"),
        RetrievalMode::LexicalOnly,
    )
    .expect("lexical retrieval succeeds");

    assert_eq!(outcome.semantic_status, SemanticStatus::NotRequested);
    assert_eq!(outcome.result.candidates.len(), 1);
}

#[test]
fn required_refuses_rather_than_pretending() {
    let store = Store::new();
    store.put(id(1), "cardio e sono", &[], &[]);
    let broken = Dictionary::misbehaving(Misbehaviour::Unavailable);
    let mut index = InMemoryIndex::new(toy_space());

    let error = retrieve_with(
        &store.core,
        &ContextRequest::with_query("cardio"),
        RetrievalMode::Semantic(SemanticRuntime::new(&broken, &mut index).requiring_semantics()),
    )
    .expect_err("somebody who asked for semantics has to be told they did not get it");

    assert_eq!(error, RetrievalError::Semantic(SemanticError::Unavailable));
}

#[test]
fn semantic_mode_with_empty_query_records_not_requested_and_does_not_call_provider() {
    let store = Store::new();
    let note = store.put(id(1), "cardio e sono", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");

    let doc_calls_before = provider.document_calls.get();
    let query_calls_before = provider.query_calls.get();

    let outcome = retrieve_with(
        &store.core,
        &ContextRequest::with_query(""),
        RetrievalMode::Semantic(SemanticRuntime::new(&provider, &mut index)),
    )
    .expect("retrieval succeeds");

    assert_eq!(outcome.semantic_status, SemanticStatus::NotRequested);
    assert_eq!(
        provider.query_calls.get(),
        query_calls_before,
        "empty query must not query the provider"
    );
    assert_eq!(
        provider.document_calls.get(),
        doc_calls_before,
        "retrieval must not embed documents"
    );
    // An empty request produces recency-only candidates
    assert_eq!(ids(&outcome.result.candidates), vec![note]);
    assert_eq!(outcome.result.candidates[0].reasons, vec![Reason::Recent]);
}

#[test]
fn semantic_mode_with_filter_only_records_not_requested_and_does_not_call_provider() {
    let store = Store::new();
    let tagged = store.put(id(1), "Hipertensão.", &["cardio"], &[]);
    let other = store.put(id(2), "Outra nota.", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&tagged), &provider, &mut index).expect("index");
    index_document(&store.document(&other), &provider, &mut index).expect("index");

    let query_calls_before = provider.query_calls.get();

    let request = ContextRequest {
        query: String::new(),
        filter: NoteFilter::new(vec!["cardio".to_string()], Vec::new()),
        include_tasks: false,
        limit: None,
    };

    let outcome = retrieve_with(
        &store.core,
        &request,
        RetrievalMode::Semantic(SemanticRuntime::new(&provider, &mut index)),
    )
    .expect("filter retrieval succeeds");

    assert_eq!(outcome.semantic_status, SemanticStatus::NotRequested);
    assert_eq!(provider.query_calls.get(), query_calls_before);
    assert_eq!(ids(&outcome.result.candidates), vec![tagged]);
    assert_eq!(
        outcome.result.candidates[0].reasons,
        vec![Reason::SharedTag]
    );
    assert!(
        !outcome.result.candidates[0]
            .reasons
            .contains(&Reason::SemanticMatch),
        "no semantic match reason can be produced without a query"
    );
}

#[test]
fn semantic_mode_with_query_folding_to_empty_records_not_requested_and_does_not_call_provider() {
    let store = Store::new();
    let note = store.put(id(1), "Hipertensão arterial.", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");

    let query_calls_before = provider.query_calls.get();

    // Combining marks alone fold to empty
    let outcome = retrieve_with(
        &store.core,
        &ContextRequest::with_query("\u{0301}\u{0302}"),
        RetrievalMode::Semantic(SemanticRuntime::new(&provider, &mut index)),
    )
    .expect("retrieval succeeds");

    assert_eq!(outcome.semantic_status, SemanticStatus::NotRequested);
    assert_eq!(provider.query_calls.get(), query_calls_before);
    // Preserves existing behavior: does not fall back to Recent, returns no candidates
    assert!(outcome.result.candidates.is_empty());
}

#[test]
fn required_fallback_without_applicable_query_does_not_error_and_records_not_requested() {
    let store = Store::new();
    let note = store.put(id(1), "cardio e sono", &[], &[]);
    // Broken provider that would fail if called
    let broken = Dictionary::misbehaving(Misbehaviour::Unavailable);
    let mut index = InMemoryIndex::new(toy_space());

    let outcome = retrieve_with(
        &store.core,
        &ContextRequest::with_query(""),
        RetrievalMode::Semantic(SemanticRuntime::new(&broken, &mut index).requiring_semantics()),
    )
    .expect("no query means semantic was not attempted, so Required does not trigger an error");

    assert_eq!(outcome.semantic_status, SemanticStatus::NotRequested);
    assert_eq!(broken.query_calls.get(), 0);
    assert_eq!(ids(&outcome.result.candidates), vec![note]);
}

#[test]
fn semantic_status_invariants_hold_across_all_retrieval_outcomes() {
    let store = Store::new();
    let note = store.put(id(1), "cardio e sono", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");

    // Invariant 1: Succeeded implies provider query_calls > 0
    let outcome_succeeded = retrieve_with(
        &store.core,
        &ContextRequest::with_query("cardio"),
        RetrievalMode::Semantic(SemanticRuntime::new(&provider, &mut index)),
    )
    .expect("retrieval succeeds");
    assert_eq!(outcome_succeeded.semantic_status, SemanticStatus::Succeeded);
    assert!(provider.query_calls.get() > 0);

    // Invariant 2: Unavailable implies provider was attempted and failed
    let broken = Dictionary::misbehaving(Misbehaviour::Unavailable);
    let mut broken_index = InMemoryIndex::new(toy_space());
    let outcome_unavailable = retrieve_with(
        &store.core,
        &ContextRequest::with_query("cardio"),
        RetrievalMode::Semantic(SemanticRuntime::new(&broken, &mut broken_index)),
    )
    .expect("automatic fallback succeeds");
    assert_eq!(
        outcome_unavailable.semantic_status,
        SemanticStatus::Unavailable
    );
    assert!(broken.query_calls.get() > 0);

    // Invariant 3: provider query_calls == 0 implies status == NotRequested
    let fresh_provider = Dictionary::new();
    let mut fresh_index = InMemoryIndex::new(toy_space());
    let outcome_not_requested = retrieve_with(
        &store.core,
        &ContextRequest::with_query(""),
        RetrievalMode::Semantic(SemanticRuntime::new(&fresh_provider, &mut fresh_index)),
    )
    .expect("empty query retrieval succeeds");
    assert_eq!(fresh_provider.query_calls.get(), 0);
    assert_eq!(
        outcome_not_requested.semantic_status,
        SemanticStatus::NotRequested
    );
    assert_ne!(
        outcome_not_requested.semantic_status,
        SemanticStatus::Succeeded
    );
    assert_ne!(
        outcome_not_requested.semantic_status,
        SemanticStatus::Unavailable
    );
}

// ------------------------------------------------ Reason coexistence (R1-001)

#[test]
fn text_match_and_term_match_and_semantic_match_coexist_without_false_exclusivity() {
    let store = Store::new();
    let note = store.put(id(1), "cardio e sono", &[], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    index_document(&store.document(&note), &provider, &mut index).expect("index");

    // The query is an exact phrase "cardio e sono".
    // 1. TextMatch is produced because the full phrase occurs.
    // 2. TermMatch is produced because normalized terms occur in visible text.
    // 3. SemanticMatch is produced because the semantic channel admitted the note.
    let outcome = retrieve_with(
        &store.core,
        &ContextRequest::with_query("cardio e sono"),
        RetrievalMode::Semantic(SemanticRuntime::new(&provider, &mut index)),
    )
    .expect("retrieval succeeds");

    let candidate = &outcome.result.candidates[0];
    assert!(
        candidate.reasons.contains(&Reason::TextMatch),
        "TextMatch must be present"
    );
    assert!(
        candidate.reasons.contains(&Reason::TermMatch),
        "TermMatch can and must coexist with TextMatch: a phrase also contains its terms"
    );
    assert!(
        candidate.reasons.contains(&Reason::SemanticMatch),
        "SemanticMatch can and must coexist with lexical matches when admitted"
    );
}

// ------------------------------------------------- when there is no query

#[test]
fn a_request_with_no_query_never_embeds_anything() {
    let store = Store::new();
    store.put(id(1), "cardio", &["cardio"], &[]);
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());

    for request in [
        ContextRequest::empty(),
        ContextRequest {
            filter: NoteFilter::new(vec!["cardio".to_string()], Vec::new()),
            ..ContextRequest::default()
        },
        // A query of nothing but combining marks folds away, so there is no
        // question left to embed either.
        ContextRequest::with_query("\u{0301}\u{0302}"),
    ] {
        ask(&store, &request, &provider, &mut index);
    }

    assert_eq!(
        provider.calls(),
        0,
        "embedding an empty question means nothing, and costs a call to find out"
    );
}

// -------------------------------------------------------- determinism

#[test]
fn the_same_question_over_the_same_store_answers_identically() {
    let store = Store::new();
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    for nth in 1..=6u8 {
        let note = store.put(id(nth), &format!("cardio e sono, nota {nth}"), &[], &[]);
        index_document(&store.document(&note), &provider, &mut index).expect("index");
    }

    let first = ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );
    for _ in 0..5 {
        assert_eq!(
            first,
            ask(
                &store,
                &ContextRequest::with_query("coracao"),
                &provider,
                &mut index,
            )
        );
    }
}

#[test]
fn notes_that_are_equally_close_are_ordered_by_identifier() {
    let store = Store::new();
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    // Identical text, so identical vectors, so a tie that only `note_id` can
    // break.
    let notes: Vec<Uuid> = (1..=3u8)
        .map(|nth| {
            let note = store.put(id(nth), "cardio", &[], &[]);
            index_document(&store.document(&note), &provider, &mut index).expect("index");
            note
        })
        .collect();

    let candidates = ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );

    assert_eq!(ids(&candidates), notes, "sorted, and the sort is total");
}

// ----------------------------------------------------- nothing is written

#[test]
fn asking_with_the_semantic_channel_on_writes_absolutely_nothing() {
    let store = Store::new();
    let provider = Dictionary::new();
    let mut index = InMemoryIndex::new(toy_space());
    for nth in 1..=4u8 {
        let note = store.put(id(nth), &format!("cardio {nth}\n\nsono {nth}"), &[], &[]);
        index_document(&store.document(&note), &provider, &mut index).expect("index");
    }
    let before = fingerprint(store.core.storage().paths().notes_dir.as_path());

    ask(
        &store,
        &ContextRequest::with_query("coracao"),
        &provider,
        &mut index,
    );
    ask(&store, &ContextRequest::empty(), &provider, &mut index);

    assert_eq!(
        before,
        fingerprint(store.core.storage().paths().notes_dir.as_path()),
        "a question changed a note"
    );
}

fn fingerprint(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let bytes = std::fs::read(&path).unwrap_or_default();
        out.push(format!(
            "{} {}",
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            sha256_hex(&bytes)
        ));
    }
    out.sort();
    out
}

/// The invariant the search's hot path rests on, asserted where it is made.
///
/// `InMemoryIndex::nearest_notes` compares the query's space against the
/// index's once and then compares *vectors*, because every record in the index
/// was refused unless it declared exactly that space. That shortcut is worth
/// 19.8 ms against 3 ms over twenty thousand vectors — a search should not be
/// twenty thousand string comparisons — and it is only sound while `replace_note`
/// is the one door and it stays shut. These three assertions are that door.
#[test]
fn the_index_refuses_every_record_that_is_not_of_its_own_space() {
    let mine = toy_space();
    let mut index = InMemoryIndex::new(mine.clone());
    let note = id(1);

    let foreign = space_named("test-dictionary", "three-words", 9);
    assert_eq!(
        foreign.dimension, mine.dimension,
        "the point of this test is that the shapes agree and the spaces do not"
    );

    let vector = EmbeddingVector::new(values_for("cardio")).expect("vector");
    let honest = Embedding::new(mine.clone(), EmbeddingRole::Document, vector.clone())
        .expect("an honest embedding");
    let alien = Embedding::new(foreign.clone(), EmbeddingRole::Document, vector)
        .expect("an embedding of another space");

    let record = |space: EmbeddingSpaceId, vector: Embedding| EmbeddingRecord {
        note_id: note,
        source_revision: NoteRevision::parse(&digest(7)).expect("a revision"),
        chunk_id: ChunkId::of(
            &note,
            &NoteRevision::parse(&digest(7)).expect("a revision"),
            0,
            CHUNKER_VERSION,
            "cardio",
        )
        .expect("chunk id"),
        chunker_version: CHUNKER_VERSION,
        space,
        vector,
    };

    // A record whose declared space is foreign.
    assert_eq!(
        index.replace_note(&note, vec![record(foreign.clone(), honest.clone())]),
        Err(SemanticError::SpaceMismatch)
    );
    // A record whose *vector* is foreign, however the record labels itself.
    assert_eq!(
        index.replace_note(&note, vec![record(mine.clone(), alien.clone())]),
        Err(SemanticError::SpaceMismatch)
    );
    // And a batch where only the last one lies still indexes nothing.
    assert_eq!(
        index.replace_note(
            &note,
            vec![record(mine.clone(), honest.clone()), record(foreign, alien),]
        ),
        Err(SemanticError::SpaceMismatch)
    );
    assert_eq!(
        index.vector_count(),
        0,
        "a refused batch left something behind"
    );

    // And a question from another space is refused before any arithmetic, so
    // the per-record shortcut is never reached with a mismatched query.
    index
        .replace_note(&note, vec![record(mine.clone(), honest)])
        .expect("the honest record is accepted");
    let stranger = Embedding::new(
        space_named("test-dictionary", "three-words", 9),
        EmbeddingRole::Query,
        EmbeddingVector::new(values_for("cardio")).expect("vector"),
    )
    .expect("a query of another space");
    assert_eq!(
        index.nearest_notes(&stranger, 10),
        Err(SemanticError::SpaceMismatch),
        "equal dimension is never compatibility"
    );
}
