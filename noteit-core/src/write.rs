//! Every mutation Note-it can be asked to make, as domain operations.
//!
//! There is one implementation of each, and both adapters run it. The desktop
//! application reaches it holding the writer lease and, when the note is open,
//! holding the editor still; the CLI reaches it either directly or through the
//! desktop application. What the operation *does* is the same in every case —
//! there is no "append for the CLI" and no "append for the interface", because
//! two of those would eventually disagree about something and one of them
//! would be wrong.
//!
//! ## The document is an argument
//!
//! [`apply`] takes the note it is mutating rather than loading it, and that is
//! the point of the whole design. A note that is open in a window may hold
//! text the file does not have yet, so the base a mutation is applied to has
//! to be decided by the caller that knows: the CLI writing on its own passes
//! what it read from disk; the desktop application passes what the editor
//! actually holds, captured after the editor was frozen. Loading inside the
//! operation would make the second case impossible to write correctly, and
//! silently overwrite whatever had not been saved yet.
//!
//! ## Nothing is rewritten for nothing
//!
//! Every operation answers `None` when it changes nothing: a tag that is
//! already there, a property already carrying that value, a task already
//! completed. The note is then not written at all, so no timestamp moves and
//! no backup is taken. That is what makes these operations safe to repeat.
//!
//! ## Which timestamps move
//!
//! - Text changes — append, edit, clear, completing and reopening a task —
//!   move `updated_at`, and only when the body really changed.
//! - Tags and properties move neither timestamp. They are what the note is
//!   *about*, not an edit of it, and the file being reserialized to record one
//!   is bookkeeping rather than a change the reader made.
//! - `created_at` never moves. Nothing here can write it.

use crate::filter::NoteSelectorError;
use crate::metadata::{semantic_identity, NoteMetadata, NoteProperty, NoteTags};
use crate::model::NoteDocument;
use crate::revision::NoteRevision;
use crate::task::{self, TaskRef, TaskRefError};
use crate::trash::RestoreError;
use crate::NoteItCore;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Everything a mutation can fail with, told apart because the callers say
/// different things about them and a caller must never have to read an error
/// message to decide what happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WriteError {
    /// The request could not be understood: an empty payload, a malformed
    /// selector, a task reference that is not one.
    InvalidInput { detail: String },
    /// No note, or no trash entry, answers to that selector.
    NotFound { selector: String },
    /// More than one note answers to that selector.
    AmbiguousSelector { selector: String, matches: usize },
    /// A domain rule refused the value: too many tags, a control character in
    /// a property key, a limit exceeded.
    Validation { detail: String },
    /// The task reference no longer names a task in this note. The note
    /// changed between listing and writing, and acting anyway would mean
    /// completing something the person never looked at.
    StaleTaskRef { task_ref: String },
    /// The reference matches more than one task in the note. Vanishingly rare
    /// and never resolved by guessing.
    AmbiguousTaskRef { task_ref: String },
    /// Another Note-it writer holds the store and could not be asked to do it.
    WriterBusy { detail: String },
    /// The store is held and the authority that holds it could not be reached.
    AuthorityUnavailable { detail: String },
    /// The request went out and no answer came back.
    ///
    /// The one outcome that is neither success nor failure. The authority may
    /// have committed the change before the connection broke, and there is no
    /// way to tell from here — so this is reported as unknown and never
    /// retried automatically. Sending an append again on the strength of a
    /// dropped socket is how a paragraph ends up in a note twice.
    Indeterminate { detail: String },
    /// A restore would have replaced a live note carrying the same identifier.
    /// Neither file was changed.
    TrashTargetOccupied { note_id: Uuid },
    /// The write was attempted and did not happen. The file on disk is
    /// untouched and repeating the operation is safe.
    Persistence { detail: String },
    /// The store itself could not be read.
    StoreUnavailable { detail: String },
    /// The note moved on since the caller read it.
    ///
    /// Its own variant rather than a `Validation` or a `Persistence`, because
    /// it is the one refusal a caller can act on without a person: the note is
    /// fine, the store is fine, and the only thing wrong is that the base this
    /// write was built from is no longer the note. The current revision travels
    /// with it so the caller can re-read, reconcile and decide — deliberately
    /// *not* so it can retry with the new token, which would be the silent
    /// overwrite this whole mechanism exists to stop.
    RevisionConflict {
        note_id: Uuid,
        expected_revision: NoteRevision,
        current_revision: NoteRevision,
    },
}

impl fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { detail } | Self::Validation { detail } => {
                formatter.write_str(detail)
            }
            Self::NotFound { selector } => write!(formatter, "no note matches `{selector}`"),
            Self::AmbiguousSelector { selector, matches } => {
                write!(formatter, "`{selector}` matches {matches} notes")
            }
            Self::StaleTaskRef { task_ref } => {
                write!(formatter, "the task reference `{task_ref}` is out of date")
            }
            Self::AmbiguousTaskRef { task_ref } => {
                write!(formatter, "`{task_ref}` matches more than one task")
            }
            Self::WriterBusy { detail }
            | Self::AuthorityUnavailable { detail }
            | Self::Indeterminate { detail } => formatter.write_str(detail),
            Self::TrashTargetOccupied { note_id } => {
                write!(formatter, "a live note already carries {note_id}")
            }
            Self::Persistence { detail } | Self::StoreUnavailable { detail } => {
                formatter.write_str(detail)
            }
            Self::RevisionConflict { note_id, .. } => write!(
                formatter,
                "a nota {note_id} mudou desde a leitura e nada foi gravado"
            ),
        }
    }
}

