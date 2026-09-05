//! Retrieval of context, for a program that has to decide what to read next.
//!
//! Phase 4.2's Second Brain is one sentence, and every word of it is a
//! requirement: *a read-only, deterministic, traceable context retrieval layer
//! over the knowledge already in the notes*. This module is that layer. It
//! selects; it does not interpret, summarise or conclude, and it does not
//! learn anything about the notes that the notes do not already say.
//!
//! See `docs/second-brain.md` for the contract and `docs/decisions.md`
//! ADR-048, ADR-048.1 and ADR-048.2 for why it is shaped this way.
//!
//! ## What it answers, and what it refuses to answer
//!
//! It answers "which notes look relevant, and why". It does not answer "what
//! is in this note" — that is [`crate::NoteItCore::read_note`], one note at a
//! time, at the cost of a full body. A candidate carries a snippet of at most
//! [`crate::search::MAX_SNIPPET_CHARS`] characters and never a whole note, so
//! the expensive step stays a decision somebody makes on purpose rather than
//! something that happens by asking a question.
//!
//! ## No revision, ever
//!
//! A candidate carries no `revision`, no `etag`, no version token of any kind.
//! That is the one decision here with teeth (ADR-048, D-13): a revision beside
//! a 240-character snippet would let a caller write over a note it has only
//! glimpsed, and the conflict that is supposed to stop it would not fire,
//! because there would be no conflict. `updated_at` travels instead, and it is
//! **recency and nothing else** — it moves when a note's *text* changes and
//! deliberately stays put when a tag, a property, a colour or a font size
//! does, so it cannot stand in for a version. To write, read the note.
//!
//! ## One projection per candidate
//!
//! Every signal about a note — its text, its label, its snippet, its tags, its
//! properties, its tasks, its `updated_at` — comes from a single
//! [`Projection`], built from one authoritative read of the [`NoteDocument`].
//! The scan before it may look at whatever enumerating and ordering the store
//! requires, and none of *that* reaches a candidate. That is D-27,
//! and it is a property rather than a preference: a candidate assembled from a
//! snippet read before an edit and tags read after it is not a note that ever
//! existed, and provenance about a note that never existed is a lie. The type
//! is what enforces it — every signal function below takes `&Projection` and
//! there is no path to the store from any of them.
//!
//! It is coherence *per note*, not across the store. Two candidates may come
//! from two different instants, and that costs nothing: no lease is taken, no
//! snapshot is held, no lock is acquired, and nothing here writes.
//!
//! ## Three channels, one engine
//!
//! Phase 4.3B added term-level retrieval and the frame for a semantic one. It
//! added them *here*, and that was the point: a second engine beside this one
//! would be a second place that reads the store, applies the filters, builds
//! the snippets, counts the tasks, enforces the ceilings and decides the order —
//! and two of those disagree the first week. There is one authority for
//! contextual retrieval.
//!
//! ```text
//! Context Engine
//!   ├── the declared signals   text, tag, property, task   (since 4.2)
//!   ├── lexical by term        BM25                        (4.3B)
//!   └── the semantic channel   optional, off by default    (4.3B frame)
//! ```
//!
//! The three do not compete for position. They are **chained**, in the
//! precedence classes 4.3A.R1.2 froze: the declared signals first, exactly as
//! they were ordered before any of this existed, then term matches, then
//! semantic ones, then recency. Classes two and three are strictly additive —
//! they add candidates below everything that already existed and move nothing.
//! That is what makes "an exact hit is never demoted" a property of the shape
//! rather than a hope about numeric scales, and it is why chaining was chosen
//! over score fusion, which 4.3A measured demoting one.
//!
//! ## The scores stay inside
//!
//! BM25 and cosine decide order and are never published. `0.873` is not
//! provenance — nobody can audit it and nobody can act on it — and a score on
//! the wire would also couple the protocol to whichever vendor produced the
//! scale. What travels is `term_match` and `semantic_match`: facts a person can
//! check by opening the note.

use crate::chunking::{chunk, ChunkId, CHUNKER_VERSION};
use crate::embedding::SemanticError;
use crate::filter::NoteFilter;
use crate::lexical::{CorpusStatistics, DocumentTerms, QueryTerms};
use crate::metadata::semantic_identity;
use crate::model::NoteDocument;
use crate::revision::NoteRevision;
use crate::search::{self, Folded, MAX_LABEL_CHARS, MAX_QUERY_CHARS, MAX_SNIPPET_CHARS};
use crate::semantic::{SemanticFallback, SemanticHit, SemanticRuntime};
use crate::task::{self, TaskEntry};
use crate::visible_text::visible_text;
use crate::warning::{ReadWarning, ReadWarningKind};
use crate::NoteItCore;
use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Candidates returned when the caller does not say. Half the reading API's
/// `unwrap_or(20)`: an opening context should be narrow, and widening it is a
/// decision the caller makes with a number.
pub const DEFAULT_CANDIDATES: usize = 10;

/// The most candidates any request can produce. Half the Core's `MAX_RESULTS`.
/// Fifty snippets of 240 characters is about 12 KB — a real slice of a context
/// window, and still small enough for a person to read while debugging.
pub const MAX_CANDIDATES: usize = 50;

/// Tasks published with one candidate.
///
/// A handful, deliberately. Tasks are a *signal* that this note has work in it
/// that matches; the list itself is one `noteit_read` away. Without a ceiling
/// here a single note with a thousand matching checkboxes would decide the size
/// of the whole answer, which is the opposite of what a context budget is for.
pub const MAX_CONTEXT_TASKS_PER_CANDIDATE: usize = 3;

/// Characters of one task's text.
///
/// A task is a *line* of a note, and the product already has a measure for how
/// much of a line to show somebody: [`MAX_LABEL_CHARS`]. The same number is
/// used here for the same reason, as its own constant so the two can part
/// company later without one silently dragging the other.
pub const MAX_CONTEXT_TASK_TEXT_CHARS: usize = MAX_LABEL_CHARS;

