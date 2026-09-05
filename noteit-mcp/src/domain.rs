//! The bridge from a typed tool argument to the one write authority.
//!
//! This module is the whole of what `noteit-mcp` does with the store, and it
//! is deliberately small. It:
//!
//! - opens the Core read-only for reads, and never with directory creation;
//! - builds a [`WriteOperation`] for writes and hands it to
//!   [`noteit_core::authority::perform_at`].
//!
//! It does not open a `.md` file. It does not spawn `noteit`. It does not
//! parse anybody's JSON output. It does not take the writer lease, connect to
//! the control socket, count a retry window or decide a timeout — all of that
//! is the authority's, in the Core, in one implementation shared with the
//! command line.
//!
//! ## Why nothing here can run on the protocol's thread
//!
//! Every function below that touches the store takes an [`OffThread`]. It has
//! one constructor, it is private to this module, and [`off_reactor`] is the
//! only thing that calls it — inside the closure `spawn_blocking` runs. So a
//! Core call on the reactor is not a mistake this crate can make: it does not
//! compile. A sixteenth tool added later cannot forget the runtime for the
//! same reason the fifteenth cannot forget `expected_revision`.
//!
//! ## Why a mutation cannot be built without a revision
//!
//! [`ExistingNoteMutation`] is the only way this crate can produce a
//! [`WriteOperation::MutateNote`], and its `expected_revision` is a
//! [`NoteRevision`] rather than an `Option<NoteRevision>`. There is no
//! constructor that omits it and no field to leave empty, so a mutation tool
//! added later cannot reach the store unconditionally by forgetting something
//! — it cannot be written at all.

use crate::budget;
use crate::contract::{
    CommitState, ContextCandidateView, ContextInput, ContextReason, ContextResult, ContextTaskView,
    ContextWarningView, ErrorCode, ListResult, NoteSummaryView, NoteView, Property, ReadResult,
    SearchHitView, SearchResult, SemanticStatusView, Status, TaskState, TaskView, TasksResult,
    TrashEntryView, TrashResult, Warning, WarningCode, WriteResult, MAX_TRASH_ENTRIES,
    MAX_WARNINGS,
};
use crate::semantic::{Retrieved, SemanticSession};
use noteit_core::authority;
use noteit_core::chrono::{DateTime, SecondsFormat, Utc};
use noteit_core::context as engine;
use noteit_core::revision::NoteRevision;
use noteit_core::settings::{AppConfig, SemanticRetrievalConfig};
use noteit_core::write::{NoteDraft, NoteMutation, WriteError, WriteOperation, WriteOutcome};
use noteit_core::{
    NoteDocument, NoteFilter, NoteItCore, NoteProperty, NoteSelectorError, NoteSummary,
    ReadWarning, ReadWarningKind, StorePaths, TaskEntry, TaskStateFilter, TrashEntry, Uuid,
};

/// The store this server speaks for.
///
/// Resolved once, from the process's own XDG environment, and never from a
/// tool argument. A client cannot name a store, a directory or a path: there
/// is no field for one anywhere in the contract, so there is nothing to
/// validate and nothing to escape.
#[derive(Clone)]
pub struct Store {
    paths: StorePaths,
    /// The semantic channel's lifetime, which is the process's rather than a
    /// request's. Cloned into every offloaded call and shared by all of them,
    /// so the model is loaded once and the index is built once — see
    /// [`crate::semantic`].
    semantic: SemanticSession,
}

/// Redacted, because a derived one would print the index.
impl std::fmt::Debug for Store {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Store")
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

impl Store {
    /// The store the ambient environment resolves to.
    ///
    /// No directory is created and no model is loaded. The retrieval
    /// configuration *is* read, because whether the semantic channel exists at
    /// all is the user's decision and there is nowhere else it could come from;
    /// a configuration that is missing, unreadable or corrupt yields the
    /// factory default, which is lexical retrieval.
    pub fn resolve() -> Self {
        let paths = StorePaths::resolve();
        let semantic = AppConfig::read_only(&paths.config_file_path()).semantic_retrieval;
        Self::with_settings(paths, semantic)
    }

