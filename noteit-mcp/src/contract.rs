//! What the MCP boundary accepts and what it answers.
//!
//! Every type here is part of a published contract, and nothing in it is
//! derived from a Core type's own `Serialize`. A field is named here on
//! purpose, so a rename inside `noteit-core` is a compile error in this file
//! rather than a silently changed schema for somebody's agent.
//!
//! ## The one rule this module exists to enforce
//!
//! **A mutation of an existing note carries a revision or it does not exist.**
//!
//! Every input struct below that names an existing note has
//! `expected_revision: String` — not `Option<String>`. That is not a style
//! choice: `Option` is exactly how an absent precondition becomes an
//! unconditional write, and an unconditional programmatic write over a note
//! somebody may be typing into is the failure the whole optimistic concurrency
//! mechanism was built to prevent. A required field in the JSON schema means
//! the request is refused by the deserializer, before this crate's code runs
//! at all, before a store is opened and before a lease is taken.
//!
//! The command line keeps its unconditional write, because a person typing
//! `noteit editar` is looking at the note. An agent is not.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================== tool names

/// Every tool this server publishes, in the order the documentation lists
/// them.
///
/// One list, used by the catalogue test and by nothing else at runtime. It is
/// here so "which tools exist" is a fact in the source rather than something
/// re-derived from whatever the router happens to hold.
pub const TOOL_NAMES: &[&str] = &[
    "noteit_append",
    "noteit_create",
    "noteit_edit",
    "noteit_list",
    "noteit_property_remove",
    "noteit_property_set",
    "noteit_read",
    "noteit_search",
    "noteit_tag_add",
    "noteit_tag_remove",
    "noteit_task_complete",
    "noteit_task_reopen",
    "noteit_tasks_list",
    "noteit_trash_list",
    "noteit_trash_restore",
];

// ============================================================== vocabulary

/// Whether a tool did what it was asked, refused, or cannot say.
///
/// `Indeterminate` is not a kind of error. It is the absence of an answer, and
/// it is kept apart from `Error` here for exactly one reason: a client that
/// treats it as a failure will repeat the request, and repeating an append
/// that may already have committed is how a paragraph lands in a note twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    Error,
    Indeterminate,
}

/// What a write result says about the bytes on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommitState {
    /// The change is on disk.
    Committed,
    /// The note already said exactly that. Nothing was written, and nothing
    /// needed to be.
    NotNeeded,
    /// Nothing was written. Repeating the request is safe.
    NotCommitted,
    /// The request went out and no answer came back. It may or may not have
    /// committed. **Never repeat this automatically.**
    Unknown,
}

/// The published name of every way a Note-it operation can refuse.
///
/// Written out rather than reused from another adapter's vocabulary: the
/// machine interface's `error.code` and this are two contracts that happen to
/// agree today, and tying them together would make one of them move because
/// the other did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The request could not be understood: an empty payload, a selector that
    /// is not one, a revision that is not sixty-four hexadecimal characters.
    InvalidInput,
    /// A domain rule refused the value.
    Validation,
    /// No note, or no trash entry, answers to that selector.
    NotFound,
    /// More than one note answers to that selector.
    AmbiguousSelector,
    /// The note moved on since it was read. **Nothing was written.** Read it
    /// again, look at what changed, and decide again.
    RevisionConflict,
    /// The task reference no longer names a task in this note.
    StaleTaskRef,
    /// The task reference matches more than one task.
    AmbiguousTaskRef,
    /// Another Note-it writer is using the store and could not be asked in
    /// time. Nothing was written; the request is safe to make again.
    WriterBusy,
    /// A Note-it instance is holding the store and could not be reached.
    /// Nothing was written, on purpose: writing anyway would mean two
    /// programs editing one note, and one of the two edits disappearing.
    AuthorityUnavailable,
    /// A restore would have replaced a live note carrying the same identifier.
    /// Neither file was changed.
    TrashTargetOccupied,
    /// The write was attempted and did not happen. The file is untouched.
    Persistence,
    /// The store itself could not be read.
    StoreUnavailable,
    /// A note could not be read or a listing could not be performed.
    ReadFailed,
    /// The request went out and the answer was lost. See [`Status::Indeterminate`].
    Indeterminate,
}

/// A non-fatal problem met while reading, reported beside the results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    UnreadableNote,
    CorruptedFrontMatter,
    SymlinkRefused,
    IoError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Warning {
    pub code: WarningCode,
    /// Diagnostic only. Never branch on it.
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
}

/// Which tasks a listing wants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    #[default]
    Pending,
    Completed,
    All,
}

// ================================================================== inputs

/// One `key = value` pair, as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Property {
    pub key: String,
    pub value: String,
}