/// Characters of the matched occurrence published with a candidate.
///
/// `matched_text` is an excerpt of the note, so it is measured the way the
/// other excerpt is. It needs a ceiling of its own and not just the query's:
/// folding *drops* combining marks entirely, so `a` followed by fifty thousand
/// combining accents and then `b` folds to `ab` and matches a two-character
/// query — while the span in the source, which is what gets published, is the
/// whole fifty thousand. Measured, not reasoned about.
pub const MAX_CONTEXT_MATCHED_TEXT_CHARS: usize = MAX_SNIPPET_CHARS;

/// Warnings published with one answer.
///
/// Enough to characterise a damaged store — which notes, and what kind of
/// damage — without letting a store full of unreadable files decide how big a
/// context answer is. What is left out is counted, never hidden.
pub const MAX_CONTEXT_WARNINGS: usize = 20;

/// What a caller wants context about.
///
/// Deliberately not a path, a filename, a glob or a directory. There is no
/// field here that names a location, so there is nothing to validate and
/// nothing to escape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextRequest {
    /// Free text to look for. Empty means "no text signal".
    pub query: String,
    /// Tags and properties to look for. These are *signals*, not a filter:
    /// a note that carries one becomes a candidate and says so in its reasons.
    pub filter: NoteFilter,
    /// Whether matching tasks travel with the answer.
    pub include_tasks: bool,
    /// Candidate ceiling, clamped to `1..=MAX_CANDIDATES`.
    pub limit: Option<usize>,
}

impl ContextRequest {
    /// A request with nothing to go on but recency.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_query(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            ..Self::default()
        }
    }

    /// Whether anything here can pick one note over another.
    ///
    /// When nothing can, recency is all that is left, and the answer says so
    /// in every candidate rather than presenting an arbitrary order as
    /// relevance.
    fn has_discriminating_signal(&self) -> bool {
        !self.query.trim().is_empty() || !self.filter.is_empty()
    }

    fn ceiling(&self) -> usize {
        self.limit
            .unwrap_or(DEFAULT_CANDIDATES)
            .clamp(1, MAX_CANDIDATES)
    }
}

/// Why a note is in the answer.
///
/// A closed set of observations, never a score. `0.873` is not provenance, it
/// is decoration: nobody can audit it and nobody can act on it. Each variant
/// below is a fact about the note that a person can check by opening it.
///
/// The order is a contract, and [`Reason::PUBLISHED_ORDER`] is where it is
/// written down. The variants are declared in that same order so that `Ord`
/// agrees with it, but a test pins the sequence rather than trusting that
/// nobody will ever insert a variant in the middle: the shape of a published
/// answer must not be decided by where a hand happened to type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// The query text, whole, occurs in the note's visible text.
    ///
    /// Still the strongest lexical fact there is, and BM25 did not replace it:
    /// a note that contains the phrase somebody typed is a different kind of
    /// answer from one that contains its words scattered about.
    TextMatch,
    /// At least one normalized lexical query term occurs in the note's visible text.
    ///
    /// Can coexist with [`Reason::TextMatch`], [`Reason::SharedTag`],
    /// [`Reason::PropertyMatch`], [`Reason::TaskMatch`], and [`Reason::SemanticMatch`].
    /// Does not assert that the exact phrase does not occur.
    TermMatch,
    /// The note carries one of the tags asked about.
    SharedTag,
    /// The note carries one of the properties asked about.
    PropertyMatch,
    /// A task in the note matches the query.
    TaskMatch,
    /// The note was admitted by the semantic channel and passed provenance validation.
    ///
    /// Can coexist with [`Reason::TextMatch`], [`Reason::TermMatch`], and structured
    /// signals. Does not assert that query words are absent from the note.
    SemanticMatch,
    /// Nothing above could apply, and the note is recent. Only ever produced
    /// when the request had no discriminating signal at all.
    Recent,
}

impl Reason {
    /// Every reason, in the order a candidate publishes them.
    ///
    /// One list, used by the builder, by the tests and by the documentation, so
    /// that "the published order" is a thing that exists rather than an
    /// emergent property of the order the signal functions happen to run in.
    pub const PUBLISHED_ORDER: [Reason; 7] = [
        Self::TextMatch,
        Self::TermMatch,
        Self::SharedTag,
        Self::PropertyMatch,
        Self::TaskMatch,
        Self::SemanticMatch,
        Self::Recent,
    ];

    /// The stable wire name. An adapter publishes this, never the `Debug` form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TextMatch => "text_match",
            Self::TermMatch => "term_match",
            Self::SharedTag => "shared_tag",
            Self::PropertyMatch => "property_match",
            Self::TaskMatch => "task_match",
            Self::SemanticMatch => "semantic_match",
            Self::Recent => "recent",
        }
    }
}

/// Which precedence class a candidate belongs to.
///
/// The whole ranking policy 4.3A.R1.2 froze, in one type. It is **explicit**,
/// and that is the requirement: neither `reasons.len()` nor the ordinal of a
/// [`Reason`] variant decides where a candidate sits, because both would move
/// the day a reason is added, silently and everywhere.
///
/// ```text
/// 1  declared signals   TextMatch · SharedTag · PropertyMatch · TaskMatch
/// 2  terms              TermMatch
/// 3  semantics          SemanticMatch
/// 4  recency            Recent — exclusive: it only ever exists alone
/// ```
///
/// The answer is the classes concatenated, each ordered inside itself, with no
/// reordering between them. A candidate belongs to the **highest** class that
/// admitted it, appears once, and carries every applicable reason — so a note
/// that matched the phrase, carries the tag and is also semantically close is
/// one class-1 candidate with three reasons.
///
/// Why the four signals of 4.2 share a class instead of forming a queue: it was
/// measured against the real binary, and today `TextMatch` has no precedence
/// over `SharedTag` or `PropertyMatch` — a note with two of those outranks one
/// with `text_match` alone. Putting `TextMatch` above them would have changed
/// that, silently, and no audit asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CandidateClass {
    Declared,
    Terms,
    Semantic,
    Recency,
}