    /// An explicitly named store. Used by the tests, which never point at a
    /// real one.
    pub fn at(paths: StorePaths) -> Self {
        Self::with_settings(paths, SemanticRetrievalConfig::default())
    }

    /// A store whose retrieval configuration came from somewhere.
    ///
    /// The default is the factory default in every constructor above, which is
    /// what makes "a release cannot turn semantics on" true of this type and
    /// not only of the file format.
    pub fn with_settings(paths: StorePaths, semantic: SemanticRetrievalConfig) -> Self {
        Self {
            paths,
            semantic: SemanticSession::new(semantic),
        }
    }

    /// A store whose semantic channel was assembled by the caller.
    ///
    /// The suites use it to point the provider at a table they built
    /// themselves, so every contract of the lifecycle is provable without the
    /// shipped artifact — the same posture a machine that never provisioned one
    /// is in.
    pub fn with_semantic_session(paths: StorePaths, semantic: SemanticSession) -> Self {
        Self { paths, semantic }
    }

    pub fn paths(&self) -> &StorePaths {
        &self.paths
    }

    /// What a diagnostic surface may say about the semantic channel.
    pub fn semantic_report(&self) -> crate::semantic::SemanticReport {
        self.semantic.report()
    }

    /// The read-only view. Creates no directory, no state file and no backup.
    ///
    /// Takes the witness rather than checking anything: opening the store is
    /// where a read starts touching the filesystem, so this is the narrowest
    /// place the rule can be made unforgettable.
    fn reader(&self, _off: &OffThread) -> NoteItCore {
        NoteItCore::open_read_only_at(self.paths.clone())
    }
}

/// Proof that the caller is not on the protocol's thread.
///
/// Carried by every function here that opens the store. The field is private
/// and the only value is built inside [`off_reactor`], so possessing one means
/// the work really is running on a blocking thread rather than on the reactor
/// `noteit-mcp` reads standard input with.
pub struct OffThread(());

/// Why a Core call did not produce an answer at all.
///
/// Not a refusal from the store — those are [`WriteResult`]s and read results
/// with a code. This is the executor itself failing, and it deliberately
/// carries no detail from the failure: a panic message can quote whatever the
/// code was holding, which here is note content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffloadFailed {
    /// The work panicked. The panic itself was already reported by the default
    /// hook, on standard error, where it does not corrupt the protocol.
    Panicked,
    /// The blocking task was cancelled. Nothing here cancels one, so this
    /// means the runtime is shutting down under the call.
    Cancelled,
}

/// Runs one Core operation off the protocol's thread.
///
/// The single door to the store, and the only place an [`OffThread`] comes
/// from. `spawn_blocking` puts the work on Tokio's blocking pool, which is a
/// separate set of threads from the runtime's own — so a *current-thread*
/// runtime keeps reading standard input, answering `ping` and accepting the
/// next request while the disk is busy.
pub async fn off_reactor<T, F>(store: &Store, work: F) -> Result<T, OffloadFailed>
where
    F: FnOnce(&OffThread, &Store) -> T + Send + 'static,
    T: Send + 'static,
{
    let store = store.clone();
    match tokio::task::spawn_blocking(move || work(&OffThread(()), &store)).await {
        Ok(value) => Ok(value),
        Err(error) if error.is_panic() => Err(OffloadFailed::Panicked),
        Err(_) => Err(OffloadFailed::Cancelled),
    }
}

/// A change to a note that already exists.
///
/// The precondition is not optional and there is no way to build one of these
/// without it. See the module documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingNoteMutation {
    selector: String,
    expected_revision: NoteRevision,
    mutation: NoteMutation,
}