impl std::error::Error for WriteError {}

impl From<NoteSelectorError> for WriteError {
    fn from(error: NoteSelectorError) -> Self {
        match error {
            NoteSelectorError::InvalidFormat(selector) => Self::InvalidInput {
                detail: format!(
                    "`{selector}` is not a note selector; give a full UUID or at \
                     least eight hexadecimal characters"
                ),
            },
            NoteSelectorError::NotFound(selector) => Self::NotFound { selector },
            NoteSelectorError::Ambiguous(selector, matches) => Self::AmbiguousSelector {
                selector,
                matches: matches.len(),
            },
            NoteSelectorError::SymlinkRefused(selector) => Self::InvalidInput {
                detail: format!("`{selector}` is a symbolic link"),
            },
            NoteSelectorError::StoreUnavailable(detail) => Self::StoreUnavailable { detail },
        }
    }
}

impl From<TaskRefError> for WriteError {
    fn from(error: TaskRefError) -> Self {
        Self::InvalidInput {
            detail: error.to_string(),
        }
    }
}

/// What kind of change was made, so an adapter can say the right sentence
/// without inspecting the note again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteOutcomeKind {
    NoteCreated,
    ContentAppended,
    ContentReplaced,
    ContentCleared,
    TagAdded,
    TagRemoved,
    PropertySet,
    PropertyRemoved,
    TaskCompleted,
    TaskReopened,
    NoteRestored,
}

/// A mutation that happened. Only ever built after the write committed.
///
/// `changed` tells apart the two successful outcomes that look alike from
/// outside: something was written, or the note already said exactly that and
/// nothing was touched at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteOutcome {
    pub note_id: Uuid,
    pub kind: WriteOutcomeKind,
    pub changed: bool,
    /// Present only when the note was committed but the open window could not
    /// be brought back into step with it.
    ///
    /// This is not a failure and must never be reported as one. The file on
    /// disk holds the new content; a caller told "it failed" would repeat the
    /// operation and append the same text twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_sync_warning: Option<String>,
    /// The note's revision now that this operation is over.
    ///
    /// For a write that changed something it is the revision of the document
    /// that was persisted; for one that changed nothing it is the revision the
    /// note already had. Either way it lets a caller chain another conditional
    /// write without reading the note again.
    ///
    /// `None` only where the operation does not describe one note's new
    /// version — a restore from the trash, which is a move rather than an edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<NoteRevision>,
}

impl WriteOutcome {
    pub fn new(note_id: Uuid, kind: WriteOutcomeKind, changed: bool) -> Self {
        Self {
            note_id,
            kind,
            changed,
            ui_sync_warning: None,
            revision: None,
        }
    }

    /// The same outcome, carrying the note's revision after the operation.
    pub fn with_revision(mut self, revision: NoteRevision) -> Self {
        self.revision = Some(revision);
        self
    }

    pub fn with_ui_sync_warning(mut self, warning: impl Into<String>) -> Self {
        self.ui_sync_warning = Some(warning.into());
        self
    }
}

/// A change to one existing note's document.
///
/// Deliberately separate from [`WriteOperation`]: this is the part that is
/// pure, that takes the document as it really is right now, and that both
/// adapters run over their own base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum NoteMutation {
    /// Adds Markdown to the end of the body.
    Append {
        payload: String,
    },
    /// Replaces the whole body. Refuses an empty payload: clearing a note is
    /// [`NoteMutation::ClearBody`] and has to be asked for by name.
    ReplaceBody {
        body: String,
    },
    /// Empties the body, on purpose.
    ClearBody,
    AddTag {
        tag: String,
    },
    RemoveTag {
        tag: String,
    },
    SetProperty {
        key: String,
        value: String,
    },
    RemoveProperty {
        key: String,
    },
    CompleteTask {
        task_ref: String,
    },
    ReopenTask {
        task_ref: String,
    },
}

impl NoteMutation {
    /// Which kind of outcome this mutation produces when it succeeds.
    ///
    /// Public because the desktop adapter runs the mutation itself — against
    /// the editor's live text rather than the file — and has to name the same
    /// outcome the direct path would. Two tables of this would drift.
    pub fn outcome_kind(&self) -> WriteOutcomeKind {
        match self {
            Self::Append { .. } => WriteOutcomeKind::ContentAppended,
            Self::ReplaceBody { .. } => WriteOutcomeKind::ContentReplaced,
            Self::ClearBody => WriteOutcomeKind::ContentCleared,
            Self::AddTag { .. } => WriteOutcomeKind::TagAdded,
            Self::RemoveTag { .. } => WriteOutcomeKind::TagRemoved,
            Self::SetProperty { .. } => WriteOutcomeKind::PropertySet,
            Self::RemoveProperty { .. } => WriteOutcomeKind::PropertyRemoved,
            Self::CompleteTask { .. } => WriteOutcomeKind::TaskCompleted,
            Self::ReopenTask { .. } => WriteOutcomeKind::TaskReopened,
        }
    }
}

