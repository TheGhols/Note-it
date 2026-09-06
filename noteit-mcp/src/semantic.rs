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

use noteit_core::chunking::CHUNKER_VERSION;
use noteit_core::context::{self as engine, RetrievalMode, SemanticStatus};
use noteit_core::embedding::EmbeddingSpaceId;
use noteit_core::semantic::{
    index_document, EmbeddingProvider, EmbeddingRecord, InMemoryIndex, SemanticFallback,
    SemanticIndex, SemanticRuntime,
};
use noteit_core::settings::{SemanticFallbackPolicy, SemanticRetrievalConfig};
use noteit_core::{NoteItCore, StorePaths, Uuid};
use noteit_embedding_local::{ArtifactError, ArtifactExpectation, LocalProvider};
use noteit_embedding_remote::worker::{WorkerHandle, WorkerPaths};
use noteit_embedding_remote::{cache, RemoteConfig, RemoteProvider, RemoteProviderId};
use std::collections::{BTreeMap, BTreeSet};
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
    /// The in-process provider, with its artifact read and verified.
    Local(Box<LocalProvider>),
    /// The out-of-process one.
    ///
    /// Building this starts nothing: a `RemoteProvider` holds a socket path and
    /// a handle, and the worker is spawned by the first request that actually
    /// needs one (§44). So there is no "failed to load" state for the
    /// remote arm — a provider that cannot be reached fails per request, with
    /// the typed word for why, and the next request may well succeed.
    Remote(Box<RemoteProvider>),
    /// Attempted and refused. Remembered rather than retried: the artifact is
    /// half a gigabyte and re-reading it on every query to be told the same
    /// thing again would cost seconds per question. Provisioning a model is an
    /// explicit act, and so is restarting the server after it.
    Failed(ArtifactError),
}

impl Loaded {
    /// The provider, whichever it is.
    fn provider(&self) -> Option<&dyn EmbeddingProvider> {
        match self {
            Self::Local(provider) => Some(provider.as_ref()),
            Self::Remote(provider) => Some(provider.as_ref()),
            Self::Never | Self::Failed(_) => None,
        }
    }

    fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }
}

/// The provider, the index, and how much of the store is in it.
struct SemanticState {
    provider: Loaded,
    index: Option<InMemoryIndex>,
    indexed_at: Option<SystemTime>,
    vectors: usize,
    /// Whether the on-disk cache has been consulted for the space now held.
    ///
    /// Consulted once per space and not once per query: reading it is the
    /// thing that stops a second start from re-embedding the whole store, and
    /// doing it again on every question would be a different kind of waste.
    cache_consulted: bool,
    /// Whether the index has changed since the cache was last written.
    ///
    /// **Changed, not "embedded" — and that distinction was a defect.** The
    /// first version set this only when something was newly embedded, so a pass
    /// whose only change was *forgetting* a note left the file alone: the
    /// vectors of a trashed note stayed on disk, were loaded back on the next
    /// start, forgotten again, and never collected. That is the unbounded
    /// growth §16 forbids and the orphan collection §83 requires, and
    /// `a_trashed_note_is_collected_from_the_cache_on_disk` is the test that
    /// found it.
    ///
    /// A start that found every note already cached and lost none still writes
    /// nothing, which is what keeps "a warm start costs no requests" also
    /// meaning "a warm start costs no rewrite" (§81).
    cache_dirty: bool,
}