/// A note the answer could not read, as the answer publishes it.
///
/// A projection of [`ReadWarning`] rather than the thing itself, and the
/// difference is the point. The Core's message is written for whoever is
/// debugging a store, so it names the file — "Leitura recusada: o arquivo
/// `/home/.../notes/<uuid>.md` é um link simbólico". That sentence must not
/// leave through this surface: the contract is that a caller is given
/// `note_id` and never a path (`docs/second-brain.md` §19), and a free-form
/// diagnostic is exactly the crack a path slips through.
///
/// So nothing free-form travels. `kind` says what went wrong, `note_id` says
/// where, and both are fixed-size — which settles the length question by
/// construction rather than by a truncation rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextWarning {
    pub note_id: Option<Uuid>,
    pub kind: ReadWarningKind,
}

impl From<&ReadWarning> for ContextWarning {
    fn from(warning: &ReadWarning) -> Self {
        Self {
            note_id: warning.note_id,
            kind: warning.kind,
        }
    }
}

/// One task travelling beside a candidate.
///
/// Everything a caller needs to complete or reopen it, and nothing that names
/// a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextTask {
    pub note_id: Uuid,
    pub task_ref: String,
    pub text: String,
    pub checked: bool,
}

/// One note worth looking at, and why.
///
/// What is deliberately absent is as much of the contract as what is present:
/// no `revision`, no `base_revision`, no `etag`, no path, no filename, no
/// `mtime`, no score. The identity is `note_id` and the way to the content is
/// [`crate::NoteItCore::read_note`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub note_id: Uuid,
    /// Derived from the note's first visible line, exactly as everywhere else
    /// in Note-it. Never written to the file and never an identity.
    pub label: String,
    /// At most [`crate::search::MAX_SNIPPET_CHARS`] characters: the text
    /// around the match, or the note's opening when nothing matched.
    pub snippet: String,
    /// When the note's **text** last changed. Recency, not a version.
    pub updated_at: Option<DateTime<Utc>>,
    /// Every reason this note is here, in [`Reason`]'s declared order, without
    /// repeats.
    pub reasons: Vec<Reason>,
    /// The first occurrence as the note spells it, when the query matched.
    pub matched_text: Option<String>,
    /// Matching tasks, when the request asked for them. At most
    /// [`MAX_CONTEXT_TASKS_PER_CANDIDATE`], in the order they appear in the
    /// note.
    pub tasks: Vec<ContextTask>,
    /// Whether the task ceiling cut this candidate's list.
    ///
    /// Only ever true when tasks were asked for: a caller that did not ask for
    /// them was not truncated, it was answered.
    pub tasks_truncated: bool,
    /// How many matching tasks the ceiling left out of this candidate.
    pub omitted_task_count: usize,
}

/// The answer to one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextResult {
    pub candidates: Vec<Candidate>,
    /// Whether the ceiling cut the answer. Never silent — see D-14.
    pub truncated: bool,
    /// How many eligible candidates were left out by the ceiling.
    pub omitted_count: usize,
    /// Notes that could not be read, reported beside the ones that could. At
    /// most [`MAX_CONTEXT_WARNINGS`].
    pub warnings: Vec<ContextWarning>,
    /// Whether the warning ceiling cut the list.
    pub warnings_truncated: bool,
    /// How many warnings the ceiling left out.
    ///
    /// A damaged store still says how damaged it is: the ceiling limits what
    /// travels, never what is admitted to.
    pub omitted_warning_count: usize,
}

/// The status of the semantic channel for a retrieval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticStatus {
    /// The semantic channel was not attempted for this retrieval (e.g. on
    /// [`RetrievalMode::LexicalOnly`], an empty query, a request with only
    /// structured filters, or a query that folds to empty).
    NotRequested,
    /// The semantic channel was attempted and completed successfully.
    Succeeded,
    /// The semantic channel was attempted, failed, and the engine degraded
    /// to lexical retrieval under [`SemanticFallback::Automatic`].
    Unavailable,
}

/// The outcome of a retrieval where channel status is tracked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalOutcome {
    pub result: ContextResult,
    pub semantic_status: SemanticStatus,
}

/// Why a request produced no answer at all.
///
/// Distinct from an empty answer on purpose: "nothing matched" and "nothing
/// could be read" are different facts and a caller has to be able to tell them
/// apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextError {
    /// The query is longer than [`MAX_QUERY_CHARS`]. Refused rather than
    /// truncated, which is the rule [`crate::search::prepare_query`] already
    /// follows: answering a question nobody asked is worse than answering
    /// none.
    QueryTooLong { limit: usize, actual: usize },
    /// The store could not be scanned.
    ///
    /// Deliberately carries nothing. The Core's own message names the
    /// directory — "The notes path /home/…/notes is not a directory" — which is
    /// right for whoever is debugging a store and wrong for anything that
    /// leaves through this surface, where a caller is given `note_id` and never
    /// a path. The lesson from bounding the warnings applies again: a variant
    /// with no payload cannot leak, and no sanitiser has to be trusted to keep
    /// working.
    ///
    /// What is lost is a diagnostic, and only here: every other read path in
    /// the Core still returns the full message.
    StoreUnavailable,
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueryTooLong { limit, actual } => write!(
                formatter,
                "a consulta aceita no máximo {limit} caracteres, e esta tem {actual}"
            ),
            Self::StoreUnavailable => formatter.write_str("o store não pôde ser lido"),
        }
    }
}

impl std::error::Error for ContextError {}

