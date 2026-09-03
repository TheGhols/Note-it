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

use crate::contract::{
    CommitState, ErrorCode, ListResult, NoteSummaryView, NoteView, Property, ReadResult,
    SearchHitView, SearchResult, Status, TaskState, TaskView, TasksResult, TrashEntryView,
    TrashResult, Warning, WarningCode, WriteResult,
};
use noteit_core::authority;
use noteit_core::chrono::{DateTime, SecondsFormat, Utc};
use noteit_core::revision::NoteRevision;
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
#[derive(Debug, Clone)]
pub struct Store {
    paths: StorePaths,
}

impl Store {
    /// The store the ambient environment resolves to. No directory is created.
    pub fn resolve() -> Self {
        Self {
            paths: StorePaths::resolve(),
        }
    }

    /// An explicitly named store. Used by the tests, which never point at a
    /// real one.
    pub fn at(paths: StorePaths) -> Self {
        Self { paths }
    }

    pub fn paths(&self) -> &StorePaths {
        &self.paths
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
        let revision = NoteRevision::parse(expected_revision).map_err(|error| {
            Box::new(WriteResult::refusal(
                CommitState::NotCommitted,
                ErrorCode::InvalidInput,
                error.to_string(),
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
        current_revision: None,
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

    let mut result = WriteResult::refusal(commit_state, code, error.to_string());
    if let WriteError::RevisionConflict {
        note_id,
        expected_revision,
        current_revision,
    } = error
    {
        result.note_id = Some(note_id.to_string());
        result.expected_revision = Some(expected_revision.to_string());
        result.current_revision = Some(current_revision.to_string());
        // Deliberately no `revision`: the field a caller chains the next write
        // from must never be filled in by a conflict, or "read again" becomes
        // "retry with the token the error handed you".
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
            ListResult {
                status: Status::Ok,
                count: notes.len(),
                notes,
                warnings: warnings(&batch.warnings),
                code: None,
                message: None,
            }
        }
        Err(detail) => ListResult::refusal(ErrorCode::ReadFailed, detail),
    }
}

pub fn read(off: &OffThread, store: &Store, selector: &str) -> ReadResult {
    let core = store.reader(off);
    let note_id = match core.resolve_note_id(selector) {
        Ok(note_id) => note_id,
        Err(error) => {
            let (code, message) = selector_refusal(&error);
            return ReadResult::refusal(code, message);
        }
    };
    let document = match core.read_note(&note_id) {
        Ok(document) => document,
        Err(detail) => return ReadResult::refusal(ErrorCode::ReadFailed, detail),
    };
    // A note that cannot be serialised cannot be given a version, and
    // answering without one would invite a write built on a base nobody can
    // name — which is exactly the unconditional write this server refuses to
    // offer.
    let revision = match NoteRevision::for_document(&document) {
        Ok(revision) => revision,
        Err(detail) => return ReadResult::refusal(ErrorCode::ReadFailed, detail),
    };

    ReadResult {
        status: Status::Ok,
        note: Some(note_view(&note_id, &document, &revision)),
        warnings: Vec::new(),
        code: None,
        message: None,
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
            SearchResult {
                status: Status::Ok,
                query: query.to_string(),
                count: results.len(),
                results,
                warnings: warnings(&batch.warnings),
                code: None,
                message: None,
            }
        }
        Err(detail) => SearchResult::refusal(ErrorCode::ReadFailed, detail),
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
            TasksResult {
                status: Status::Ok,
                count: tasks.len(),
                tasks,
                warnings: warnings(&batch.warnings),
                code: None,
                message: None,
            }
        }
        Err(detail) => TasksResult::refusal(ErrorCode::ReadFailed, detail),
    }
}

pub fn trash(off: &OffThread, store: &Store) -> TrashResult {
    let entries: Vec<TrashEntryView> = store
        .reader(off)
        .list_trash()
        .iter()
        .map(trash_view)
        .collect();
    TrashResult {
        status: Status::Ok,
        count: entries.len(),
        entries,
        warnings: Vec::new(),
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

fn warnings(raw: &[ReadWarning]) -> Vec<Warning> {
    raw.iter()
        .map(|warning| Warning {
            code: match warning.kind {
                ReadWarningKind::UnreadableNote => WarningCode::UnreadableNote,
                ReadWarningKind::CorruptedFrontMatter => WarningCode::CorruptedFrontMatter,
                ReadWarningKind::SymlinkRefused => WarningCode::SymlinkRefused,
                ReadWarningKind::IoError => WarningCode::IoError,
            },
            message: warning.message.clone(),
            note_id: warning.note_id.map(|id| id.to_string()),
        })
        .collect()
}

fn selector_refusal(error: &NoteSelectorError) -> (ErrorCode, String) {
    let code = match error {
        NoteSelectorError::InvalidFormat(_) | NoteSelectorError::SymlinkRefused(_) => {
            ErrorCode::InvalidInput
        }
        NoteSelectorError::NotFound(_) => ErrorCode::NotFound,
        NoteSelectorError::Ambiguous(_, _) => ErrorCode::AmbiguousSelector,
        NoteSelectorError::StoreUnavailable(_) => ErrorCode::StoreUnavailable,
    };
    (code, error.to_string())
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
