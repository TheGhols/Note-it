//! The lifecycle of the semantic channel, for one server process.
//!
//! Three things live longer than a request and so cannot be built inside one:
//! the provider, the index, and the settings that say whether either should
//! exist. This module is where they live, and the shape of it is the answer to
//! four questions the specification asks:
//!
//! * **the model is loaded once.** Reading and verifying the artifact costs
//!   seconds; doing it per query would make the feature unusable and would say
//!   nothing new each time;
//! * **the index is reused.** It is derived from the notes and rebuilt from
//!   them, never from a file, so losing it costs time and never information;
//! * **one indexing per process at a time.** The mutex here is that rule.
//!   Two concurrent questions about an unindexed store cannot build two
//!   indexes: the second waits and then finds the first one's work;
//! * **a note that changed is reindexed, and only that note.** How that is
//!   noticed is the interesting part, and it is written up on
//!   [`SemanticSession::synchronise`].
//!
//! Nothing here reaches the network, and nothing here can: the provider is
//! `noteit-embedding-local`, whose whole dependency graph is checked by
//! `scripts/check-embedding-boundary`.

use noteit_core::context::{self as engine, RetrievalMode, SemanticStatus};
use noteit_core::semantic::{
    index_document, EmbeddingProvider, InMemoryIndex, SemanticFallback, SemanticIndex,
    SemanticRuntime,
};
use noteit_core::settings::{SemanticFallbackPolicy, SemanticRetrievalConfig};
use noteit_core::{NoteItCore, Uuid};
use noteit_embedding_local::{ArtifactError, ArtifactExpectation, LocalProvider};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// What one retrieval got out of the semantic channel.
pub enum Retrieved {
    /// An answer, and what the semantic channel contributed to it.
    Answer(engine::ContextResult, SemanticStatus),
    /// The request itself could not be served.
    Refused(engine::ContextError),
    /// The caller asked for the semantic channel and did not get it.
    SemanticRequired,
}

/// Whether a provider was ever asked for, and what came back.
enum Loaded {
    /// Not attempted. The factory default never leaves this state.
    Never,
    Ready(Box<LocalProvider>),
    /// Attempted and refused. Remembered rather than retried: the artifact is
    /// half a gigabyte and re-reading it on every query to be told the same
    /// thing again would cost seconds per question. Provisioning a model is an
    /// explicit act, and so is restarting the server after it.
    Failed(ArtifactError),
}

/// The provider, the index, and how much of the store is in it.
struct SemanticState {
    provider: Loaded,
    index: Option<InMemoryIndex>,
    indexed_at: Option<SystemTime>,
    vectors: usize,
}

impl SemanticState {
    fn new() -> Self {
        Self {
            provider: Loaded::Never,
            index: None,
            indexed_at: None,
            vectors: 0,
        }
    }
}

/// Which artifact a session's provider is built from.
///
/// Two variants and not one, because 4.3C ships exactly one model and a test
/// needs a table it can build in a millisecond. It is a parameter rather than
/// an environment variable on purpose: an override that any process could set
/// would be a way to point the provider at somebody else's weights, and the
/// product passes [`ArtifactSource::Pinned`] from every constructor there is.
#[derive(Clone)]
pub enum ArtifactSource {
    /// The artifact this build pins, in the XDG location.
    Pinned,
    /// A named directory, and what is expected in it.
    At {
        directory: PathBuf,
        expectation: ArtifactExpectation,
    },
}

impl ArtifactSource {
    fn load(&self) -> Result<LocalProvider, ArtifactError> {
        match self {
            Self::Pinned => LocalProvider::load_default(),
            Self::At {
                directory,
                expectation,
            } => LocalProvider::load(directory, expectation),
        }
    }

    fn present(&self) -> bool {
        let (directory, _) = match self {
            Self::Pinned => (
                match noteit_embedding_local::artifact_directory(
                    &noteit_embedding_local::POTION_MULTILINGUAL_128M,
                ) {
                    Some(directory) => directory,
                    None => return false,
                },
                (),
            ),
            Self::At { directory, .. } => (directory.clone(), ()),
        };
        let (weights, tokenizer) = noteit_embedding_local::artifact::artifact_files(&directory);
        weights.is_file() && tokenizer.is_file()
    }

    fn model(&self) -> &str {
        match self {
            Self::Pinned => noteit_embedding_local::POTION_MULTILINGUAL_128M.model,
            Self::At { expectation, .. } => expectation.model,
        }
    }
}

/// A handle on that state, cheap to clone and shared by every request.
#[derive(Clone)]
pub struct SemanticSession {
    settings: SemanticRetrievalConfig,
    source: ArtifactSource,
    state: Arc<Mutex<SemanticState>>,
}