impl ExistingNoteMutation {
    /// Reads a tool's arguments into a mutation, or refuses them.
    ///
    /// The revision is parsed here, and a malformed one is an
    /// [`ErrorCode::InvalidInput`] rather than "no precondition". That
    /// distinction is the entire safety of the field: a token that quietly
    /// became `None` would hand a client an unconditional write over a note it
    /// has not looked at.
    ///
    /// The refusal is boxed because it is the wide half of this answer and the
    /// narrow half is what almost every call returns.
    pub fn new(
        selector: String,
        expected_revision: &str,
        mutation: NoteMutation,
    ) -> Result<Self, Box<WriteResult>> {
        // The parse error says how the token was wrong, and saying so would
        // mean quoting an argument of whatever length arrived. The code says
        // `invalid_input`; the schema says what a revision is.
        let revision = NoteRevision::parse(expected_revision).map_err(|_| {
            Box::new(WriteResult::refusal_saying(
                CommitState::NotCommitted,
                ErrorCode::InvalidInput,
                "`expected_revision` must be sixty-four lowercase hexadecimal characters, \
                 exactly as `noteit_read` published them",
            ))
        })?;
        Ok(Self {
            selector,
            expected_revision: revision,
            mutation,
        })
    }

    /// The operation this mutation is, with its precondition attached.
    ///
    /// `Some(...)` always: this type cannot exist without a revision, which is
    /// why there is no branch here that could produce `None`.
    pub fn into_operation(self) -> WriteOperation {
        WriteOperation::MutateNote {
            selector: self.selector,
            mutation: self.mutation,
            expected_revision: Some(self.expected_revision),
        }
    }
}

// ================================================================== writes

/// Runs one mutation of an existing note.
pub fn mutate(off: &OffThread, store: &Store, mutation: ExistingNoteMutation) -> WriteResult {
    perform(off, store, &mutation.into_operation())
}

/// Creates a note. The only write that takes no precondition, because there is
/// no earlier version of a note that does not exist yet.
pub fn create(
    off: &OffThread,
    store: &Store,
    content: String,
    tags: Vec<String>,
    properties: Vec<Property>,
) -> WriteResult {
    perform(
        off,
        store,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content,
                tags,
                properties: properties
                    .into_iter()
                    .map(|property| NoteProperty {
                        key: property.key,
                        value: property.value,
                    })
                    .collect(),
            },
        },
    )
}

/// Moves a note back out of the trash. A move, not an edit — see
/// [`crate::contract::TrashRestoreInput`].
pub fn restore(off: &OffThread, store: &Store, selector: String) -> WriteResult {
    perform(off, store, &WriteOperation::RestoreFromTrash { selector })
}

/// The single place a write leaves this crate.
///
/// Every write tool ends here, so there is exactly one call to the authority
/// and exactly one translation of its answer.
fn perform(_off: &OffThread, store: &Store, operation: &WriteOperation) -> WriteResult {
    match authority::perform_at(store.paths(), operation) {
        // The path the write took — direct or through the running desktop
        // instance — is deliberately dropped. It is a detail of a private
        // conversation between two Note-it processes, and publishing it would
        // make it something an agent could come to depend on.
        Ok(performed) => committed(performed.outcome),
        Err(error) => refused(&error),
    }
}

/// Turns a committed outcome into the published answer.
///
/// `WriteOutcome::kind` is deliberately **not** published. An agent knows which
/// tool it called, so `content_appended` beside a `noteit_append` answer says
/// nothing it did not already know, and every field this contract carries is
/// one more thing that can never be renamed. The machine interface publishes
/// `kind` because a `--json` document has to say which command produced it;
/// this boundary does not have that problem.
///
/// That is a decision, not an oversight, and it is one a new `WriteOutcomeKind`
/// in the Core has to be measured against — so it is pinned by
/// `mcp_contract_decisions.rs`, whose exhaustive `match` over the enum does not
/// compile until somebody has looked at the new variant.
fn committed(outcome: WriteOutcome) -> WriteResult {
    WriteResult {
        status: Status::Ok,
        commit_state: if outcome.changed {
            CommitState::Committed
        } else {
            CommitState::NotNeeded
        },
        note_id: Some(outcome.note_id.to_string()),
        changed: Some(outcome.changed),
        revision: outcome.revision.map(|revision| revision.to_string()),
        code: None,
        message: None,
        expected_revision: None,
        ui_sync_warning: outcome.ui_sync_warning,
    }
}