/// How a retrieval may fail when the semantic channel is in play.
///
/// A second error type rather than a new variant on [`ContextError`], and the
/// reason is structural: the lexical path has nowhere to put a provider, so a
/// semantic failure cannot happen on it. Folding the two together would put a
/// case on `retrieve`'s signature that `retrieve` cannot produce, and every
/// caller — the MCP adapter included — would have to write an arm for something
/// that never arrives. The surface stays exactly as small as what can occur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalError {
    Context(ContextError),
    /// The semantic channel failed and the caller had asked for
    /// [`SemanticFallback::Required`].
    Semantic(SemanticError),
}

impl std::fmt::Display for RetrievalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Context(error) => error.fmt(formatter),
            Self::Semantic(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetrievalError {}

impl From<ContextError> for RetrievalError {
    fn from(error: ContextError) -> Self {
        Self::Context(error)
    }
}

/// Which channels one retrieval may use.
///
/// Two states, and the first carries nothing. That is the whole design: the
/// default is not "semantic, with the provider left unset" or "semantic, unless
/// a flag says otherwise" — it is a variant with no field a provider could go
/// in. The lexical default cannot accidentally reach a provider, because in
/// that state there is no provider to reach, and no configuration mistake, no
/// missing file and no failed initialisation can change that.
pub enum RetrievalMode<'a> {
    /// The declared signals and BM25. What production runs, and what
    /// [`retrieve`] uses.
    LexicalOnly,
    /// The above, plus the semantic channel. No caller in the shipped product
    /// constructs this: 4.3B builds the engine, and 4.3C decides how a real
    /// provider is configured and offered.
    Semantic(SemanticRuntime<'a>),
}

impl<'a> RetrievalMode<'a> {
    fn semantic(&self) -> Option<&SemanticRuntime<'a>> {
        match self {
            Self::LexicalOnly => None,
            Self::Semantic(runtime) => Some(runtime),
        }
    }
}

/// One note, as every signal about it will see it.
///
/// This is D-27 in a type. It is built from one authoritative read of a
/// [`NoteDocument`] and nothing may reach past it to the store, so a candidate
/// cannot be assembled out of two different versions of the same note. The
/// enumeration that found the note is not part of it: nothing the scan
/// observed is carried into a candidate. It is not a cache, not a
/// second source of truth and not persisted: it lives for the length of one
/// note's turn in one query and is dropped.
struct Projection {
    note_id: Uuid,
    label: String,
    content: String,
    /// The note as the reader sees it. Computed once here because six things
    /// need it — the label, the opening, the phrase search, the terms, the
    /// chunks and the evidence — and projecting the same note six times is work
    /// nobody asked for.
    visible: String,
    updated_at: Option<DateTime<Utc>>,
    tags: Vec<String>,
    properties: Vec<(String, String)>,
}

impl Projection {
    fn of(document: &NoteDocument) -> Self {
        let visible = visible_text(&document.content);
        let label = search::label_of_visible(&visible);
        Self {
            note_id: document.metadata.id,
            content: document.content.clone(),
            updated_at: document.metadata.updated_at,
            tags: document
                .user_metadata
                .tags
                .as_slice()
                .iter()
                .map(String::from)
                .collect(),
            properties: document
                .user_metadata
                .properties
                .as_slice()
                .iter()
                .map(|property| (property.key.clone(), property.value.clone()))
                .collect(),
            label,
            visible,
        }
    }

    /// The tasks in this note, parsed by the Core's own task reader.
    ///
    /// From `self.content`, like everything else here: a task list read from
    /// the store a second time could belong to a different version of the
    /// note, and that is the exact defect D-27 exists to prevent.
    fn tasks(&self) -> Vec<TaskEntry> {
        task::parse_tasks(self.note_id, &self.label, &self.content)
    }
}

/// A candidate before the corpus is closed and the order is known.
///
/// BM25 needs statistics over every readable note, and those are not final
/// until the last one has been read — so the score cannot be computed while the
/// candidate is being built. Everything that *can* be settled from one note's
/// own reading is settled there and then, and what is carried forward is the
/// small remainder: the counts, the class, and the two numbers the sort needs.
struct Prepared {
    candidate: Candidate,
    class: CandidateClass,
    /// How many **class-1** signals admitted it.
    ///
    /// Not `reasons.len()`, and the difference is the whole guarantee. Class 1
    /// is ordered by the rule the engine has always used — the number of
    /// declared signals — and if `TermMatch` were counted here, adding BM25
    /// would reshuffle candidates that existed before it. Classes 2 and 3 are
    /// additive precisely because this number cannot see them.
    declared: usize,
    /// What the note contributes to BM25, kept until the corpus is closed.
    terms: Option<DocumentTerms>,
    /// Filled in once the statistics are final. Internal, never published.
    score: f64,
    /// Cosine against the question, for class 3. Internal, never published.
    similarity: f64,
}

/// What the semantic channel established about one note, after its record was
/// checked against the note as it is now.
struct SemanticEvidence {
    similarity: f64,
    /// Where the winning chunk begins **in the current reading** — so the
    /// snippet is cut out of the note that exists, never out of the index.
    at: usize,
}

/// One note's reading, and everything derived from it.
struct Reading<'a> {
    projection: &'a Projection,
    /// The visible text folded, when there is a query to compare against.
    folded: Option<Folded>,
    /// The query's terms counted in this note, when there is a query.
    counted: Option<DocumentTerms>,
    semantic: Option<SemanticEvidence>,
}

/// One answer, plus whether the semantic channel had something to admit to.
struct Outcome {
    result: ContextResult,
    semantic_status: SemanticStatus,
    /// `None` on the lexical path, always: there is nothing there that can
    /// fail semantically.
    semantic_failure: Option<SemanticError>,
}