/// The tag and property constraints a listing may carry.
///
/// Flattened into the tools that take them so an agent sends one flat object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct FilterInput {
    /// Every tag a note must carry to appear. Accents and case are folded.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Every property a note must carry to appear.
    #[serde(default)]
    pub properties: Vec<Property>,
    /// At most this many results. Clamped to the store's own bounds.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct ListInput {
    #[serde(flatten)]
    pub filter: FilterInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct ReadInput {
    /// A full note UUID, or at least eight hexadecimal characters of one.
    /// Never a path: a selector containing a separator or `..` is refused.
    pub note_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct SearchInput {
    /// The text to look for. Empty lists the most recent notes instead.
    #[serde(default)]
    pub query: String,
    #[serde(flatten)]
    pub filter: FilterInput,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct TasksListInput {
    #[serde(default)]
    pub state: TaskState,
    #[serde(flatten)]
    pub filter: FilterInput,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct CreateInput {
    /// The new note's Markdown. An empty note is a legitimate thing to ask
    /// for, and is exactly what the interface's own new note is.
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub properties: Vec<Property>,
}

/// Restoring a note from the trash.
///
/// Deliberately carries no `expected_revision`. A restore is a *move*, not an
/// edit: there is no live note whose version a caller could have read, and
/// inventing a precondition to make the API look uniform would be a field that
/// names nothing. The identity guarantee it does have is the one that matters
/// — a restore that would land on a live note carrying the same identifier is
/// refused and neither file is touched.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct TrashRestoreInput {
    /// A full note UUID, or at least eight hexadecimal characters of one,
    /// resolved against the trash and never against the live notes.
    pub note_id: String,
}

/// The precondition every mutation of an existing note must carry.
///
/// Sixty-four lowercase hexadecimal characters, exactly as `noteit_read`
/// published them. A malformed one is refused; an absent one does not
/// deserialize at all.
pub type RevisionArgument = String;

macro_rules! mutation_input {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$field_meta:meta])* $field:ident : $ty:ty ),* $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
        pub struct $name {
            /// A full note UUID, or at least eight hexadecimal characters of
            /// one. Never a path.
            pub note_id: String,
            $( $(#[$field_meta])* pub $field: $ty, )*
            /// **Required.** The revision `noteit_read` gave for the note this
            /// change was decided from, as sixty-four lowercase hexadecimal
            /// characters.
            ///
            /// If the note has moved on since, this write is refused with
            /// `revision_conflict` and nothing is changed. Read the note
            /// again, look at what it says now, and decide again — never send
            /// the `current_revision` the conflict returned, which would write
            /// over a change nobody has looked at.
            pub expected_revision: RevisionArgument,
        }
    };
}

mutation_input! {
    /// Adds Markdown to the end of a note's body.
    AppendInput {
        /// The Markdown to add. Never trimmed, reflowed or reindented.
        text: String,
    }
}

mutation_input! {
    /// Replaces a note's whole body, or empties it.
    EditInput {
        /// The new body. Required unless `clear` is true, and refused
        /// alongside it.
        #[serde(default)]
        body: Option<String>,
        /// Empties the note. Asked for by name and never by accident: an
        /// empty body is a mistake far more often than it is an instruction.
        #[serde(default)]
        clear: bool,
    }
}

mutation_input! {
    /// Adds a tag to a note.
    TagAddInput {
        /// The tag, with or without its leading `#`.
        tag: String,
    }
}

mutation_input! {
    /// Removes a tag from a note.
    TagRemoveInput {
        tag: String,
    }
}

mutation_input! {
    /// Sets a property on a note, adding it or replacing its value.
    PropertySetInput {
        key: String,
        value: String,
    }
}

mutation_input! {
    /// Removes a property from a note.
    PropertyRemoveInput {
        key: String,
    }
}

mutation_input! {
    /// Marks one Markdown task in a note as done.
    TaskCompleteInput {
        /// The reference `noteit_tasks_list` gave for this task. It names the
        /// task *in the note as it was then*, and stops matching as soon as
        /// the task itself changes.
        task_ref: String,
    }
}

mutation_input! {
    /// Marks one Markdown task in a note as not done.
    TaskReopenInput {
        task_ref: String,
    }
}

// ================================================================= outputs