impl SemanticState {
    fn new() -> Self {
        Self {
            provider: Loaded::Never,
            index: None,
            indexed_at: None,
            vectors: 0,
            cache_consulted: false,
            cache_dirty: false,
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

    /// Whether a diagnostic may say the artifact is there.
    ///
    /// It asks the provider crate, which answers by the same rule its loader
    /// applies — `symlink_metadata`, regular files only, plausible sizes — so
    /// this cannot report "available" for something `load` would refuse. It
    /// still reads no bytes: a report is not a verification, and saying so
    /// costs one `stat` per file.
    fn present(&self) -> bool {
        let directory = match self {
            Self::Pinned => match noteit_embedding_local::artifact_directory(
                &noteit_embedding_local::POTION_MULTILINGUAL_128M,
            ) {
                Some(directory) => directory,
                None => return false,
            },
            Self::At { directory, .. } => directory.clone(),
        };
        noteit_embedding_local::artifact::artifact_availability(&directory).is_ok()
    }

    fn model(&self) -> &str {
        match self {
            Self::Pinned => noteit_embedding_local::POTION_MULTILINGUAL_128M.model,
            Self::At { expectation, .. } => expectation.model,
        }
    }
}

/// Where a remote provider's worker and cache live.
#[derive(Clone)]
pub struct RemoteSource {
    pub config: RemoteConfig,
    pub worker: Arc<WorkerHandle>,
    /// Where the worker looks for a credential, so a diagnostic can ask
    /// whether one is there without reaching for the worker.
    pub config_dir: PathBuf,
    /// Where the derived vector cache goes.
    ///
    /// `None` means no cache at all, which is a state only a test asks for: in
    /// the product a remote provider without persistence would re-embed the
    /// whole store on every start, and §20 measured what that costs in
    /// money rather than in seconds.
    pub cache_root: Option<PathBuf>,
}

/// What a session builds when it is asked for a provider.
#[derive(Clone)]
pub enum ProviderSource {
    Local(ArtifactSource),
    Remote(Box<RemoteSource>),
}

impl ProviderSource {
    /// Resolves the configuration into something buildable.
    ///
    /// **Nothing is loaded and no process is started here.** This decides what
    /// *would* be built; `ensure_provider` is what builds it, and only when a
    /// question actually needs one.
    pub fn from_settings(settings: &SemanticRetrievalConfig, paths: &StorePaths) -> Self {
        match RemoteProviderId::parse(settings.provider.as_str()) {
            None => Self::Local(ArtifactSource::Pinned),
            Some(provider) => {
                let (dimension, dimension_requested) = settings.resolved_dimension();
                Self::Remote(Box::new(RemoteSource {
                    config: RemoteConfig {
                        provider,
                        model: settings.resolved_model(),
                        dimension,
                        dimension_requested,
                    },
                    worker: Arc::new(WorkerHandle::new(WorkerPaths::resolve(
                        paths.config_dir.clone(),
                        &paths.runtime_dir,
                    ))),
                    config_dir: paths.config_dir.clone(),
                    cache_root: cache::default_root(),
                }))
            }
        }
    }

    fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    fn model(&self) -> String {
        match self {
            Self::Local(source) => source.model().to_string(),
            Self::Remote(source) => source.config.model.clone(),
        }
    }

    fn provider_id(&self) -> &'static str {
        match self {
            Self::Local(_) => noteit_embedding_local::PROVIDER_ID,
            Self::Remote(source) => source.config.provider.as_str(),
        }
    }

    fn present(&self) -> bool {
        match self {
            Self::Local(source) => source.present(),
            // A remote provider needs no artifact on this machine. Whether it
            // can *answer* is a different question, and one a diagnostic must
            // not spend a request to ask (§53).
            Self::Remote(_) => true,
        }
    }
}

/// A handle on that state, cheap to clone and shared by every request.
#[derive(Clone)]
pub struct SemanticSession {
    settings: SemanticRetrievalConfig,
    source: ProviderSource,
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
    /// Whether note text leaves the machine under this configuration.
    pub remote: bool,
    /// The dimension the space declares.
    pub dimension: usize,
    /// Whether the vendor promises the model behind this name does not move.
    pub space_verifiable: bool,
    /// Whether a key exists for the configured provider. **Never the key.**
    pub credential_present: bool,
    /// Whether a worker is running now. Asked without starting one.
    pub worker_running: bool,
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
    fn ensure_provider(&mut self, source: &ProviderSource) {
        if !matches!(self.provider, Loaded::Never) {
            return;
        }
        self.provider = match source {
            ProviderSource::Local(artifact) => match artifact.load() {
                Ok(provider) => Loaded::Local(Box::new(provider)),
                Err(error) => Loaded::Failed(error),
            },
            // Constructing this reads nothing and starts nothing. The worker
            // appears on the first request that needs one.
            ProviderSource::Remote(remote) => Loaded::Remote(Box::new(
                RemoteProvider::with_worker(remote.config.clone(), Arc::clone(&remote.worker)),
            )),
        };
    }