/// Retrieves context over the live notes.
///
/// Reads. Never writes: no file is created, no note is moved, no timestamp is
/// touched, no cache is built and no directory is made. The trash is not
/// scanned — a note somebody deleted must not come back as active memory
/// (D-15).
///
/// The whole store is walked, because `omitted_count` has to be true: a
/// ceiling that stopped the scan could not say how many candidates it did not
/// mention. That is the honest cost of having no index (D-04), and
/// `docs/second-brain.md` publishes what it measures.
///
/// This is the production path, and since 4.3B it means **the declared signals
/// plus BM25, with the semantic channel switched off by construction**. No
/// provider is consulted, because [`RetrievalMode::LexicalOnly`] has no field
/// one could be put in. Callers that want the semantic channel say so with
/// [`retrieve_with`].
pub fn retrieve(
    core: &NoteItCore,
    request: &ContextRequest,
) -> Result<ContextResult, ContextError> {
    // The second half of the outcome is `None` here by construction rather
    // than by luck, so it is dropped without a case to handle.
    Ok(run(core, request, RetrievalMode::LexicalOnly)?.result)
}

/// The same retrieval, with the channels the caller chose.
///
/// The semantic channel is not exposed to anybody in the shipped product: no
/// tool constructs one, no setting turns one on, and nothing downloads a model.
/// This exists so the engine can be tested against a provider that is not real,
/// which is the only way to tell an engine bug from a model bug later.
pub fn retrieve_with(
    core: &NoteItCore,
    request: &ContextRequest,
    mode: RetrievalMode<'_>,
) -> Result<RetrievalOutcome, RetrievalError> {
    let required = mode
        .semantic()
        .is_some_and(|runtime| runtime.fallback == SemanticFallback::Required);
    let outcome = run(core, request, mode)?;
    if let Some(error) = outcome.semantic_failure {
        if required {
            return Err(RetrievalError::Semantic(error));
        }
    }
    Ok(RetrievalOutcome {
        result: outcome.result,
        semantic_status: outcome.semantic_status,
    })
}

/// The pipeline, in the order 4.3A.R1.1 fixed.
///
/// ```text
///  1  validate the request
///  2  if the semantic channel is on and there is a question: embed it and ask
///     the index for PRELIMINARY hits — nothing from them is trusted yet
///  3  enumerate the live notes
///  4  read each one ONCE
///  5  from that same reading: the current revision, the projection, the
///     visible text, the exact signal, the structured signals, the tasks, the
///     terms, this note's BM25 contribution, and the verdict on whatever the
///     index claimed about this note
///  6  close the corpus statistics
///  7  score
///  8  build the reasons
///  9  assign the class
/// 10  order inside each class, then concatenate
/// 11  apply the ceilings
/// ```
///
/// Step 5 is the one with teeth. A vector is checked against the note as it is
/// **now**, from the reading the engine was going to do anyway, and a record
/// whose `source_revision` no longer matches is discarded and forgotten rather
/// than published. A stale result can cost a worse answer; it must never cost
/// an answer that presents old content as current.
fn run(
    core: &NoteItCore,
    request: &ContextRequest,
    mut mode: RetrievalMode<'_>,
) -> Result<Outcome, ContextError> {
    let query = prepare(&request.query)?;
    let ceiling = request.ceiling();
    let recency_only = !request.has_discriminating_signal();
    let terms = query.as_ref().map(QueryTerms::of).unwrap_or_default();

    // Step 2. Before any note is read, and only when there is something to
    // embed: an empty query has no meaning to look for, and a query that folds
    // away has no query. Neither is worth a provider call.
    let mut semantic_status = SemanticStatus::NotRequested;
    let mut semantic_failure = None;
    let mut claimed: BTreeMap<Uuid, SemanticHit> = BTreeMap::new();
    if let Some(runtime) = mode.semantic() {
        if query.is_some() {
            match runtime.preliminary_hits(request.query.trim()) {
                Ok(hits) => {
                    claimed = hits.into_iter().map(|hit| (hit.note_id, hit)).collect();
                    semantic_status = SemanticStatus::Succeeded;
                }
                Err(error) => {
                    semantic_failure = Some(error);
                    semantic_status = SemanticStatus::Unavailable;
                }
            }
        }
    }

    let (ids, mut warnings) = core
        .storage()
        .list_notes_by_recency_with_warnings()
        // The message is dropped here on purpose, and this is the only place
        // it could have entered the answer.
        .map_err(|_| ContextError::StoreUnavailable)?;

    let mut statistics = CorpusStatistics::for_query(&terms);
    let mut prepared: Vec<Prepared> = Vec::new();
    let mut enumerated: BTreeSet<Uuid> = BTreeSet::new();
    let mut forget: Vec<Uuid> = Vec::new();

    for id in ids {
        enumerated.insert(id);
        // The authoritative read. The scan above already looked at each note's
        // header to order the identifiers, and deliberately none of what it saw
        // is used below: everything about this candidate comes from what this
        // call returned, and the document is dropped before the next note.
        let document = match core.read_note(&id) {
            Ok(document) => document,
            Err(message) => {
                // A note that could not be read coherently produces a warning
                // and never a half-filled candidate: partial provenance is
                // worse than an acknowledged gap. It is not a BM25 document
                // either — counting it as an empty one would quietly shorten
                // every other note's length normalisation.
                warnings.push(ReadWarning {
                    note_id: Some(id),
                    kind: ReadWarningKind::UnreadableNote,
                    message,
                });
                continue;
            }
        };
        let projection = Projection::of(&document);

        let folded = query.as_ref().map(|_| search::fold(&projection.visible));
        let counted = folded.as_ref().map(|folded| terms.count_in(&folded.text));
        if let Some(counted) = &counted {
            // Every readable live note is a document of the corpus, candidate
            // or not: `N`, `df` and `avgdl` describe the store, not the answer.
            statistics.observe(counted);
        }

        // Provenance, from this same reading, before anything of this note is
        // published.
        let semantic = match claimed.get(&id) {
            None => None,
            Some(hit) => match verify(&document, &projection, hit) {
                Some(evidence) => Some(evidence),
                None => {
                    forget.push(id);
                    None
                }
            },
        };

        let reading = Reading {
            projection: &projection,
            folded,
            counted,
            semantic,
        };
        if let Some(candidate) = consider(&reading, request, query.as_ref(), &terms, recency_only) {
            prepared.push(candidate);
        }
    }

    // A record naming a note the live scan never mentioned: deleted, trashed,
    // or never there. It must not resurrect anything, and it must not survive
    // to be asked again.
    for note_id in claimed.keys() {
        if !enumerated.contains(note_id) {
            forget.push(*note_id);
        }
    }
    if let RetrievalMode::Semantic(runtime) = &mut mode {
        for note_id in &forget {
            runtime.index.invalidate_note(note_id);
        }
    }

    // Steps 6 and 7. The corpus is only complete now, so this is the earliest
    // moment a BM25 score can exist.
    for candidate in &mut prepared {
        if let Some(terms) = &candidate.terms {
            candidate.score = statistics.score(terms);
        }
    }

    prepared.sort_by(order);

    // The semantic ceiling is **admission policy**, not truncation: a
    // nearest-neighbour search always has a nearest neighbour, and a fourth
    // stranger was never a candidate. It is applied before the answer's own
    // ceiling so that `omitted_count` keeps meaning exactly one thing — how
    // many eligible candidates the caller's limit left out.
    let max_semantic_only = mode
        .semantic()
        .map_or(0, |runtime| runtime.policy.max_semantic_only);
    let mut semantic_only = 0;
    prepared.retain(|candidate| {
        if candidate.class != CandidateClass::Semantic {
            return true;
        }
        semantic_only += 1;
        semantic_only <= max_semantic_only
    });

    let omitted_count = prepared.len().saturating_sub(ceiling);
    prepared.truncate(ceiling);

    // The warnings keep the order the scan produced — notes by recency, then
    // whatever the scan itself reported — which is already deterministic, so
    // the ones that survive the ceiling are the same ones every time. Sorting
    // them by message would be ordering by a sentence written for a person.
    let omitted_warning_count = warnings.len().saturating_sub(MAX_CONTEXT_WARNINGS);
    warnings.truncate(MAX_CONTEXT_WARNINGS);

    Ok(Outcome {
        result: ContextResult {
            candidates: prepared
                .into_iter()
                .map(|candidate| candidate.candidate)
                .collect(),
            truncated: omitted_count > 0,
            omitted_count,
            warnings: warnings.iter().map(ContextWarning::from).collect(),
            warnings_truncated: omitted_warning_count > 0,
            omitted_warning_count,
        },
        semantic_status,
        semantic_failure,
    })
}