/// Names every refusal the Core can produce.
///
/// Exhaustive on purpose: a new [`WriteError`] variant is a compile error here
/// rather than a code nobody decided on reaching an agent.
fn refused(error: &WriteError) -> WriteResult {
    let code = match error {
        WriteError::InvalidInput { .. } => ErrorCode::InvalidInput,
        WriteError::Validation { .. } => ErrorCode::Validation,
        WriteError::NotFound { .. } => ErrorCode::NotFound,
        WriteError::AmbiguousSelector { .. } => ErrorCode::AmbiguousSelector,
        WriteError::StaleTaskRef { .. } => ErrorCode::StaleTaskRef,
        WriteError::AmbiguousTaskRef { .. } => ErrorCode::AmbiguousTaskRef,
        WriteError::WriterBusy { .. } => ErrorCode::WriterBusy,
        WriteError::AuthorityUnavailable { .. } => ErrorCode::AuthorityUnavailable,
        WriteError::Indeterminate { .. } => ErrorCode::Indeterminate,
        WriteError::TrashTargetOccupied { .. } => ErrorCode::TrashTargetOccupied,
        WriteError::Persistence { .. } => ErrorCode::Persistence,
        WriteError::StoreUnavailable { .. } => ErrorCode::StoreUnavailable,
        WriteError::RevisionConflict { .. } => ErrorCode::RevisionConflict,
    };

    // The one refusal that is not one. See `Status::Indeterminate`.
    let commit_state = match error {
        WriteError::Indeterminate { .. } => CommitState::Unknown,
        _ => CommitState::NotCommitted,
    };

    // `error.to_string()` is deliberately never called. The Core writes those
    // sentences for whoever is debugging a store, so they quote paths, parser
    // output and arguments at whatever length they arrived; see
    // `crate::contract::message_for`, which is where a public sentence comes
    // from now.
    let mut result = WriteResult::refusal(commit_state, code);
    if let WriteError::RevisionConflict {
        note_id,
        expected_revision,
        // The Core knows where the note actually is. This adapter is where
        // that stops: `current_revision` is read out of the error and dropped
        // here, and nothing below writes it anywhere. See ADR-051 — publishing
        // it made "read again" a rule an agent could decline to follow, because
        // the token it needed to ignore was in its hands.
        current_revision: _,
    } = error
    {
        result.note_id = Some(note_id.to_string());
        result.expected_revision = Some(expected_revision.to_string());
        // Deliberately no `revision` either: the field a caller chains the next
        // write from must never be filled in by a conflict.
    }
    if let WriteError::TrashTargetOccupied { note_id } = error {
        result.note_id = Some(note_id.to_string());
    }
    result
}

// =================================================================== reads

pub fn list(
    off: &OffThread,
    store: &Store,
    filter: &NoteFilter,
    limit: Option<usize>,
) -> ListResult {
    match store.reader(off).list_summaries(filter, limit) {
        Ok(batch) => {
            let notes: Vec<NoteSummaryView> = batch.items.iter().map(summary_view).collect();
            let reported = warnings(&batch.warnings);
            ListResult {
                status: Status::Ok,
                count: notes.len(),
                notes,
                warnings: reported.carried,
                warnings_truncated: reported.truncated,
                omitted_warning_count: reported.omitted,
                code: None,
                message: None,
            }
        }
        Err(_) => ListResult::refusal(ErrorCode::ReadFailed),
    }
}