    fn ready(&self) -> bool {
        self.provider.provider().is_some()
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
        cache_root: Option<&std::path::Path>,
    ) -> Result<(engine::RetrievalOutcome, bool), engine::RetrievalError> {
        let Some(provider) = self.provider.provider() else {
            unreachable!("callers check readiness before reaching here")
        };
        let remote = self.provider.is_remote();

        // An index belongs to one space. A provider whose artifact changed is a
        // different space, and the old index is dropped rather than
        // reinterpreted — reinterpreting is the failure §5 measured.
        let space = provider.space();
        let stale_space = self
            .index
            .as_ref()
            .is_none_or(|index| *SemanticIndex::space(index) != space);
        if stale_space {
            self.index = Some(InMemoryIndex::new(space.clone()));
            self.indexed_at = None;
            // A new space is a new cache to consult. Without this, switching
            // provider and switching back would reuse the first space's
            // "already consulted" and never read the second's file.
            self.cache_consulted = false;
            self.cache_dirty = false;
        }
        let index = self.index.as_mut().expect("an index was just ensured");

        // The cache is read once per space, before anything is embedded. This
        // is the whole reason it exists: without it a second start re-sends
        // every note to the provider and the user pays twice for the same
        // vectors (§20, §81).
        if remote && !self.cache_consulted {
            self.cache_consulted = true;
            if let Some(root) = cache_root {
                restore_from_cache(root, &space, index);
            }
        }

        let synced = synchronise(core, provider, index).map_err(engine::RetrievalError::Context)?;
        if synced.changed() {
            self.cache_dirty = true;
        }
        let before = SemanticIndex::vector_count(index);
        self.vectors = before;
        self.indexed_at = Some(SystemTime::now());

        let runtime = SemanticRuntime::new(provider, index).with_fallback(fallback);
        let outcome = engine::retrieve_with(core, request, RetrievalMode::Semantic(runtime))?;

        let index = self.index.as_ref().expect("the index is still there");
        let after = SemanticIndex::vector_count(index);
        self.vectors = after;

        // Written after the retrieval rather than before it, so a question is
        // never made slower by a save it did not need — and only when the index
        // actually changed, so a warm start that lost nothing writes nothing.
        if remote && self.cache_dirty {
            if let Some(root) = cache_root {
                if persist(root, &space, index) {
                    self.cache_dirty = false;
                }
            }
        }
        Ok((outcome, after < before))
    }
}

/// Puts a saved space's vectors back into a fresh index.
///
/// Every refusal the cache can make is the same answer here — start empty and
/// let `synchronise` embed what is missing — because a cache is derived and
/// rebuilding it is always correct. Nothing is repaired and nothing is
/// partially read: `load` either returns a whole, verified set or an error.
fn restore_from_cache(root: &std::path::Path, space: &EmbeddingSpaceId, index: &mut InMemoryIndex) {
    let Ok(records) = cache::load(root, space, CHUNKER_VERSION) else {
        return;
    };
    let mut by_note: BTreeMap<Uuid, Vec<EmbeddingRecord>> = BTreeMap::new();
    for record in records {
        by_note.entry(record.note_id).or_default().push(record);
    }
    for (note_id, records) in by_note {
        // `replace_note` validates the whole batch — space, chunker, note —
        // before it accepts any of it, so a cache that passed its own checks
        // and still disagrees with this index is refused here rather than
        // mixed in.
        let _ = index.replace_note(&note_id, records);
    }
}

/// Writes the index for this space, and bounds what is kept.
///
/// Returns whether the save succeeded. A failure is not an error to report to
/// the caller: the vectors are in memory and the answer is already correct, and
/// the only cost of a cache that did not get written is that the next start
/// pays for them again.
fn persist(root: &std::path::Path, space: &EmbeddingSpaceId, index: &InMemoryIndex) -> bool {
    let mut records = Vec::with_capacity(SemanticIndex::vector_count(index));
    for note_id in index.note_ids() {
        records.extend(index.records_for(&note_id).iter().cloned());
    }
    if cache::save(root, space, CHUNKER_VERSION, &records).is_err() {
        return false;
    }
    cache::prune(root, space);
    true
}

impl SemanticSession {
    /// The session the product builds, from a configuration and the store's
    /// own paths.
    ///
    /// The paths are needed because a remote provider's worker socket lives in
    /// the runtime directory and its credentials file in the config directory —
    /// and because neither is a thing this crate should be resolving twice.
    pub fn new(settings: SemanticRetrievalConfig, paths: &StorePaths) -> Self {
        let source = ProviderSource::from_settings(&settings, paths);
        Self::with_source(settings, source)
    }

    /// A session pointed at a local artifact the caller named.
    pub fn with_artifact(settings: SemanticRetrievalConfig, source: ArtifactSource) -> Self {
        Self::with_source(settings, ProviderSource::Local(source))
    }

    /// A session whose provider the caller resolved.
    pub fn with_source(settings: SemanticRetrievalConfig, source: ProviderSource) -> Self {
        Self {
            settings,
            source,
            state: Arc::new(Mutex::new(SemanticState::new())),
        }
    }

    /// Where the remote cache goes, when there is one.
    fn cache_root(&self) -> Option<&std::path::Path> {
        match &self.source {
            ProviderSource::Local(_) => None,
            ProviderSource::Remote(remote) => remote.cache_root.as_deref(),
        }
    }