/// Whether what the index claimed about a note is still true of the note.
///
/// The check needs the document, and 4.3A.R1 corrected the specification after
/// reading the code rather than assuming: `NoteRevision::for_document` is the
/// only authoritative way to a revision in this crate, and it serialises the
/// whole canonical document. The scan's `NoteSummary` has no revision, and
/// `updated_at` is no substitute — it moves with the text and stays put when a
/// tag, a property or a colour changes, which is exactly the class of edit the
/// revision exists to catch.
///
/// The read costs nothing extra, because the engine already does exactly one
/// per candidate for D-27. What it is not, is free of the read — and this
/// comment will not claim otherwise.
fn verify(
    document: &NoteDocument,
    projection: &Projection,
    hit: &SemanticHit,
) -> Option<SemanticEvidence> {
    if hit.chunker_version != CHUNKER_VERSION {
        return None;
    }
    let current = NoteRevision::for_document(document).ok()?;
    if current != hit.source_revision {
        return None;
    }
    // The revision matched, so cutting the note again gives back the very
    // chunks the record was made from — which is how the winning one is found
    // without the index ever having stored a line of the note's text.
    for piece in chunk(&projection.visible) {
        let id = ChunkId::of(
            &projection.note_id,
            &current,
            piece.ordinal,
            CHUNKER_VERSION,
            &piece.text,
        )
        .ok()?;
        if id == hit.chunk_id {
            return Some(SemanticEvidence {
                similarity: hit.similarity,
                at: piece.at,
            });
        }
    }
    None
}

/// Cuts text to a character ceiling, and says so where it cut.
///
/// Characters and never bytes: `chars().nth()` lands on a boundary by
/// construction, so a slice here cannot split one. The ellipsis is the same
/// convention a label already uses, which makes the cut visible to whoever
/// reads the text rather than something they have to be told about.
fn clip(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        None => text.to_string(),
        Some((cut, _)) => format!("{}…", &text[..cut]),
    }
}

/// The query as it will be compared, or a refusal.
///
/// `None` is "no text signal", which is not an error: a request may carry only
/// tags, or nothing at all.
fn prepare(query: &str) -> Result<Option<Folded>, ContextError> {
    let actual = query.chars().count();
    if actual > MAX_QUERY_CHARS {
        return Err(ContextError::QueryTooLong {
            limit: MAX_QUERY_CHARS,
            actual,
        });
    }
    Ok(search::prepare_query(query))
}