/// Reads one note in full, or refuses to read it at all.
///
/// ## Full read or refusal, and never anything between
///
/// This is the only tool that answers with a whole note and the `revision` that
/// names it, and the two travel together for a reason: the revision is what
/// authorises the next write, and it authorises a write over *the state it
/// names*. An answer carrying part of a note and the revision of the whole of
/// it would hand a caller permission to overwrite text it had never seen — the
/// precise failure Phase 4.2D closed on the conflict path, arriving instead
/// through the read path.
///
/// So when the answer will not fit, nothing goes out but the code. No body, no
/// revision, no label, no tags, no timestamps: there is no partial state here
/// to be mistaken for a whole one, and no token to write from. See ADR-053.
///
/// The size is measured before the answer is built, not after — see
/// [`crate::budget`]. Sixteen megabytes of note cost one pass over sixteen
/// megabytes to refuse, instead of the thirty-four-megabyte answer and the
/// hundred and fifty megabytes of process the measurement found before this
/// existed.
pub fn read(off: &OffThread, store: &Store, selector: &str) -> ReadResult {
    let core = store.reader(off);
    let note_id = match core.resolve_note_id(selector) {
        Ok(note_id) => note_id,
        Err(error) => return ReadResult::refusal(selector_refusal(&error)),
    };
    let Ok(document) = core.read_note(&note_id) else {
        return ReadResult::refusal(ErrorCode::ReadFailed);
    };
    // A note that cannot be serialised cannot be given a version, and
    // answering without one would invite a write built on a base nobody can
    // name — which is exactly the unconditional write this server refuses to
    // offer.
    let Ok(revision) = NoteRevision::for_document(&document) else {
        return ReadResult::refusal(ErrorCode::ReadFailed);
    };

    let answer = ReadResult {
        status: Status::Ok,
        note: Some(note_view(&note_id, &document, &revision)),
        warnings: Vec::new(),
        warnings_truncated: false,
        omitted_warning_count: 0,
        code: None,
        message: None,
    };

    // Weighed as the host will receive it: the payload, the copy of it the
    // SDK publishes as a text block, and the envelope around both. A note that
    // is mostly quotation marks weighs more than its bytes, and this is where
    // that is counted rather than assumed.
    match budget::result_bytes(&answer) {
        Ok(bytes) if budget::within_read_budget(bytes) => answer,
        Ok(_) => ReadResult::refusal(ErrorCode::ResponseTooLarge),
        // A note that will not serialise is the same failure as one that will
        // not hash: there is no version to name it by, so there is no answer.
        Err(_) => ReadResult::refusal(ErrorCode::ReadFailed),
    }
}

pub fn search(
    off: &OffThread,
    store: &Store,
    query: &str,
    filter: &NoteFilter,
    limit: Option<usize>,
) -> SearchResult {
    match store
        .reader(off)
        .search_notes_filtered(query, filter, limit)
    {
        Ok(batch) => {
            let results: Vec<SearchHitView> = batch
                .items
                .iter()
                .map(|hit| SearchHitView {
                    note_id: hit.note_id.to_string(),
                    label: hit.label.clone(),
                    snippet: hit.snippet.clone(),
                    match_count: hit.match_count,
                    matched_text: hit.matched_text.clone(),
                })
                .collect();
            let reported = warnings(&batch.warnings);
            SearchResult {
                status: Status::Ok,
                query: query.to_string(),
                count: results.len(),
                results,
                warnings: reported.carried,
                warnings_truncated: reported.truncated,
                omitted_warning_count: reported.omitted,
                code: None,
                message: None,
            }
        }
        Err(_) => SearchResult::refusal(ErrorCode::ReadFailed),
    }
}