    pub fn settings(&self) -> SemanticRetrievalConfig {
        self.settings.clone()
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

        let cache_root = self.cache_root();
        match state.sync_and_run(core, request, fallback, cache_root) {
            Ok((outcome, forgot)) => {
                if !forgot {
                    return Retrieved::Answer(outcome.result, outcome.semantic_status);
                }
                // Something was edited since it was indexed. It has just been
                // dropped from the index, so one more pass re-embeds exactly
                // those notes and asks again — bounded to a single retry, so a
                // note being written continuously cannot spin here.
                match state.sync_and_run(core, request, fallback, cache_root) {
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
    ///
    /// Nothing here starts a worker, opens a socket or reaches a network, and
    /// nothing here reads a key's value. §53 asks for exactly that: a
    /// person must be able to see the mode, the provider, the model, the
    /// dimension, whether the space is verifiable, the state of the index, and
    /// whether a credential exists — **without** the act of looking becoming
    /// the act that sends something.
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
        let remote = self.source.is_remote();
        let (dimension, space_verifiable, credential_present, worker_running) = match &self.source {
            ProviderSource::Local(_) => (
                noteit_embedding_local::POTION_MULTILINGUAL_128M.dimension,
                // A local artifact's identity is the digest of bytes that were
                // loaded, which is the strongest of the three answers §5.1
                // defines.
                true,
                false,
                false,
            ),
            ProviderSource::Remote(source) => {
                let space = noteit_embedding_remote::space::space_for(
                    source.config.provider,
                    &source.config.model,
                    source.config.dimension,
                );
                (
                    source.config.dimension,
                    space.artifact.is_verifiable(),
                    // Yes or no. The value never enters this process: this is a
                    // `bool` and there is no field it could go in.
                    noteit_embedding_remote::credential_present(
                        source.config.provider,
                        &source.config_dir,
                    ),
                    // `is_reachable` and not `is_running`, and that was a
                    // defect: since the socket became one per session, a worker
                    // may well belong to another Note-it process — `ensure`
                    // adopts a live one rather than starting a second beside
                    // it. `is_running` answers "did *this handle* spawn one",
                    // which is bookkeeping; a person reading a diagnostic is
                    // asking about the machine. Still asked and never caused:
                    // this connects to a Unix socket and hangs up, and starts
                    // nothing.
                    source.worker.is_reachable(),
                )
            }
        };
        SemanticReport {
            enabled: self.settings.semantic_is_enabled(),
            provider: self.source.provider_id(),
            model: self.source.model(),
            local: !remote,
            artifact_available: loaded || self.source.present(),
            artifact_error: failure,
            indexed_notes,
            indexed_vectors,
            last_indexed,
            remote,
            dimension,
            space_verifiable,
            credential_present,
            worker_running,
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
/// What one synchronisation pass did.
///
/// Two counts and not one, because they answer two different questions and the
/// first version of this conflated them. `embedded` is what a request costs
/// (§81, §82); `forgotten` is what the cache on disk has to stop
/// holding (§16, §83). A pass can do either without the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Synced {
    /// Notes read and embedded on this pass. Each one cost the provider.
    embedded: usize,
    /// Notes dropped because the live store no longer has them.
    forgotten: usize,
}

impl Synced {
    /// Whether the index is different from what the cache on disk holds.
    fn changed(self) -> bool {
        self.embedded > 0 || self.forgotten > 0
    }
}

fn synchronise(
    core: &NoteItCore,
    provider: &dyn EmbeddingProvider,
    index: &mut InMemoryIndex,
) -> Result<Synced, engine::ContextError> {
    let live = core
        .storage()
        .list_notes_by_recency()
        .map_err(|_| engine::ContextError::StoreUnavailable)?;
    let live_set: BTreeSet<Uuid> = live.iter().copied().collect();

    // Counted, because a note that left the store has to leave the file too —
    // not only the in-memory index. Without this count the cache kept the
    // vectors of trashed notes for ever.
    let mut forgotten = 0usize;
    for note_id in index.note_ids() {
        if !live_set.contains(&note_id) {
            index.invalidate_note(&note_id);
            forgotten += 1;
        }
    }

    // Counted, and together with `forgotten` this is what decides whether the
    // remote cache is rewritten. A pass that embedded nothing *and* lost
    // nothing found every note already cached, and rewriting the same file for
    // it would be the one cost a warm start is not supposed to have
    // (§81, §82).
    let mut embedded = 0usize;
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
        // nothing in common, or a provider that is not answering — is left out
        // of the index rather than allowed to fail the whole pass. Lexical
        // retrieval still finds it.
        if index_document(&document, provider, index).is_ok() {
            embedded += 1;
        }
    }
    Ok(Synced {
        embedded,
        forgotten,
    })
}