/// Decides whether one note is a candidate, using only its own reading.
///
/// Takes a [`Reading`] and nothing else that could reach the store. Every field
/// of the candidate it returns comes from that one reading, which is what makes
/// D-27 structural rather than a comment asking for care.
///
/// The reasons are pushed in [`Reason::PUBLISHED_ORDER`] by walking the signals
/// in that order, so the published sequence is a consequence of the code's
/// shape rather than of a sort at the end.
fn consider(
    reading: &Reading<'_>,
    request: &ContextRequest,
    query: Option<&Folded>,
    terms: &QueryTerms,
    recency_only: bool,
) -> Option<Prepared> {
    let projection = reading.projection;
    let mut reasons = Vec::new();
    let mut declared = 0;
    let mut snippet = None;
    let mut matched_text = None;
    let mut tasks = Vec::new();
    let mut omitted_task_count = 0;

    // 1. The phrase, whole. BM25 did not replace this and must not: a note that
    //    contains what somebody typed, spelled as they typed it, is a stronger
    //    answer than one that contains the same words apart.
    let exact = match (query, reading.folded.as_ref()) {
        (Some(query), Some(folded)) => {
            search::search_visible(query, projection.note_id, &projection.visible, folded)
        }
        _ => None,
    };
    if let Some(hit) = &exact {
        reasons.push(Reason::TextMatch);
        declared += 1;
        matched_text = Some(clip(&hit.matched_text, MAX_CONTEXT_MATCHED_TEXT_CHARS));
        snippet = Some(hit.snippet.clone());
    }

    // 2. The terms. A reason on its own, never a stronger `TextMatch`.
    let term_match = reading.counted.as_ref().is_some_and(DocumentTerms::matched);
    if term_match {
        reasons.push(Reason::TermMatch);
    }

    // 3 and 4. The structured signals, which admit with or without a query.
    if shares_tag(projection, &request.filter) {
        reasons.push(Reason::SharedTag);
        declared += 1;
    }
    if shares_property(projection, &request.filter) {
        reasons.push(Reason::PropertyMatch);
        declared += 1;
    }

    // 5. Tasks are read from the same projection, so a task can never describe
    //    a version of the note the snippet beside it does not.
    if let Some(query) = query {
        let matching: Vec<TaskEntry> = projection
            .tasks()
            .into_iter()
            .filter(|entry| search::fold(&entry.text).text.contains(&query.text))
            .collect();
        if !matching.is_empty() {
            reasons.push(Reason::TaskMatch);
            declared += 1;
            if request.include_tasks {
                // Counted from the set already derived from this projection,
                // never by reading the note again: the number of tasks left out
                // must not cost the coherence the candidate is built on.
                omitted_task_count = matching
                    .len()
                    .saturating_sub(MAX_CONTEXT_TASKS_PER_CANDIDATE);
                tasks = matching
                    .into_iter()
                    // The note's own order, which is the order somebody reading
                    // the Markdown sees. Keeping the first few is the only cut
                    // that needs no rule of its own to explain.
                    .take(MAX_CONTEXT_TASKS_PER_CANDIDATE)
                    .map(|entry| ContextTask {
                        note_id: entry.note_id,
                        task_ref: entry.task_ref.as_str().to_string(),
                        text: clip(&entry.text, MAX_CONTEXT_TASK_TEXT_CHARS),
                        checked: entry.checked,
                    })
                    .collect();
            }
        }
    }

    // 6. The semantic channel, already checked against this reading.
    if reading.semantic.is_some() {
        reasons.push(Reason::SemanticMatch);
    }

    // 7. Recency is a last resort and is labelled as one. It is never mixed
    //    with a factual signal, because "this note is recent" adds nothing to
    //    "this note contains what you asked for".
    if reasons.is_empty() {
        if !recency_only {
            return None;
        }
        reasons.push(Reason::Recent);
    }

    // The class is the highest channel that admitted it, decided from the
    // signals themselves. Every possible candidate lands in exactly one: the
    // four branches are exhaustive because `reasons` is not empty by here.
    let class = if declared > 0 {
        CandidateClass::Declared
    } else if term_match {
        CandidateClass::Terms
    } else if reading.semantic.is_some() {
        CandidateClass::Semantic
    } else {
        CandidateClass::Recency
    };

    // The evidence, in the order the specification fixes. A phrase match brings
    // its own; failing that, the first query term that occurs, at its first
    // occurrence; failing that, and only for a candidate the semantic channel
    // admitted on its own, the winning chunk — rebuilt from the reading, never
    // taken from the index; failing all of it, the note's opening, which is
    // what a candidate admitted by a tag or by recency has always shown.
    if snippet.is_none() && term_match {
        if let (Some(folded), Some(counted)) = (reading.folded.as_ref(), reading.counted.as_ref()) {
            if let Some((at, occurrence)) =
                first_term_occurrence(terms, counted, folded, &projection.visible)
            {
                snippet = Some(search::snippet_around(&projection.visible, at));
                matched_text = Some(clip(&occurrence, MAX_CONTEXT_MATCHED_TEXT_CHARS));
            }
        }
    }
    if snippet.is_none() && class == CandidateClass::Semantic {
        if let Some(evidence) = &reading.semantic {
            snippet = Some(search::snippet_around(&projection.visible, evidence.at));
        }
    }

    Some(Prepared {
        candidate: Candidate {
            note_id: projection.note_id,
            label: projection.label.clone(),
            snippet: snippet.unwrap_or_else(|| search::snippet_around(&projection.visible, 0)),
            updated_at: projection.updated_at,
            reasons,
            // Left as `None` for a candidate nothing lexical matched — a purely
            // semantic one included. There is no substring of that note anybody
            // could honestly call the matched text, and inventing one would be
            // the first small lie in a chain that ends with an agent editing a
            // passage it was told matched.
            matched_text,
            tasks,
            tasks_truncated: omitted_task_count > 0,
            omitted_task_count,
        },
        class,
        declared,
        terms: reading.counted.clone(),
        score: 0.0,
        similarity: reading
            .semantic
            .as_ref()
            .map_or(0.0, |evidence| evidence.similarity),
    })
}