pub fn tasks(
    off: &OffThread,
    store: &Store,
    state: TaskState,
    filter: &NoteFilter,
    limit: Option<usize>,
) -> TasksResult {
    let state = match state {
        TaskState::Pending => TaskStateFilter::Pending,
        TaskState::Completed => TaskStateFilter::Completed,
        TaskState::All => TaskStateFilter::All,
    };
    match store.reader(off).list_tasks(state, filter, limit) {
        Ok(batch) => {
            let tasks: Vec<TaskView> = batch.items.iter().map(task_view).collect();
            let reported = warnings(&batch.warnings);
            TasksResult {
                status: Status::Ok,
                count: tasks.len(),
                tasks,
                warnings: reported.carried,
                warnings_truncated: reported.truncated,
                omitted_warning_count: reported.omitted,
                code: None,
                message: None,
            }
        }
        Err(_) => TasksResult::refusal(ErrorCode::ReadFailed),
    }
}

pub fn trash(off: &OffThread, store: &Store) -> TrashResult {
    let found = store.reader(off).list_trash();
    // The Core answers with the whole trash, ordered, because the desktop
    // window shows the whole trash. This surface is a discovery surface like
    // every other listing here and takes the same ceiling — see
    // `MAX_TRASH_ENTRIES`. What the ceiling left out is published rather than
    // silently dropped.
    let omitted = found.len().saturating_sub(MAX_TRASH_ENTRIES);
    let entries: Vec<TrashEntryView> = found
        .iter()
        .take(MAX_TRASH_ENTRIES)
        .map(trash_view)
        .collect();
    TrashResult {
        status: Status::Ok,
        count: entries.len(),
        entries,
        truncated: omitted > 0,
        omitted_count: omitted,
        warnings: Vec::new(),
        warnings_truncated: false,
        omitted_warning_count: 0,
        code: None,
        message: None,
    }
}

// ============================================================== translation

fn timestamp(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|value| value.to_rfc3339_opts(SecondsFormat::AutoSi, true))
}

fn properties(pairs: &[NoteProperty]) -> Vec<Property> {
    pairs
        .iter()
        .map(|property| Property {
            key: property.key.clone(),
            value: property.value.clone(),
        })
        .collect()
}

fn note_view(note_id: &Uuid, document: &NoteDocument, revision: &NoteRevision) -> NoteView {
    NoteView {
        note_id: note_id.to_string(),
        label: noteit_core::search::label_for(&document.content),
        content: document.content.clone(),
        tags: document.user_metadata.tags.as_slice().to_vec(),
        properties: properties(document.user_metadata.properties.as_slice()),
        created_at: timestamp(document.metadata.created_at),
        updated_at: timestamp(document.metadata.updated_at),
        revision: revision.to_string(),
    }
}

fn summary_view(summary: &NoteSummary) -> NoteSummaryView {
    NoteSummaryView {
        note_id: summary.id.to_string(),
        label: summary.label.clone(),
        snippet: summary.snippet.clone(),
        tags: summary.tags.clone(),
        properties: summary
            .properties
            .iter()
            .map(|(key, value)| Property {
                key: key.clone(),
                value: value.clone(),
            })
            .collect(),
        created_at: timestamp(summary.created_at),
        updated_at: timestamp(summary.updated_at),
    }
}

fn task_view(task: &TaskEntry) -> TaskView {
    TaskView {
        task_ref: task.task_ref.as_str().to_string(),
        note_id: task.note_id.to_string(),
        note_label: task.note_label.clone(),
        text: task.text.clone(),
        checked: task.checked,
        completed_at: timestamp(task.completed_at),
        depth: task.depth,
    }
}

fn trash_view(entry: &TrashEntry) -> TrashEntryView {
    TrashEntryView {
        note_id: entry.note_id.to_string(),
        label: entry.label.clone(),
        snippet: entry.snippet.clone(),
        deleted_at: timestamp(entry.deleted_at),
    }
}

/// The warnings one answer carries, and what it had to leave behind.
struct ReportedWarnings {
    carried: Vec<Warning>,
    truncated: bool,
    omitted: usize,
}