/// One note, in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct NoteView {
    pub note_id: String,
    /// Derived from the note's first line. Never written to the file, and
    /// never an identity: nothing addresses a note by its label.
    pub label: String,
    /// The Markdown exactly as the Core holds it, unsanitized. JSON escaping
    /// is what makes a control character safe in a document nobody is
    /// rendering as terminal text.
    pub content: String,
    pub tags: Vec<String>,
    pub properties: Vec<Property>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// The version this response describes. Send it back as
    /// `expected_revision` to write on top of exactly this note.
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct NoteSummaryView {
    pub note_id: String,
    pub label: String,
    pub snippet: String,
    pub tags: Vec<String>,
    pub properties: Vec<Property>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchHitView {
    pub note_id: String,
    pub label: String,
    pub snippet: String,
    pub match_count: usize,
    /// The first occurrence as the note spells it.
    pub matched_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TaskView {
    /// The reference `noteit_task_complete` and `noteit_task_reopen` name this
    /// task by.
    pub task_ref: String,
    pub note_id: String,
    pub note_label: String,
    pub text: String,
    pub checked: bool,
    pub completed_at: Option<String>,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TrashEntryView {
    pub note_id: String,
    pub label: String,
    pub snippet: String,
    pub deleted_at: Option<String>,
}

/// What every tool that changes the store answers with.
///
/// One shape for creation, every mutation and the restore, so a client writes
/// one branch and not eleven. Every decision it has to make is a typed field:
///
/// ```text
/// did it work?          status
/// is it on disk?        commit_state
/// did anything change?  changed
/// what do I send next?  revision
/// why did it refuse?    code
/// ```
///
/// `message` is for a person reading a log. Nothing programmatic may depend on
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct WriteResult {
    pub status: Status,
    /// Always present, on success and on refusal alike. It is the field that
    /// answers "may I repeat this?", and a refusal that did not carry it would
    /// force a client to guess.
    pub commit_state: CommitState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_id: Option<String>,
    /// Whether anything was actually written. `false` with
    /// `commit_state = not_needed` is a success: the note already said exactly
    /// that.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
    /// The note's revision after this operation, so the next conditional write
    /// needs no extra read. Absent for a restore, which does not describe one
    /// note's new version, and absent on every refusal except a conflict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<ErrorCode>,
    /// Diagnostic only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// On `revision_conflict` only: the precondition that was sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<String>,
    /// On `revision_conflict` only: the revision the note actually has now.
    ///
    /// Enough to notice the note moved, and deliberately not enough to retry:
    /// the new content is not here, because a client that has not looked at
    /// what changed has no business writing over it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<String>,
    /// Present only when the note was committed and an open window could not
    /// be brought into step with it. **Not a failure.** The file on disk holds
    /// the new content; repeating the operation would append twice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_sync_warning: Option<String>,
}

impl WriteResult {
    pub fn refusal(commit_state: CommitState, code: ErrorCode, message: String) -> Self {
        Self {
            status: if matches!(commit_state, CommitState::Unknown) {
                Status::Indeterminate
            } else {
                Status::Error
            },
            commit_state,
            note_id: None,
            changed: None,
            revision: None,
            code: Some(code),
            message: Some(message),
            expected_revision: None,
            current_revision: None,
            ui_sync_warning: None,
        }
    }
}

macro_rules! read_result {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$field_meta:meta])* $field:ident : $ty:ty = $empty:expr ),* $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
        pub struct $name {
            pub status: Status,
            $( $(#[$field_meta])* pub $field: $ty, )*
            /// Notes that could not be read, reported beside the ones that
            /// could. A store with one damaged file still answers.
            pub warnings: Vec<Warning>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub code: Option<ErrorCode>,
            /// Diagnostic only.
            #[serde(skip_serializing_if = "Option::is_none")]
            pub message: Option<String>,
        }

        impl $name {
            pub fn refusal(code: ErrorCode, message: String) -> Self {
                Self {
                    status: Status::Error,
                    $( $field: $empty, )*
                    warnings: Vec::new(),
                    code: Some(code),
                    message: Some(message),
                }
            }
        }
    };
}

read_result! {
    /// The answer to `noteit_list`.
    ListResult {
        notes: Vec<NoteSummaryView> = Vec::new(),
        count: usize = 0,
    }
}

read_result! {
    /// The answer to `noteit_read`.
    ReadResult {
        /// Absent only on a refusal.
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<NoteView> = None,
    }
}

read_result! {
    /// The answer to `noteit_search`.
    SearchResult {
        query: String = String::new(),
        results: Vec<SearchHitView> = Vec::new(),
        count: usize = 0,
    }
}

read_result! {
    /// The answer to `noteit_tasks_list`.
    TasksResult {
        tasks: Vec<TaskView> = Vec::new(),
        count: usize = 0,
    }
}

read_result! {
    /// The answer to `noteit_trash_list`.
    TrashResult {
        entries: Vec<TrashEntryView> = Vec::new(),
        count: usize = 0,
    }
}