/// Where to point a snippet when the phrase did not match but a term did.
///
/// Deterministic twice over: the **first query term that occurs**, in the order
/// the query was typed rather than in the order the note happens to mention
/// them, and then its **first occurrence** in the note. Neither half depends on
/// a score, so the same question always shows the same evidence.
///
/// The returned span is in the note's own spelling — the fold is only how the
/// occurrence was found, never what is published.
fn first_term_occurrence(
    terms: &QueryTerms,
    counted: &DocumentTerms,
    folded: &Folded,
    visible: &str,
) -> Option<(usize, String)> {
    let wanted = terms
        .as_slice()
        .iter()
        .enumerate()
        .find(|(index, _)| counted.frequency(*index) > 0)
        .map(|(_, term)| term)?;
    let token = crate::lexical::terms(&folded.text)
        .into_iter()
        .find(|term| term.text == wanted)?;
    let from = search::source_offset_of(visible, token.at);
    let to = search::source_offset_of(visible, token.at + token.text.len());
    Some((from, visible[from..to.max(from)].to_string()))
}

/// Whether the note carries any tag the request asked about.
///
/// Compared by [`semantic_identity`], the same folding the rest of the product
/// uses, so `Medicina` and `medicina` are one tag here exactly as they are in
/// the palette and on the command line.
fn shares_tag(projection: &Projection, filter: &NoteFilter) -> bool {
    filter.tags.iter().any(|wanted| {
        let wanted = semantic_identity(wanted.trim());
        !wanted.is_empty()
            && projection
                .tags
                .iter()
                .any(|tag| semantic_identity(tag) == wanted)
    })
}

/// Whether the note carries any property the request asked about.
///
/// An empty asked-for value matches the key alone, which is how "does this
/// note have a `status` at all" is asked.
fn shares_property(projection: &Projection, filter: &NoteFilter) -> bool {
    filter.properties.iter().any(|(key, value)| {
        let wanted_key = semantic_identity(key.trim());
        if wanted_key.is_empty() {
            return false;
        }
        let wanted_value = semantic_identity(value.trim());
        projection.properties.iter().any(|(have_key, have_value)| {
            semantic_identity(have_key) == wanted_key
                && (wanted_value.is_empty() || semantic_identity(have_value) == wanted_value)
        })
    })
}

/// The published order, and it is total.
///
/// The class comes first and nothing crosses it. Inside a class:
///
/// | class | 1st | 2nd | 3rd | 4th |
/// | --- | --- | --- | --- | --- |
/// | declared | more declared signals | more recent, absent last | `note_id` | — |
/// | terms | BM25 descending | more reasons | more recent | `note_id` |
/// | semantic | similarity descending | more recent | `note_id` | — |
/// | recency | more recent | `note_id` | — | — |
///
/// Class 1 and class 4 are the rule this function has always had, unchanged, so
/// a candidate that existed before BM25 sits exactly where it sat. `note_id`
/// closes all four: without it two notes written in the same second would fall
/// back on the order the filesystem happened to hand over, and the same
/// question would answer differently on the same store. It is stability, not a
/// hidden score.
///
/// The floats are compared with `total_cmp` rather than
/// `partial_cmp().unwrap()`. The second panics on a `NaN` that the types here
/// make impossible — which is exactly the kind of guarantee that stops being
/// true one refactor later, at which point a retrieval would abort a process.
fn order(left: &Prepared, right: &Prepared) -> Ordering {
    left.class
        .cmp(&right.class)
        .then_with(|| match left.class {
            CandidateClass::Declared => right.declared.cmp(&left.declared),
            CandidateClass::Terms => right.score.total_cmp(&left.score).then_with(|| {
                right
                    .candidate
                    .reasons
                    .len()
                    .cmp(&left.candidate.reasons.len())
            }),
            CandidateClass::Semantic => right.similarity.total_cmp(&left.similarity),
            CandidateClass::Recency => Ordering::Equal,
        })
        .then_with(|| by_recency(left, right))
        .then_with(|| left.candidate.note_id.cmp(&right.candidate.note_id))
}

/// More recently written first, and a note with no `updated_at` after every
/// note that has one.
fn by_recency(left: &Prepared, right: &Prepared) -> Ordering {
    match (left.candidate.updated_at, right.candidate.updated_at) {
        (Some(left), Some(right)) => right.cmp(&left),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published order is a list, and the list is the contract.
    ///
    /// Checked against `Ord` as well, because the rest of the engine relies on
    /// the two agreeing — the "no repeats, in order" assertion the integration
    /// tests make on every candidate is a comparison, not a lookup.
    #[test]
    fn the_published_order_is_declared_and_not_inferred() {
        assert_eq!(
            Reason::PUBLISHED_ORDER,
            [
                Reason::TextMatch,
                Reason::TermMatch,
                Reason::SharedTag,
                Reason::PropertyMatch,
                Reason::TaskMatch,
                Reason::SemanticMatch,
                Reason::Recent,
            ]
        );
        assert!(Reason::PUBLISHED_ORDER
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
    }

    /// Every variant is in the list, and every variant has a wire name.
    ///
    /// The `match` is what does the work: adding a reason without deciding
    /// where it is published stops compiling here.
    #[test]
    fn no_reason_is_left_out_of_the_published_order() {
        for reason in Reason::PUBLISHED_ORDER {
            let expected = match reason {
                Reason::TextMatch => "text_match",
                Reason::TermMatch => "term_match",
                Reason::SharedTag => "shared_tag",
                Reason::PropertyMatch => "property_match",
                Reason::TaskMatch => "task_match",
                Reason::SemanticMatch => "semantic_match",
                Reason::Recent => "recent",
            };
            assert_eq!(reason.as_str(), expected);
        }
        assert_eq!(
            Reason::PUBLISHED_ORDER.len(),
            7,
            "a new reason needs a place in the published order, not just a name"
        );
    }

    /// Every class is ordered by something, and the classes are ordered against
    /// each other in the sequence 4.3A.R1.2 froze.
    #[test]
    fn the_classes_are_a_queue_and_nothing_crosses_it() {
        assert!(CandidateClass::Declared < CandidateClass::Terms);
        assert!(CandidateClass::Terms < CandidateClass::Semantic);
        assert!(CandidateClass::Semantic < CandidateClass::Recency);
    }
}