/// Turns the Core's read anomalies into the ones this boundary publishes.
///
/// Two things happen here and both are the point:
///
/// **The message is dropped.** `ReadWarning::message` is written for whoever is
/// debugging a store, so it names the file — `Leitura recusada: o arquivo
/// `/home/…/notes/….md` é um link simbólico` — and it is as long as whatever it
/// is quoting, which for a corrupt front matter is a scalar out of the note.
/// Neither belongs to a caller that is given `note_id` and no path anywhere else
/// in this contract.
///
/// **The list is bounded.** A store with twenty thousand symbolic links in it
/// answered a `noteit_list` asking for one note with twenty thousand warnings.
/// The context surface already stopped at twenty; this is the same ceiling.
fn warnings(raw: &[ReadWarning]) -> ReportedWarnings {
    ReportedWarnings {
        carried: raw
            .iter()
            .take(MAX_WARNINGS)
            .map(|warning| Warning {
                code: warning_code(warning.kind),
                note_id: warning.note_id.map(|id| id.to_string()),
            })
            .collect(),
        truncated: raw.len() > MAX_WARNINGS,
        omitted: raw.len().saturating_sub(MAX_WARNINGS),
    }
}

/// The one place a read anomaly becomes a published code.
///
/// Shared with the context surface, which publishes the code and deliberately
/// not the message: two copies of this `match` would be two chances for the
/// two surfaces to start describing the same damage differently.
fn warning_code(kind: ReadWarningKind) -> WarningCode {
    match kind {
        ReadWarningKind::UnreadableNote => WarningCode::UnreadableNote,
        ReadWarningKind::CorruptedFrontMatter => WarningCode::CorruptedFrontMatter,
        ReadWarningKind::SymlinkRefused => WarningCode::SymlinkRefused,
        ReadWarningKind::IoError => WarningCode::IoError,
    }
}

/// The code a selector refusal publishes.
///
/// The Core's own sentence quotes the selector, which is an argument of
/// whatever length arrived — three hundred kilobytes of `Z` came back as three
/// hundred kilobytes of `Z`. The code says what was wrong with it; the schema
/// says what a selector is.
fn selector_refusal(error: &NoteSelectorError) -> ErrorCode {
    match error {
        NoteSelectorError::InvalidFormat(_) | NoteSelectorError::SymlinkRefused(_) => {
            ErrorCode::InvalidInput
        }
        NoteSelectorError::NotFound(_) => ErrorCode::NotFound,
        NoteSelectorError::Ambiguous(_, _) => ErrorCode::AmbiguousSelector,
        NoteSelectorError::StoreUnavailable(_) => ErrorCode::StoreUnavailable,
    }
}

/// Builds the Core's filter from the contract's.
pub fn filter_of(tags: Vec<String>, properties: Vec<Property>) -> NoteFilter {
    NoteFilter::new(
        tags,
        properties
            .into_iter()
            .map(|property| (property.key, property.value))
            .collect(),
    )
}

/// The listing bound, as the Core takes it.
pub fn limit_of(limit: Option<u32>) -> Option<usize> {
    limit.map(|value| value as usize)
}

// ================================================================= context