/// What a new note starts as.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteDraft {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub properties: Vec<NoteProperty>,
}

/// One complete request to change the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum WriteOperation {
    CreateNote {
        draft: NoteDraft,
    },
    /// A change to a live note, named the way a person names one.
    ///
    /// The selector travels, never a path: resolving it is the authority's
    /// job, and [`NoteItCore::resolve_note_id`] refuses anything that is not a
    /// UUID or a hexadecimal prefix of one.
    MutateNote {
        selector: String,
        mutation: NoteMutation,
        /// The revision the caller built this mutation from, when it has one.
        ///
        /// `None` is an unconditional write and stays exactly what it always
        /// was: last writer wins, which is what a person typing `noteit editar`
        /// is asking for. A programmatic client that read the note first must
        /// send the revision it read, or it is racing every other writer.
        ///
        /// `default` on purpose: an authority built before this field existed
        /// still decodes a request that carries it, and one that does not
        /// carry it still decodes here.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<NoteRevision>,
    },
    RestoreFromTrash {
        selector: String,
    },
}

/// Applies a mutation to a document, answering `None` when it changes nothing.
///
/// Pure: no file is read and none is written. Everything about *which* note
/// and *whether it may be written* has already been decided by the caller.
pub fn apply(
    document: &NoteDocument,
    mutation: &NoteMutation,
) -> Result<Option<NoteDocument>, WriteError> {
    match mutation {
        NoteMutation::Append { payload } => {
            if payload.is_empty() {
                return Err(WriteError::InvalidInput {
                    detail: "there is nothing to append".to_string(),
                });
            }
            Ok(replace_body(
                document,
                &join_for_append(&document.content, payload),
            ))
        }
        NoteMutation::ReplaceBody { body } => {
            if NoteDocument::canonical_content(body).is_empty() {
                return Err(WriteError::InvalidInput {
                    detail: "replacing a note with nothing has to be asked for explicitly"
                        .to_string(),
                });
            }
            Ok(replace_body(document, body))
        }
        NoteMutation::ClearBody => Ok(replace_body(document, "")),
        NoteMutation::AddTag { tag } => add_tag(document, tag),
        NoteMutation::RemoveTag { tag } => remove_tag(document, tag),
        NoteMutation::SetProperty { key, value } => set_property(document, key, value),
        NoteMutation::RemoveProperty { key } => remove_property(document, key),
        NoteMutation::CompleteTask { task_ref } => set_task_state(document, task_ref, true),
        NoteMutation::ReopenTask { task_ref } => set_task_state(document, task_ref, false),
    }
}

/// How a payload is joined to the body it is appended to.
///
/// One rule, written down so both adapters and every test mean the same thing:
///
/// - an empty body becomes the payload exactly;
/// - a body that already ends in a line break takes the payload straight on
///   the end;
/// - a body that does not gets exactly one line break inserted first.
///
/// The stored form of a note never keeps its trailing line breaks — see
/// [`NoteDocument::canonical_content`] — so in practice it is always the third
/// case, and appending to `ABC` gives `ABC\nXYZ`. The payload itself is not
/// trimmed, reflowed or reindented: what was handed in is what goes in, and
/// the document's own canonical form has the last word on the result.
pub fn join_for_append(body: &str, payload: &str) -> String {
    if body.is_empty() {
        return payload.to_string();
    }
    if body.ends_with('\n') {
        return format!("{body}{payload}");
    }
    format!("{body}\n{payload}")
}

fn replace_body(document: &NoteDocument, body: &str) -> Option<NoteDocument> {
    let canonical = NoteDocument::canonical_content(body);
    if document.content == canonical {
        return None;
    }
    let mut candidate = document.clone();
    candidate.content = canonical.to_string();
    candidate.touch_content_modified();
    Some(candidate)
}

fn add_tag(document: &NoteDocument, tag: &str) -> Result<Option<NoteDocument>, WriteError> {
    // Validated on its own first, so `#` handling, trimming and the length
    // limit are the same rules the interface applies, and a rejected tag never
    // half-changes the note.
    let normalized =
        NoteTags::try_new([tag.to_string()]).map_err(|error| WriteError::Validation {
            detail: error.to_string(),
        })?;
    let Some(display) = normalized.as_slice().first().cloned() else {
        return Err(WriteError::Validation {
            detail: "a tag cannot be empty".to_string(),
        });
    };

    let identity = semantic_identity(&display);
    let existing = document.user_metadata.tags.as_slice();
    if existing
        .iter()
        .any(|tag| semantic_identity(tag) == identity)
    {
        return Ok(None);
    }

    let mut tags: Vec<String> = existing.to_vec();
    tags.push(display);
    Ok(Some(with_metadata(
        document,
        tags,
        current_properties(document),
    )?))
}

