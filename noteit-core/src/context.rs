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
//! [`Projection`], built from one [`NoteDocument`] read once. That is D-27,
//! and it is a property rather than a preference: a candidate assembled from a
//! snippet read before an edit and tags read after it is not a note that ever
//! existed, and provenance about a note that never existed is a lie. The type
//! is what enforces it — every signal function below takes `&Projection` and
//! there is no path to the store from any of them.
//!
//! It is coherence *per note*, not across the store. Two candidates may come
//! from two different instants, and that costs nothing: no lease is taken, no
//! snapshot is held, no lock is acquired, and nothing here writes.

use crate::filter::NoteFilter;
use crate::metadata::semantic_identity;
use crate::model::NoteDocument;
use crate::search::{self, Folded, MAX_QUERY_CHARS};
use crate::task::{self, TaskEntry};
use crate::warning::{ReadBatch, ReadWarning, ReadWarningKind};
use crate::NoteItCore;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Candidates returned when the caller does not say. Half the reading API's
/// `unwrap_or(20)`: an opening context should be narrow, and widening it is a
/// decision the caller makes with a number.
pub const DEFAULT_CANDIDATES: usize = 10;

/// The most candidates any request can produce. Half the Core's `MAX_RESULTS`.
/// Fifty snippets of 240 characters is about 12 KB — a real slice of a context
/// window, and still small enough for a person to read while debugging.
pub const MAX_CANDIDATES: usize = 50;

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
/// The declared order is also the published order — see [`Candidate::reasons`]
/// — so the same note always explains itself the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// The query text occurs in the note's visible text.
    TextMatch,
    /// The note carries one of the tags asked about.
    SharedTag,
    /// The note carries one of the properties asked about.
    PropertyMatch,
    /// A task in the note matches the query.
    TaskMatch,
    /// Nothing above could apply, and the note is recent. Only ever produced
    /// when the request had no discriminating signal at all.
    Recent,
}

impl Reason {
    /// The stable wire name. An adapter publishes this, never the `Debug` form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TextMatch => "text_match",
            Self::SharedTag => "shared_tag",
            Self::PropertyMatch => "property_match",
            Self::TaskMatch => "task_match",
            Self::Recent => "recent",
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
    /// Matching tasks, when the request asked for them.
    pub tasks: Vec<ContextTask>,
}

/// The answer to one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextResult {
    pub candidates: Vec<Candidate>,
    /// Whether the ceiling cut the answer. Never silent — see D-14.
    pub truncated: bool,
    /// How many eligible candidates were left out by the ceiling.
    pub omitted_count: usize,
    /// Notes that could not be read, reported beside the ones that could.
    pub warnings: Vec<ReadWarning>,
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
    StoreUnavailable(String),
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueryTooLong { limit, actual } => write!(
                formatter,
                "a consulta aceita no máximo {limit} caracteres, e esta tem {actual}"
            ),
            Self::StoreUnavailable(detail) => {
                write!(formatter, "o store não pôde ser lido: {detail}")
            }
        }
    }
}

impl std::error::Error for ContextError {}

/// One note, read once, as every signal about it will see it.
///
/// This is D-27 in a type. It is built from a single [`NoteDocument`] and
/// nothing may reach past it to the store, so a candidate cannot be assembled
/// out of two different versions of the same note. It is not a cache, not a
/// second source of truth and not persisted: it lives for the length of one
/// note's turn in one query and is dropped.
struct Projection {
    note_id: Uuid,
    label: String,
    content: String,
    updated_at: Option<DateTime<Utc>>,
    tags: Vec<String>,
    properties: Vec<(String, String)>,
}