/// Retrieves context, and translates it.
///
/// The whole of what this crate does with the Context Engine. It converts the
/// tool's arguments into the Core's request, calls
/// [`noteit_core::context::retrieve`] once, and copies the answer field by
/// field. It does not read a note, rank anything, build a snippet, parse a
/// task, sort, or recompute a truncation count — all of that is the Core's,
/// and doing any of it a second time here is how two implementations of one
/// idea start to disagree.
///
/// Takes the witness like every other store function, so the scan happens on a
/// blocking thread and the protocol keeps answering while it runs.
pub fn context(off: &OffThread, store: &Store, input: ContextInput) -> ContextResult {
    // Pure conversion: no file is opened to decide what a tag means, and a tag
    // nobody uses simply matches nothing.
    let request = engine::ContextRequest {
        query: input.query,
        filter: NoteFilter::new(
            input.tags,
            input
                .properties
                .into_iter()
                .map(|property| (property.key, property.value))
                .collect(),
        ),
        include_tasks: input.include_tasks,
        // The ceiling is the Core's. This only carries the caller's wish; the
        // engine clamps it, so no request can argue its way past fifty.
        limit: input.limit.map(|limit| limit as usize),
    };

    // One call, and which channels it uses is the configuration's decision
    // rather than this function's. In the factory default the session's own
    // first line takes the lexical path, where there is no field a provider
    // could go in.
    match store.semantic.retrieve(off, &store.reader(off), &request) {
        Retrieved::Answer(answer, status) => context_answer(answer, status),
        Retrieved::Refused(error) => ContextResult::refusal(match error {
            // The query is the caller's mistake and worth fixing.
            engine::ContextError::QueryTooLong { .. } => ErrorCode::InvalidInput,
            // And this one carries nothing to pass on — see ADR-049.2.
            engine::ContextError::StoreUnavailable => ErrorCode::StoreUnavailable,
        }),
        // The caller configured `semantic_required` and the channel did not
        // run. Refused rather than answered, because degrading in silence is
        // what that setting exists to forbid — and the code is the Note-it's
        // own, never a sentence from a library.
        Retrieved::SemanticRequired => ContextResult::refusal(ErrorCode::SemanticUnavailable),
    }
}

/// One answer, copied. Every count comes from the Core, never from `len()`
/// after a cut: a number recomputed here could only ever be a guess about what
/// was already thrown away.
fn context_answer(
    answer: engine::ContextResult,
    semantic: engine::SemanticStatus,
) -> ContextResult {
    ContextResult {
        status: Status::Ok,
        semantic_status: match semantic {
            engine::SemanticStatus::NotRequested => SemanticStatusView::NotRequested,
            engine::SemanticStatus::Succeeded => SemanticStatusView::Succeeded,
            engine::SemanticStatus::Unavailable => SemanticStatusView::Unavailable,
        },
        candidates: answer.candidates.iter().map(candidate_view).collect(),
        truncated: answer.truncated,
        omitted_count: answer.omitted_count,
        warnings: answer
            .warnings
            .iter()
            .map(|warning| ContextWarningView {
                code: warning_code(warning.kind),
                note_id: warning.note_id.map(|id| id.to_string()),
            })
            .collect(),
        warnings_truncated: answer.warnings_truncated,
        omitted_warning_count: answer.omitted_warning_count,
        code: None,
    }
}

fn candidate_view(candidate: &engine::Candidate) -> ContextCandidateView {
    ContextCandidateView {
        note_id: candidate.note_id.to_string(),
        label: candidate.label.clone(),
        snippet: candidate.snippet.clone(),
        updated_at: timestamp(candidate.updated_at),
        reasons: candidate.reasons.iter().copied().map(reason).collect(),
        matched_text: candidate.matched_text.clone(),
        tasks: candidate
            .tasks
            .iter()
            .map(|task| ContextTaskView {
                note_id: task.note_id.to_string(),
                task_ref: task.task_ref.clone(),
                text: task.text.clone(),
                checked: task.checked,
            })
            .collect(),
        tasks_truncated: candidate.tasks_truncated,
        omitted_task_count: candidate.omitted_task_count,
    }
}

/// Exhaustive on purpose: a reason added to the Core is a compile error here
/// rather than a variant nobody decided how to publish.
fn reason(reason: engine::Reason) -> ContextReason {
    match reason {
        engine::Reason::TextMatch => ContextReason::TextMatch,
        engine::Reason::TermMatch => ContextReason::TermMatch,
        engine::Reason::SharedTag => ContextReason::SharedTag,
        engine::Reason::PropertyMatch => ContextReason::PropertyMatch,
        engine::Reason::TaskMatch => ContextReason::TaskMatch,
        engine::Reason::SemanticMatch => ContextReason::SemanticMatch,
        engine::Reason::Recent => ContextReason::Recent,
    }
}