fn remove_tag(document: &NoteDocument, tag: &str) -> Result<Option<NoteDocument>, WriteError> {
    let identity = semantic_identity(tag.trim().strip_prefix('#').unwrap_or(tag.trim()));
    if identity.is_empty() {
        return Err(WriteError::Validation {
            detail: "a tag cannot be empty".to_string(),
        });
    }
    let remaining: Vec<String> = document
        .user_metadata
        .tags
        .as_slice()
        .iter()
        .filter(|existing| semantic_identity(existing) != identity)
        .cloned()
        .collect();
    if remaining.len() == document.user_metadata.tags.as_slice().len() {
        return Ok(None);
    }
    Ok(Some(with_metadata(
        document,
        remaining,
        current_properties(document),
    )?))
}

fn set_property(
    document: &NoteDocument,
    key: &str,
    value: &str,
) -> Result<Option<NoteDocument>, WriteError> {
    let identity = semantic_identity(key.trim());
    if identity.is_empty() {
        return Err(WriteError::Validation {
            detail: "a property key cannot be empty".to_string(),
        });
    }

    let mut properties = current_properties(document);
    let mut replaced = false;
    for property in properties.iter_mut() {
        if semantic_identity(&property.key) == identity {
            if property.value == value {
                return Ok(None);
            }
            // The stored spelling of the key is kept. Setting a value is a
            // change of value; renaming the key by capitalising it differently
            // would rewrite bytes nobody asked about.
            property.value = value.to_string();
            replaced = true;
            break;
        }
    }
    if !replaced {
        properties.push(NoteProperty {
            key: key.trim().to_string(),
            value: value.to_string(),
        });
    }

    Ok(Some(with_metadata(
        document,
        current_tags(document),
        properties,
    )?))
}

fn remove_property(document: &NoteDocument, key: &str) -> Result<Option<NoteDocument>, WriteError> {
    let identity = semantic_identity(key.trim());
    if identity.is_empty() {
        return Err(WriteError::Validation {
            detail: "a property key cannot be empty".to_string(),
        });
    }
    let properties = current_properties(document);
    let remaining: Vec<NoteProperty> = properties
        .iter()
        .filter(|property| semantic_identity(&property.key) != identity)
        .cloned()
        .collect();
    if remaining.len() == properties.len() {
        return Ok(None);
    }
    Ok(Some(with_metadata(
        document,
        current_tags(document),
        remaining,
    )?))
}

fn current_tags(document: &NoteDocument) -> Vec<String> {
    document.user_metadata.tags.as_slice().to_vec()
}

fn current_properties(document: &NoteDocument) -> Vec<NoteProperty> {
    document.user_metadata.properties.as_slice().to_vec()
}

/// Rebuilds the note with new semantic metadata and neither timestamp touched.
fn with_metadata(
    document: &NoteDocument,
    tags: Vec<String>,
    properties: Vec<NoteProperty>,
) -> Result<NoteDocument, WriteError> {
    let metadata =
        NoteMetadata::try_new(tags, properties).map_err(|error| WriteError::Validation {
            detail: error.to_string(),
        })?;
    let mut candidate = document.clone();
    candidate.user_metadata = metadata;
    Ok(candidate)
}

/// Completes or reopens exactly the task the reference names.
fn set_task_state(
    document: &NoteDocument,
    task_ref: &str,
    complete: bool,
) -> Result<Option<NoteDocument>, WriteError> {
    let wanted = TaskRef::parse(task_ref)?;
    let line_index = task::resolve_task_ref(document.metadata.id, &document.content, &wanted)
        .map_err(|error| match error {
            task::TaskResolution::Stale => WriteError::StaleTaskRef {
                task_ref: wanted.as_str().to_string(),
            },
            task::TaskResolution::Ambiguous => WriteError::AmbiguousTaskRef {
                task_ref: wanted.as_str().to_string(),
            },
        })?;

    let completed_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let Some(rewritten) =
        task::rewrite_task_line(&document.content, line_index, complete, &completed_at)
    else {
        // Already in the state that was asked for: nothing to write, no
        // timestamp to move, and a success rather than an error.
        return Ok(None);
    };

    Ok(replace_body(document, &rewritten))
}

/// What a mutation over unsaved editor text works out to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveMutation {
    /// The document to write, or `None` when nothing needs writing at all.
    pub candidate: Option<NoteDocument>,
    /// Whether the mutation itself changed anything.
    ///
    /// Separate from whether anything gets written, because the two really do
    /// come apart: adding a tag a note already has changes nothing, and the
    /// note may still have to be written to keep the paragraph the editor was
    /// holding.
    pub mutation_changed: bool,
    /// Whether the editor was holding text the file did not have.
    pub adopted_unsaved_text: bool,
    /// The revision of the base this mutation was checked and applied against.
    ///
    /// The note as the window really holds it, unsaved text included — which is
    /// the version a caller's precondition was compared with, and the version
    /// to report back when nothing ends up being written.
    pub base_revision: NoteRevision,
}