impl Projection {
    fn of(document: &NoteDocument) -> Self {
        let label = search::label_for(&document.content);
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
pub fn retrieve(
    core: &NoteItCore,
    request: &ContextRequest,
) -> Result<ContextResult, ContextError> {
    let query = prepare(&request.query)?;
    let ceiling = request.ceiling();
    let recency_only = !request.has_discriminating_signal();

    let (ids, mut warnings) = core
        .storage()
        .list_notes_by_recency_with_warnings()
        .map_err(ContextError::StoreUnavailable)?;

    let mut candidates = Vec::new();
    for id in ids {
        // One read. Everything about this candidate comes from what it
        // returned, and the document is dropped before the next note.
        let document = match core.read_note(&id) {
            Ok(document) => document,
            Err(message) => {
                // A note that could not be read coherently produces a warning
                // and never a half-filled candidate: partial provenance is
                // worse than an acknowledged gap.
                warnings.push(ReadWarning {
                    note_id: Some(id),
                    kind: ReadWarningKind::UnreadableNote,
                    message,
                });
                continue;
            }
        };
        let projection = Projection::of(&document);
        if let Some(candidate) = consider(&projection, request, query.as_ref(), recency_only) {
            candidates.push(candidate);
        }
    }

    candidates.sort_by(order);

    let omitted_count = candidates.len().saturating_sub(ceiling);
    candidates.truncate(ceiling);

    Ok(ContextResult {
        candidates,
        truncated: omitted_count > 0,
        omitted_count,
        warnings,
    })
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

/// Decides whether one note is a candidate, using only its projection.
///
/// Takes `&Projection` and nothing else that could reach the store. Every
/// field of the candidate it returns comes from that one projection, which is
/// what makes D-27 structural rather than a comment asking for care.
fn consider(
    projection: &Projection,
    request: &ContextRequest,
    query: Option<&Folded>,
    recency_only: bool,
) -> Option<Candidate> {
    let mut reasons = Vec::new();
    let mut snippet = None;
    let mut matched_text = None;
    let mut tasks = Vec::new();

    if let Some(query) = query {
        if let Some(hit) = search::search_note(query, projection.note_id, &projection.content) {
            reasons.push(Reason::TextMatch);
            matched_text = Some(hit.matched_text);
            snippet = Some(hit.snippet);
        }
    }

    if shares_tag(projection, &request.filter) {
        reasons.push(Reason::SharedTag);
    }
    if shares_property(projection, &request.filter) {
        reasons.push(Reason::PropertyMatch);
    }

    // Tasks are read from the same projection, so a task can never describe a
    // version of the note the snippet beside it does not.
    if let Some(query) = query {
        let matching: Vec<TaskEntry> = projection
            .tasks()
            .into_iter()
            .filter(|entry| search::fold(&entry.text).text.contains(&query.text))
            .collect();
        if !matching.is_empty() {
            reasons.push(Reason::TaskMatch);
            if request.include_tasks {
                tasks = matching
                    .into_iter()
                    .map(|entry| ContextTask {
                        note_id: entry.note_id,
                        task_ref: entry.task_ref.as_str().to_string(),
                        text: entry.text,
                        checked: entry.checked,
                    })
                    .collect();
            }
        }
    }

    if reasons.is_empty() {
        // Recency is a last resort and is labelled as one. It is never mixed
        // with a factual signal, because "this note is recent" adds nothing to
        // "this note contains what you asked for".
        if !recency_only {
            return None;
        }
        reasons.push(Reason::Recent);
    }

    Some(Candidate {
        note_id: projection.note_id,
        label: projection.label.clone(),
        snippet: snippet.unwrap_or_else(|| search::opening_of(&projection.content)),
        updated_at: projection.updated_at,
        reasons,
        matched_text,
        tasks,
    })
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
/// 1. more distinct reasons first — a note that matched the text *and* carries
///    the tag is a better answer than one that only did either;
/// 2. then more recently written first, and a note with no `updated_at` after
///    every note that has one;
/// 3. then by `note_id`.
///
/// The third rule is not a tie-break nobody will hit: two notes written in the
/// same second, or two notes with no timestamp at all, would otherwise fall
/// back on the order the filesystem happened to hand them, and the same
/// question would answer differently on the same store. It is stability, not a
/// hidden score.
fn order(left: &Candidate, right: &Candidate) -> std::cmp::Ordering {
    right
        .reasons
        .len()
        .cmp(&left.reasons.len())
        .then_with(|| match (left.updated_at, right.updated_at) {
            (Some(left), Some(right)) => right.cmp(&left),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
        .then_with(|| left.note_id.cmp(&right.note_id))
}

/// The same answer, as a [`ReadBatch`], for a caller that already speaks that
/// shape.
impl ContextResult {
    pub fn into_batch(self) -> ReadBatch<Candidate> {
        ReadBatch::new(self.candidates, self.warnings)
    }
}