/// What a diagnostic surface may say about the channel.
///
/// Deliberately small, and deliberately without a path, a digest or a vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticReport {
    pub enabled: bool,
    pub provider: &'static str,
    pub model: String,
    pub local: bool,
    pub artifact_available: bool,
    /// Why a provider could not be built, when one was tried and refused.
    ///
    /// A closed enum of Note-it's own — never a library's sentence and never a
    /// path — so it is safe for a local diagnostic and for a test to assert on.
    pub artifact_error: Option<ArtifactError>,
    pub indexed_notes: Option<usize>,
    pub indexed_vectors: Option<usize>,
    pub last_indexed: Option<SystemTime>,
}

impl SemanticState {
    /// Makes sure a provider has been asked for exactly once.
    fn ensure_provider(&mut self, source: &ArtifactSource) {
        if matches!(self.provider, Loaded::Never) {
            self.provider = match source.load() {
                Ok(provider) => Loaded::Ready(Box::new(provider)),
                Err(error) => Loaded::Failed(error),
            };
        }
    }

    fn ready(&self) -> bool {
        matches!(self.provider, Loaded::Ready(_))
    }

    /// Syncs the index and runs one retrieval.
    ///
    /// Also reports whether the engine *forgot* anything while it ran. It
    /// forgets exactly the records whose `source_revision` no longer matches
    /// the note — so a drop means "something was edited since it was indexed",
    /// which the caller turns into one more pass rather than a query that
    /// lags behind the user's own edit.
    fn sync_and_run(
        &mut self,
        core: &NoteItCore,
        request: &engine::ContextRequest,
        fallback: SemanticFallback,
    ) -> Result<(engine::RetrievalOutcome, bool), engine::RetrievalError> {
        let Loaded::Ready(provider) = &self.provider else {
            unreachable!("callers check readiness before reaching here")
        };

        // An index belongs to one space. A provider whose artifact changed is a
        // different space, and the old index is dropped rather than
        // reinterpreted — reinterpreting is the failure §5 measured.
        let space = provider.space();
        let stale_space = self
            .index
            .as_ref()
            .is_none_or(|index| *SemanticIndex::space(index) != space);
        if stale_space {
            self.index = Some(InMemoryIndex::new(space));
            self.indexed_at = None;
        }
        let index = self.index.as_mut().expect("an index was just ensured");

        synchronise(core, provider, index).map_err(engine::RetrievalError::Context)?;
        let before = SemanticIndex::vector_count(index);
        self.vectors = before;
        self.indexed_at = Some(SystemTime::now());

        let runtime = SemanticRuntime::new(provider.as_ref(), index).with_fallback(fallback);
        let outcome = engine::retrieve_with(core, request, RetrievalMode::Semantic(runtime))?;

        let index = self.index.as_ref().expect("the index is still there");
        let after = SemanticIndex::vector_count(index);
        self.vectors = after;
        Ok((outcome, after < before))
    }
}

impl SemanticSession {
    pub fn new(settings: SemanticRetrievalConfig) -> Self {
        Self::with_artifact(settings, ArtifactSource::Pinned)
    }

    pub fn with_artifact(settings: SemanticRetrievalConfig, source: ArtifactSource) -> Self {
        Self {
            settings,
            source,
            state: Arc::new(Mutex::new(SemanticState::new())),
        }
    }

    pub fn settings(&self) -> SemanticRetrievalConfig {
        self.settings
    }

    /// Runs one retrieval, with or without the semantic channel.
    ///
    /// The first early return is the R1.1 contract and may not be reordered:
    /// when the channel is off **or there is no question to embed**, nothing is
    /// loaded, nothing is consulted, and the status is `NotRequested` — and
    /// that holds under `semantic_required` too. Refusing a request that asked
    /// for no semantic work, because a model is missing, would be answering a
    /// question nobody put.
    pub fn retrieve(
        &self,
        // The same witness every other Core call in this crate takes. It is
        // built in one place, inside `off_reactor`, so holding one is proof
        // that this indexing pass is not running on the thread the protocol is
        // read with — see `crate::domain::OffThread`.
        _off: &crate::domain::OffThread,
        core: &NoteItCore,
        request: &engine::ContextRequest,
    ) -> Retrieved {
        if !self.settings.semantic_is_enabled() || !engine::semantic_channel_applies(request) {
            return match engine::retrieve(core, request) {
                Ok(answer) => Retrieved::Answer(answer, SemanticStatus::NotRequested),
                Err(error) => Retrieved::Refused(error),
            };
        }

        // From here the channel was asked for and there is something to ask it.
        // The lock is the "one indexing per process" rule: a second question
        // arriving during a cold index waits here and then finds the first
        // one's work, rather than building a second index beside it.
        let mut state = match self.state.lock() {
            Ok(state) => state,
            // A panic under the lock leaves derived data behind, and derived
            // data is exactly what may be discarded: the index is rebuilt from
            // the notes, which were never in danger.
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.index = None;
                state.vectors = 0;
                state.indexed_at = None;
                state
            }
        };

        state.ensure_provider(&self.source);
        if !state.ready() {
            drop(state);
            return self.degraded(core, request);
        }

        let fallback = match self.settings.fallback {
            SemanticFallbackPolicy::SemanticRequired => SemanticFallback::Required,
            _ => SemanticFallback::Automatic,
        };