/// Applies a mutation on top of text an editor is holding but has not saved.
///
/// **This is the answer to the failure that makes the whole phase necessary.**
/// A note open in a window may hold a paragraph that is not on disk yet.
/// Loading that note from the file, appending to it and writing it back would
/// store the file's version plus the new text — and the paragraph would be
/// gone, silently, with nothing failing and nothing to notice.
///
/// So the base is not the file. It is the note as the host last committed it,
/// with the editor's live text put back in first; the mutation is applied to
/// *that*, and the result is one write carrying both.
///
/// Two rules follow from it and both matter:
///
/// - the unsaved text moves `updated_at`, because it is a real edit somebody
///   made, and it does so once — the mutation does not move it a second time;
/// - a mutation that changes nothing still produces a write when the editor
///   was holding text, because "you already have that tag" must never be the
///   reason a paragraph is thrown away.
pub fn apply_over_live_body(
    committed: &NoteDocument,
    live_body: &str,
    mutation: &NoteMutation,
    expected_revision: &Option<NoteRevision>,
) -> Result<LiveMutation, WriteError> {
    let mut base = committed.clone();
    let live = NoteDocument::canonical_content(live_body);
    let adopted_unsaved_text = base.content != live;
    if adopted_unsaved_text {
        base.content = live.to_string();
        base.touch_content_modified();
    }

    // Checked here, on the folded base, and before a single mutation is
    // applied. This is the case the file on disk cannot answer: a client read
    // the note, somebody typed a paragraph into the open window that has not
    // been autosaved yet, and the client's write is built on a note that no
    // longer describes what the person is looking at. Comparing against the
    // file would say "unchanged" and let that paragraph be overwritten.
    let base_revision = ensure_revision_matches(&base.metadata.id, &base, expected_revision)?;

    let mutated = apply(&base, mutation)?;
    let mutation_changed = mutated.is_some();
    let candidate = match (mutated, adopted_unsaved_text) {
        (Some(candidate), _) => Some(candidate),
        (None, true) => Some(base),
        (None, false) => None,
    };

    Ok(LiveMutation {
        candidate,
        mutation_changed,
        adopted_unsaved_text,
        base_revision,
    })
}

/// The revision of a document, as a write error rather than a string.
///
/// A document that cannot be serialised cannot be hashed and could not have
/// been written either, so this is a persistence failure and never a revision
/// that happens to be missing.
pub fn revision_of(document: &NoteDocument) -> Result<NoteRevision, WriteError> {
    NoteRevision::for_document(document).map_err(|detail| WriteError::Persistence { detail })
}

/// Checks a caller's precondition against the base a mutation will be applied
/// to, and answers that base's revision.
///
/// Both adapters call this, with their own base: the direct path passes the
/// document it loaded from disk, and the desktop passes the document it is
/// really going to mutate — the committed note with the editor's unsaved text
/// already folded in. Comparing against anything else would check a version
/// nobody is about to overwrite.
///
/// No precondition is not a failure: an unconditional write is a supported
/// request and this returns the current revision so the caller still learns
/// where the note ended up.
pub fn ensure_revision_matches(
    note_id: &Uuid,
    base: &NoteDocument,
    expected: &Option<NoteRevision>,
) -> Result<NoteRevision, WriteError> {
    let current = revision_of(base)?;
    match expected {
        Some(expected) if *expected != current => Err(WriteError::RevisionConflict {
            note_id: *note_id,
            expected_revision: expected.clone(),
            current_revision: current,
        }),
        _ => Ok(current),
    }
}

/// Runs one whole operation against a store this process is entitled to write.
///
/// The caller must already hold the writer lease. Nothing here takes it:
/// making the operation acquire it would hide the one decision — direct write
/// or ask the authority — that the adapter has to make consciously.
pub fn execute(core: &NoteItCore, operation: &WriteOperation) -> Result<WriteOutcome, WriteError> {
    match operation {
        WriteOperation::CreateNote { draft } => create_note(core, draft),
        WriteOperation::MutateNote {
            selector,
            mutation,
            expected_revision,
        } => {
            let note_id = core.resolve_note_id(selector)?;
            let document = core
                .read_note(&note_id)
                .map_err(|detail| WriteError::StoreUnavailable { detail })?;
            // Against this document and no other. The base the precondition is
            // checked on has to be the base the mutation is applied to, or the
            // check is about a note nobody is writing.
            let base_revision = ensure_revision_matches(&note_id, &document, expected_revision)?;
            let outcome_kind = mutation.outcome_kind();
            match apply(&document, mutation)? {
                None => Ok(
                    WriteOutcome::new(note_id, outcome_kind, false).with_revision(base_revision)
                ),
                Some(candidate) => {
                    commit_addressed(core, &note_id, &candidate)?;
                    let committed_revision = revision_of(&candidate)?;
                    Ok(WriteOutcome::new(note_id, outcome_kind, true)
                        .with_revision(committed_revision))
                }
            }
        }
        WriteOperation::RestoreFromTrash { selector } => {
            let note_id = core.resolve_trash_id(selector)?;
            match core.storage().restore_note_from_trash(&note_id) {
                Ok(()) => Ok(WriteOutcome::new(
                    note_id,
                    WriteOutcomeKind::NoteRestored,
                    true,
                )),
                Err(RestoreError::Occupied) => Err(WriteError::TrashTargetOccupied { note_id }),
                Err(RestoreError::Missing) => Err(WriteError::NotFound {
                    selector: selector.clone(),
                }),
                Err(RestoreError::Failed(detail)) => Err(WriteError::Persistence { detail }),
            }
        }
    }
}

/// Builds and writes a new note.
pub fn create_note(core: &NoteItCore, draft: &NoteDraft) -> Result<WriteOutcome, WriteError> {
    let mut document = NoteDocument::new_empty();
    document.content = NoteDocument::canonical_content(&draft.content).to_string();
    document.user_metadata = NoteMetadata::try_new(draft.tags.clone(), draft.properties.clone())
        .map_err(|error| WriteError::Validation {
            detail: error.to_string(),
        })?;
    commit(core, &document)?;
    let revision = revision_of(&document)?;
    Ok(
        WriteOutcome::new(document.metadata.id, WriteOutcomeKind::NoteCreated, true)
            .with_revision(revision),
    )
}

/// Commits a document to disk, ensuring that the addressed note ID matches the document ID.
///
/// Refuses to write if the addressed note ID does not match the document metadata ID,
/// enforcing defense-in-depth against silent write redirection or identity confusion.
pub fn commit_addressed(
    core: &NoteItCore,
    addressed_id: &Uuid,
    document: &NoteDocument,
) -> Result<(), WriteError> {
    if document.metadata.id != *addressed_id {
        return Err(WriteError::Persistence {
            detail: format!(
                "identity mismatch: addressed note {addressed_id} cannot be written to {}",
                document.metadata.id
            ),
        });
    }
    core.storage()
        .save_note_atomic_with_id(addressed_id, document)
        .map(|_| ())
        .map_err(|detail| WriteError::Persistence { detail })
}