        match state.sync_and_run(core, request, fallback) {
            Ok((outcome, forgot)) => {
                if !forgot {
                    return Retrieved::Answer(outcome.result, outcome.semantic_status);
                }
                // Something was edited since it was indexed. It has just been
                // dropped from the index, so one more pass re-embeds exactly
                // those notes and asks again — bounded to a single retry, so a
                // note being written continuously cannot spin here.
                match state.sync_and_run(core, request, fallback) {
                    Ok((outcome, _)) => Retrieved::Answer(outcome.result, outcome.semantic_status),
                    Err(error) => Self::refuse(error),
                }
            }
            Err(error) => Self::refuse(error),
        }
    }

    fn refuse(error: engine::RetrievalError) -> Retrieved {
        match error {
            engine::RetrievalError::Context(error) => Retrieved::Refused(error),
            engine::RetrievalError::Semantic(_) => Retrieved::SemanticRequired,
        }
    }

    /// The channel was asked for, could not run, and the policy decides.
    fn degraded(&self, core: &NoteItCore, request: &engine::ContextRequest) -> Retrieved {
        if self.settings.fallback == SemanticFallbackPolicy::SemanticRequired {
            return Retrieved::SemanticRequired;
        }
        match engine::retrieve(core, request) {
            // Attempted, failed, degraded — and the answer says so.
            Ok(answer) => Retrieved::Answer(answer, SemanticStatus::Unavailable),
            Err(error) => Retrieved::Refused(error),
        }
    }

    /// What the channel is, for a person looking at a diagnostic.
    ///
    /// Whether the artifact is *available* is answered without loading one: the
    /// question is about the machine, and a diagnostic that spent seconds
    /// hashing half a gigabyte to answer it would be a different feature.
    pub fn report(&self) -> SemanticReport {
        let state = self.state.lock().ok();
        let (indexed_notes, indexed_vectors, last_indexed, loaded, failure) = match state.as_deref()
        {
            Some(state) => (
                state.index.as_ref().map(InMemoryIndex::notes),
                state.index.as_ref().map(|_| state.vectors),
                state.indexed_at,
                state.ready(),
                match &state.provider {
                    Loaded::Failed(error) => Some(error.clone()),
                    _ => None,
                },
            ),
            None => (None, None, None, false, None),
        };
        SemanticReport {
            enabled: self.settings.semantic_is_enabled(),
            provider: noteit_embedding_local::PROVIDER_ID,
            model: self.source.model().to_string(),
            local: true,
            artifact_available: loaded || self.source.present(),
            artifact_error: failure,
            indexed_notes,
            indexed_vectors,
            last_indexed,
        }
    }
}

/// Brings the index up to date with the live store, and only where it is not.
///
/// The rule is one sentence: **index what the index does not hold, forget what
/// the store no longer has.** Everything else follows from it, including the
/// part that looks missing:
///
/// * a note that was never indexed is not held → it is read and indexed;
/// * a note that was **edited** is still held, so this pass leaves it alone —
///   and then the retrieval reads it, finds `source_revision` no longer
///   matches the note as it is now, discards the candidate and forgets the
///   note. It is then no longer held, so the next pass reindexes it. The
///   caller notices the drop and runs one more pass immediately, so the edit
///   is visible to the very question that revealed it;
/// * a note in the trash is not in the live scan → it is forgotten;
/// * a restored note is live and not held → it is indexed again.
///
/// What this deliberately does **not** do is ask a second, cheaper question
/// about whether a note changed. `updated_at` moves with the text and stays put
/// when a tag, a property or a colour changes, so a pass that trusted it would
/// keep stale vectors for exactly the edits the revision exists to catch. The
/// canonical revision stays the only detector of note state, which is what §7
/// of `docs/semantic-retrieval.md` demands.
///
/// One note changing therefore costs one note's embedding, never the store's.
fn synchronise(
    core: &NoteItCore,
    provider: &LocalProvider,
    index: &mut InMemoryIndex,
) -> Result<(), engine::ContextError> {
    let live = core
        .storage()
        .list_notes_by_recency()
        .map_err(|_| engine::ContextError::StoreUnavailable)?;
    let live_set: BTreeSet<Uuid> = live.iter().copied().collect();

    for note_id in index.note_ids() {
        if !live_set.contains(&note_id) {
            index.invalidate_note(&note_id);
        }
    }

    for note_id in live {
        if index.holds(&note_id) {
            continue;
        }
        let Ok(document) = core.read_note(&note_id) else {
            // A note that cannot be read is not a candidate anywhere else
            // either, and the retrieval will report it as a warning in its own
            // words. Skipped rather than half-indexed.
            continue;
        };
        // A note that cannot be embedded — an artifact and a text that have
        // nothing in common — is left out of the index rather than allowed to
        // fail the whole pass. Lexical retrieval still finds it.
        let _ = index_document(&document, provider, index);
    }
    Ok(())
}