/// The one way a mutation reaches the disk.
///
/// Straight through the canonical atomic writer, so the backup that precedes a
/// day's first change, the temp file, the rename that is the commit point and
/// the directory sync that follows it all behave exactly as they do for an
/// edit made in a window. There is deliberately no second write path here to
/// keep in step with that one.
pub fn commit(core: &NoteItCore, document: &NoteDocument) -> Result<(), WriteError> {
    commit_addressed(core, &document.metadata.id, document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(body: &str) -> NoteDocument {
        let mut document = NoteDocument::new_empty();
        document.content = body.to_string();
        document
    }

    #[test]
    fn appending_puts_one_line_break_between_the_body_and_the_payload() {
        assert_eq!(join_for_append("", "XYZ"), "XYZ");
        assert_eq!(join_for_append("ABC", "XYZ"), "ABC\nXYZ");
        assert_eq!(join_for_append("ABC\n", "XYZ"), "ABC\nXYZ");
    }

    #[test]
    fn appending_never_reflows_the_payload() {
        let document = note("ABC");
        let candidate = apply(
            &document,
            &NoteMutation::Append {
                payload: "  espaçado  \n\n- item".to_string(),
            },
        )
        .expect("append")
        .expect("changed");
        assert_eq!(candidate.content, "ABC\n  espaçado  \n\n- item");
    }

    #[test]
    fn appending_nothing_is_a_usage_error_rather_than_a_silent_write() {
        let document = note("ABC");
        let error = apply(
            &document,
            &NoteMutation::Append {
                payload: String::new(),
            },
        )
        .expect_err("an empty payload must be refused");
        assert!(
            matches!(error, WriteError::InvalidInput { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn appending_only_line_breaks_changes_nothing() {
        // Canonicalisation takes them straight back off, so the note is
        // already exactly this and nothing is rewritten.
        let document = note("ABC");
        assert!(apply(
            &document,
            &NoteMutation::Append {
                payload: "\n".to_string()
            }
        )
        .expect("append")
        .is_none());
    }

    #[test]
    fn replacing_the_body_moves_updated_at_and_never_created_at() {
        let document = note("antes");
        let candidate = apply(
            &document,
            &NoteMutation::ReplaceBody {
                body: "depois".to_string(),
            },
        )
        .expect("replace")
        .expect("changed");
        assert_eq!(candidate.content, "depois");
        assert_eq!(candidate.metadata.created_at, document.metadata.created_at);
        assert!(candidate.metadata.updated_at >= document.metadata.updated_at);
    }

    #[test]
    fn replacing_a_body_with_the_same_text_writes_nothing() {
        let document = note("igual");
        assert!(apply(
            &document,
            &NoteMutation::ReplaceBody {
                body: "igual\n".to_string()
            }
        )
        .expect("replace")
        .is_none());
    }

    #[test]
    fn emptying_a_note_has_to_be_asked_for_by_name() {
        let document = note("valioso");
        let error = apply(
            &document,
            &NoteMutation::ReplaceBody {
                body: String::new(),
            },
        )
        .expect_err("an accidental empty pipe must not wipe a note");
        assert!(
            matches!(error, WriteError::InvalidInput { .. }),
            "{error:?}"
        );

        let cleared = apply(&document, &NoteMutation::ClearBody)
            .expect("clear")
            .expect("changed");
        assert_eq!(cleared.content, "");
    }

    #[test]
    fn a_tag_is_added_once_and_repeating_it_changes_nothing() {
        let document = note("corpo");
        let tagged = apply(
            &document,
            &NoteMutation::AddTag {
                tag: "Medicina".into(),
            },
        )
        .expect("add")
        .expect("changed");
        assert_eq!(tagged.user_metadata.tags.as_slice(), ["Medicina"]);

        // Identity is case- and accent-insensitive, so this is the same tag.
        assert!(apply(
            &tagged,
            &NoteMutation::AddTag {
                tag: "medicina".into()
            }
        )
        .expect("add")
        .is_none());
        assert!(apply(
            &tagged,
            &NoteMutation::AddTag {
                tag: "#Medicina".into()
            }
        )
        .expect("add")
        .is_none());
    }

    #[test]
    fn removing_a_tag_that_is_not_there_changes_nothing() {
        let document = note("corpo");
        assert!(apply(
            &document,
            &NoteMutation::RemoveTag {
                tag: "Ausente".into()
            }
        )
        .expect("remove")
        .is_none());
    }

    #[test]
    fn removing_a_tag_matches_the_semantic_identity() {
        let document = apply(
            &note("corpo"),
            &NoteMutation::AddTag {
                tag: "Urgência".into(),
            },
        )
        .expect("add")
        .expect("changed");
        let stripped = apply(
            &document,
            &NoteMutation::RemoveTag {
                tag: "urgencia".into(),
            },
        )
        .expect("remove")
        .expect("changed");
        assert!(stripped.user_metadata.tags.is_empty());
    }

    #[test]
    fn tags_and_properties_never_move_a_timestamp() {
        let document = note("corpo");
        let created = document.metadata.created_at;
        let updated = document.metadata.updated_at;

        let tagged = apply(&document, &NoteMutation::AddTag { tag: "PBL".into() })
            .expect("add")
            .expect("changed");
        assert_eq!(tagged.metadata.created_at, created);
        assert_eq!(tagged.metadata.updated_at, updated);

        let propertied = apply(
            &tagged,
            &NoteMutation::SetProperty {
                key: "tipo".into(),
                value: "estudo".into(),
            },
        )
        .expect("set")
        .expect("changed");
        assert_eq!(propertied.metadata.created_at, created);
        assert_eq!(propertied.metadata.updated_at, updated);

        let removed = apply(
            &propertied,
            &NoteMutation::RemoveProperty { key: "TIPO".into() },
        )
        .expect("remove")
        .expect("changed");
        assert_eq!(removed.metadata.updated_at, updated);
    }

    #[test]
    fn setting_a_property_to_the_value_it_already_has_changes_nothing() {
        let document = apply(
            &note("corpo"),
            &NoteMutation::SetProperty {
                key: "fonte".into(),
                value: "Harrison".into(),
            },
        )
        .expect("set")
        .expect("changed");

        assert!(apply(
            &document,
            &NoteMutation::SetProperty {
                key: "Fonte".into(),
                value: "Harrison".into()
            }
        )
        .expect("set")
        .is_none());
    }

    #[test]
    fn setting_a_property_keeps_the_stored_spelling_of_the_key() {
        let document = apply(
            &note("corpo"),
            &NoteMutation::SetProperty {
                key: "Fonte".into(),
                value: "Harrison".into(),
            },
        )
        .expect("set")
        .expect("changed");
        let updated = apply(
            &document,
            &NoteMutation::SetProperty {
                key: "fonte".into(),
                value: "Cecil".into(),
            },
        )
        .expect("set")
        .expect("changed");
        assert_eq!(updated.user_metadata.properties.as_slice()[0].key, "Fonte");
        assert_eq!(
            updated.user_metadata.properties.as_slice()[0].value,
            "Cecil"
        );
    }

    #[test]
    fn removing_an_absent_property_changes_nothing() {
        let document = note("corpo");
        assert!(apply(
            &document,
            &NoteMutation::RemoveProperty {
                key: "ausente".into()
            }
        )
        .expect("remove")
        .is_none());
    }

    #[test]
    fn a_tag_that_breaks_a_domain_rule_is_refused_without_touching_the_note() {
        let document = note("corpo");
        let error = apply(
            &document,
            &NoteMutation::AddTag {
                tag: "com\nquebra".into(),
            },
        )
        .expect_err("a control character must be refused");
        assert!(matches!(error, WriteError::Validation { .. }), "{error:?}");
    }

    #[test]
    fn unknown_front_matter_survives_a_metadata_mutation() {
        let raw = concat!(
            "---\n",
            "note_it:\n",
            "  id: 00000000-0000-4000-8000-000000000055\n",
            "future_tool:\n",
            "  enabled: true\n",
            "---\n\n",
            "texto\n",
        );
        let document = NoteDocument::parse(raw).expect("parse");
        let tagged = apply(
            &document,
            &NoteMutation::AddTag {
                tag: "Projeto".into(),
            },
        )
        .expect("add")
        .expect("changed");
        let serialized = tagged.serialize().expect("serialize");
        assert!(serialized.contains("future_tool"));
        assert!(serialized.contains("enabled: true"));
    }
    // The race the whole phase exists for ------------------------------------

    #[test]
    fn an_append_keeps_the_paragraph_the_editor_had_not_saved_yet() {
        // disk and host hold "ABC"; the editor holds "ABCD"; a command appends
        // "XYZ". The one unacceptable answer is "ABC\nXYZ", which is exactly
        // what loading the file and appending to it would produce.
        let result = apply_over_live_body(
            &note("ABC"),
            "ABCD",
            &NoteMutation::Append {
                payload: "XYZ".into(),
            },
            &None,
        )
        .expect("append over live text");

        let candidate = result.candidate.expect("something must be written");
        assert_eq!(candidate.content, "ABCD\nXYZ");
        assert_ne!(
            candidate.content, "ABC\nXYZ",
            "the unsaved edit was thrown away"
        );
        assert!(result.mutation_changed);
        assert!(result.adopted_unsaved_text);
    }

    #[test]
    fn adding_a_tag_commits_the_unsaved_text_with_it() {
        // disk "A", editor "AB", `tags adicionar Medicina`. The body must end
        // up "AB": the tag must never be the reason the note goes back to "A".
        let result = apply_over_live_body(
            &note("A"),
            "AB",
            &NoteMutation::AddTag {
                tag: "Medicina".into(),
            },
            &None,
        )
        .expect("tag over live text");

        let candidate = result.candidate.expect("something must be written");
        assert_eq!(candidate.content, "AB");
        assert_eq!(candidate.user_metadata.tags.as_slice(), ["Medicina"]);
    }

    #[test]
    fn the_unsaved_text_moves_the_modification_date_and_the_tag_does_not_move_it_again() {
        let base = note("A");
        let before = base.metadata.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));

        let result = apply_over_live_body(
            &base,
            "AB",
            &NoteMutation::AddTag {
                tag: "Medicina".into(),
            },
            &None,
        )
        .expect("tag over live text");

        let candidate = result.candidate.expect("candidate");
        assert_eq!(candidate.metadata.created_at, base.metadata.created_at);
        assert!(
            candidate.metadata.updated_at > before,
            "the text edit did not move the modification date"
        );

        // The same tag again with nothing unsaved: neither date moves and
        // nothing is written.
        let repeat = apply_over_live_body(
            &candidate,
            &candidate.content,
            &NoteMutation::AddTag {
                tag: "medicina".into(),
            },
            &None,
        )
        .expect("repeat");
        assert!(repeat.candidate.is_none());
        assert!(!repeat.mutation_changed);
        assert!(!repeat.adopted_unsaved_text);
    }

    #[test]
    fn a_no_op_mutation_still_commits_text_the_editor_was_holding() {
        let result = apply_over_live_body(
            &note("A"),
            "AB",
            &NoteMutation::RemoveTag {
                tag: "inexistente".into(),
            },
            &None,
        )
        .expect("no-op tag over live text");

        assert!(
            !result.mutation_changed,
            "removing an absent tag changed something"
        );
        let candidate = result
            .candidate
            .expect("the unsaved text must be written even so");
        assert_eq!(candidate.content, "AB");
    }

    #[test]
    fn a_task_reference_is_resolved_against_the_editor_text_and_not_the_file() {
        // The file has one task; the editor has two, the new one first. A
        // reference taken from the editor's own listing must act on the task it
        // named, and a reference the editor's text has made stale must be
        // refused rather than applied to whatever now sits in that position.
        let base = note("- [ ] Antiga");
        let live = "- [ ] Nova\n- [ ] Antiga";

        let live_refs = crate::task::parse_tasks(base.metadata.id, "nota", live);
        assert_eq!(live_refs[0].text, "Nova");
        let nova = live_refs[0].task_ref.as_str().to_string();

        let result = apply_over_live_body(
            &base,
            live,
            &NoteMutation::CompleteTask { task_ref: nova },
            &None,
        )
        .expect("complete over live text");
        let candidate = result.candidate.expect("candidate");
        assert!(
            candidate.content.starts_with("- [x] Nova"),
            "{}",
            candidate.content
        );
        assert!(candidate.content.contains("- [ ] Antiga"));

        // A reference is content, not a position, so the one taken from the
        // file still names "Antiga" even though a task was inserted above it.
        // That is deliberate: an unrelated insertion must not invalidate every
        // reference below it.
        let file_refs = crate::task::parse_tasks(base.metadata.id, "nota", &base.content);
        let unchanged = file_refs[0].task_ref.as_str().to_string();
        let still_valid = apply_over_live_body(
            &base,
            live,
            &NoteMutation::CompleteTask {
                task_ref: unchanged.clone(),
            },
            &None,
        )
        .expect("an unrelated insertion does not invalidate a reference");
        assert!(still_valid
            .candidate
            .expect("candidate")
            .content
            .contains("- [x] Antiga"));

        // What *does* make it stale is the task itself changing. Here the
        // editor reworded it, so the file's reference names nothing at all and
        // is refused rather than applied to the other task.
        let reworded = "- [ ] Antiga, revisada\n- [ ] Nova";
        let error = apply_over_live_body(
            &base,
            reworded,
            &NoteMutation::CompleteTask {
                task_ref: unchanged,
            },
            &None,
        )
        .expect_err("a reference the live text made stale");
        assert!(
            matches!(error, WriteError::StaleTaskRef { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn nothing_at_all_is_written_when_neither_the_text_nor_the_mutation_changed() {
        let result = apply_over_live_body(
            &note("igual"),
            "igual\n",
            &NoteMutation::ReplaceBody {
                body: "igual".into(),
            },
            &None,
        )
        .expect("no-op");
        assert!(result.candidate.is_none());
        assert!(!result.adopted_unsaved_text);
    }
}
