//! The machine interface: one JSON document per execution.
//!
//! This is the second renderer over [`crate::outcome`], and it is a *contract*
//! rather than a convenience. A script or an agent calling `noteit --json`
//! must be able to answer, from typed fields alone:
//!
//! ```text
//! did the command work?          status
//! was anything changed?          data.write.changed
//! did the commit happen?         data.write.commit_state
//! is the open window in step?    data.write.ui_sync.status
//! which note?                    data.write.note_id  (a full UUID)
//! what went wrong?               error.code
//! ```
//!
//! No sentence in any of these documents is part of the contract. `message`
//! exists so a person reading a log knows what happened; a consumer that
//! branches on it has misread the interface.
//!
//! ## The two answers that must never be confused
//!
//! `ui_sync.status = "warning"` means the change **is committed** and only the
//! window is behind. `commit_state = "unknown"` means the request went out and
//! the answer never came, so the commit may or may not have happened. Neither
//! may be retried automatically, and neither is a failed commit. Everything in
//! this module exists so those two cases are structurally distinct from
//! `commit_state = "not_committed"`, which is the only one where nothing was
//! written at all.
//!
//! ## What is deliberately not here
//!
//! The private control protocol — request identifiers, protocol version, the
//! socket, the writer lease, the window generation, and which of the two write
//! paths a command took. That conversation is between two Note-it processes;
//! this one is between Note-it and its caller, and they are not the same
//! boundary even though both happen to be JSON.

use crate::outcome::{CliResponse, Command, CommandError, Executed, HelpText, Outcome, ReadError};
use crate::output::OutputContext;
use noteit_core::chrono::{DateTime, SecondsFormat, Utc};
use noteit_core::write::{WriteError, WriteOutcome, WriteOutcomeKind};
use noteit_core::{
    MetadataCatalog, NoteDocument, NoteSelectorError, NoteSummary, ReadWarning, ReadWarningKind,
    SearchResult, StorePaths, TaskEntry, TaskStateFilter, TrashEntry, Uuid,
};
use serde::Serialize;

/// The version of this document format.
///
/// New optional fields may appear without a change here. Renaming a field,
/// removing one, or changing what one means is a new version and an explicit
/// decision. Consumers must not depend on the order of keys.
pub const SCHEMA_VERSION: u32 = 1;

/// The last resort if serialisation itself ever failed.
///
/// Unreachable for the types below — they are strings, booleans, integers,
/// vectors and options, none of which can fail to serialise — but a half
/// written document is the one thing this interface may never emit, so the
/// impossible branch answers with a complete, valid, constant one instead of
/// panicking or printing a fragment.
const SERIALIZATION_FAILURE_DOCUMENT: &str = concat!(
    r#"{"schema_version":1,"status":"error","command":null,"data":null,"#,
    r#""error":{"code":"internal_error","#,
    r#""message":"a resposta não pôde ser serializada","commit_state":null},"#,
    r#""warnings":[]}"#,
    "\n"
);

// ---------------------------------------------------------------- the envelope

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MachineStatus {
    Ok,
    Warning,
    Error,
    Indeterminate,
}

/// What a machine consumer needs in order to know whether repeating a write
/// could duplicate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitState {
    /// The change is on disk.
    Committed,
    /// The store already said exactly that; nothing was written.
    NotNeeded,
    /// Nothing was written.
    NotCommitted,
    /// The request went out and the result never came back. It may or may not
    /// have committed. Never repeat this automatically.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MachineWarning {
    code: &'static str,
    message: String,
    note_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MachineError {
    code: &'static str,
    /// Diagnostic only. Never branch on it.
    message: String,
    /// `null` for a command that could not have committed anything.
    commit_state: Option<CommitState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MachineEnvelope {
    schema_version: u32,
    status: MachineStatus,
    command: Option<&'static str>,
    data: Option<MachineData>,
    error: Option<MachineError>,
    warnings: Vec<MachineWarning>,
}

// -------------------------------------------------------------------- the data

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
enum MachineData {
    Welcome(WelcomeData),
    Help(HelpData),
    Version(VersionData),
    Status(StatusData),
    Notes(NotesData),
    Note(NoteEnvelopeData),
    Search(SearchData),
    Tags(TagsData),
    Properties(PropertiesData),
    Tasks(TasksData),
    Trash(TrashData),
    Write(WriteEnvelopeData),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WelcomeData {
    version: &'static str,
    machine_interface: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HelpData {
    usage: &'static str,
    help: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct VersionData {
    version: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct StatusData {
    version: &'static str,
    cli_ready: bool,
    core_available: bool,
    store_exists: bool,
    data_path: String,
    config_path: String,
    state_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PropertyData {
    key: String,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct NoteSummaryData {
    note_id: String,
    label: String,
    snippet: String,
    tags: Vec<String>,
    properties: Vec<PropertyData>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct NotesData {
    notes: Vec<NoteSummaryData>,
    count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct NoteData {
    note_id: String,
    label: String,
    /// The note's Markdown exactly as the Core holds it. Not sanitized: JSON
    /// escaping is what makes a control character safe here, and mangling the
    /// body to protect a terminal that is not rendering it would hand a script
    /// text the note does not contain.
    content: String,
    tags: Vec<String>,
    properties: Vec<PropertyData>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct NoteEnvelopeData {
    note: NoteData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SearchResultData {
    note_id: String,
    label: String,
    snippet: String,
    match_count: usize,
    matched_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SearchData {
    query: String,
    results: Vec<SearchResultData>,
    count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TagData {
    name: String,
    note_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TagsData {
    tags: Vec<TagData>,
    count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PropertyKeyData {
    key: String,
    note_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PropertiesData {
    properties: Vec<PropertyKeyData>,
    count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TaskData {
    /// The reference `tasks complete` and `tasks reopen` name this task by.
    /// Produced by the Core, never derived here.
    task_ref: String,
    note_id: String,
    note_label: String,
    text: String,
    checked: bool,
    completed_at: Option<String>,
    depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TasksData {
    state: &'static str,
    tasks: Vec<TaskData>,
    count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TrashEntryData {
    note_id: String,
    label: String,
    snippet: String,
    deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TrashData {
    entries: Vec<TrashEntryData>,
    count: usize,
}

/// Whether the window showing this note is known to be in step with it.
///
/// `ok` covers both "no window was involved" and "the window confirmed it
/// took the change". `warning` means the change is committed and the window
/// could not be brought into step — the file is right and only the screen is
/// behind. It is never a failure and never a reason to repeat the command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct UiSyncData {
    status: &'static str,
    code: Option<&'static str>,
    message: Option<String>,
}

impl UiSyncData {
    fn in_step() -> Self {
        Self {
            status: "ok",
            code: None,
            message: None,
        }
    }

    fn not_confirmed(detail: &str) -> Self {
        Self {
            status: "warning",
            code: Some(UI_SYNC_WARNING_CODE),
            message: Some(detail.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WriteData {
    note_id: String,
    kind: &'static str,
    changed: bool,
    commit_state: CommitState,
    ui_sync: UiSyncData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WriteEnvelopeData {
    write: WriteData,
}

/// The one code a UI synchronisation warning carries.
pub const UI_SYNC_WARNING_CODE: &str = "window_not_confirmed";

/// The code that warning takes in the envelope's `warnings` list.
pub const UI_SYNC_WARNING_LIST_CODE: &str = "ui_sync_window_not_confirmed";

// --------------------------------------------------------------- the vocabulary

/// The public name of a write outcome.
///
/// Written out rather than derived from the Core's own `Serialize`, because
/// this is a published contract and the Core's spelling is not: a rename there
/// must not silently become a schema change here.
fn outcome_kind_token(kind: WriteOutcomeKind) -> &'static str {
    match kind {
        WriteOutcomeKind::NoteCreated => "note_created",
        WriteOutcomeKind::ContentAppended => "content_appended",
        WriteOutcomeKind::ContentReplaced => "content_replaced",
        WriteOutcomeKind::ContentCleared => "content_cleared",
        WriteOutcomeKind::TagAdded => "tag_added",
        WriteOutcomeKind::TagRemoved => "tag_removed",
        WriteOutcomeKind::PropertySet => "property_set",
        WriteOutcomeKind::PropertyRemoved => "property_removed",
        WriteOutcomeKind::TaskCompleted => "task_completed",
        WriteOutcomeKind::TaskReopened => "task_reopened",
        WriteOutcomeKind::NoteRestored => "note_restored",
    }
}

fn task_state_token(state: TaskStateFilter) -> &'static str {
    match state {
        TaskStateFilter::Pending => "pending",
        TaskStateFilter::Completed => "completed",
        TaskStateFilter::All => "all",
    }
}

fn read_warning_code(kind: ReadWarningKind) -> &'static str {
    match kind {
        ReadWarningKind::UnreadableNote => "unreadable_note",
        ReadWarningKind::CorruptedFrontMatter => "corrupted_front_matter",
        ReadWarningKind::SymlinkRefused => "symlink_refused",
        ReadWarningKind::IoError => "io_error",
    }
}

/// The public code of a write refusal.
///
/// Every variant is listed on purpose: adding one to the Core without deciding
/// what it is called here is a compile error rather than a document with a
/// code nobody documented.
pub fn write_error_code(error: &WriteError) -> &'static str {
    match error {
        WriteError::InvalidInput { .. } => "invalid_input",
        WriteError::NotFound { .. } => "not_found",
        WriteError::AmbiguousSelector { .. } => "ambiguous_selector",
        WriteError::Validation { .. } => "validation",
        WriteError::StaleTaskRef { .. } => "stale_task_ref",
        WriteError::AmbiguousTaskRef { .. } => "ambiguous_task_ref",
        WriteError::WriterBusy { .. } => "writer_busy",
        WriteError::AuthorityUnavailable { .. } => "authority_unavailable",
        WriteError::Indeterminate { .. } => "indeterminate",
        WriteError::TrashTargetOccupied { .. } => "trash_target_occupied",
        WriteError::Persistence { .. } => "persistence",
        WriteError::StoreUnavailable { .. } => "store_unavailable",
    }
}

/// What a write refusal says about the store.
///
/// Every refusal except one means nothing was written. The exception is
/// [`WriteError::Indeterminate`], which is not a refusal at all: it is the
/// absence of an answer, and calling it `not_committed` is exactly the mistake
/// that duplicates an append.
pub fn commit_state_for(error: &WriteError) -> CommitState {
    match error {
        WriteError::Indeterminate { .. } => CommitState::Unknown,
        _ => CommitState::NotCommitted,
    }
}

/// The public code of a read refusal.
fn read_error_code(error: &ReadError) -> &'static str {
    match error {
        ReadError::Selector(NoteSelectorError::InvalidFormat(_))
        | ReadError::Selector(NoteSelectorError::SymlinkRefused(_)) => "invalid_input",
        ReadError::Selector(NoteSelectorError::NotFound(_)) => "not_found",
        ReadError::Selector(NoteSelectorError::Ambiguous(_, _)) => "ambiguous_selector",
        ReadError::Selector(NoteSelectorError::StoreUnavailable(_)) => "store_unavailable",
        ReadError::NoteRead { .. } | ReadError::Listing { .. } => "read_failed",
    }
}

/// The diagnostic sentence a read refusal carries.
///
/// The Core's own `Display`, not the CLI's Portuguese rendering: the machine
/// document is not built out of what a person would have been shown.
fn read_error_message(error: &ReadError) -> String {
    match error {
        ReadError::Selector(inner) => inner.to_string(),
        ReadError::NoteRead { detail } | ReadError::Listing { detail } => detail.clone(),
    }
}

// ------------------------------------------------------------------ timestamps

/// RFC 3339 in UTC, or `null`.
///
/// Never a localised date and never the word "desconhecida": a machine reading
/// a date has to be told the instant or told there is none.
fn timestamp(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|value| value.to_rfc3339_opts(SecondsFormat::AutoSi, true))
}

fn uuid(value: &Uuid) -> String {
    value.to_string()
}

// ------------------------------------------------------------------- rendering

/// Turns one finished execution into its JSON document and the channel it
/// belongs on.
pub fn render(executed: &Executed) -> CliResponse {
    let command = executed.command.map(Command::canonical);

    match &executed.result {
        Ok(outcome) => {
            let warnings = warnings_of(outcome);
            let status = if warnings.is_empty() {
                MachineStatus::Ok
            } else {
                MachineStatus::Warning
            };
            let envelope = MachineEnvelope {
                schema_version: SCHEMA_VERSION,
                status,
                command,
                data: Some(data_of(outcome)),
                error: None,
                warnings,
            };
            match document(&envelope) {
                Ok(text) => CliResponse::success(text),
                Err(fallback) => fallback,
            }
        }
        Err(error) => {
            let status = match error {
                CommandError::Write(WriteError::Indeterminate { .. }) => {
                    MachineStatus::Indeterminate
                }
                _ => MachineStatus::Error,
            };
            let envelope = MachineEnvelope {
                schema_version: SCHEMA_VERSION,
                status,
                command,
                data: None,
                error: Some(machine_error(executed.command, error)),
                warnings: Vec::new(),
            };
            match document(&envelope) {
                Ok(text) => CliResponse::failure(error.exit_code(), text),
                Err(fallback) => fallback,
            }
        }
    }
}

/// Serialises an envelope into exactly one document with a closing newline.
///
/// Built completely before anything is written, so there is no path on which
/// half a document reaches a channel. If it could not be built at all — which
/// these types cannot do — the answer is a complete refusal on the error
/// channel rather than a fragment, and never a success carrying an error
/// document.
fn document(envelope: &MachineEnvelope) -> Result<String, CliResponse> {
    match serde_json::to_string(envelope) {
        Ok(text) => Ok(format!("{text}\n")),
        Err(_) => Err(CliResponse::failure(
            crate::EXIT_EXECUTION_ERROR,
            SERIALIZATION_FAILURE_DOCUMENT.to_string(),
        )),
    }
}

fn machine_error(command: Option<Command>, error: &CommandError) -> MachineError {
    // A failure has a commit state only when the command could have committed
    // something. A listing that failed did not fail to commit.
    let could_write = command.map(Command::writes).unwrap_or(false);

    match error {
        CommandError::Usage(usage) => MachineError {
            code: "usage_error",
            message: usage.sentence(),
            commit_state: could_write.then_some(CommitState::NotCommitted),
        },
        CommandError::Read(read) => MachineError {
            code: read_error_code(read),
            message: read_error_message(read),
            commit_state: could_write.then_some(CommitState::NotCommitted),
        },
        CommandError::Write(write) => MachineError {
            code: write_error_code(write),
            message: write.to_string(),
            commit_state: Some(commit_state_for(write)),
        },
    }
}

fn warnings_of(outcome: &Outcome) -> Vec<MachineWarning> {
    match outcome {
        Outcome::Notes(batch) => batch.warnings.iter().map(read_warning).collect(),
        Outcome::Search { batch, .. } => batch.warnings.iter().map(read_warning).collect(),
        Outcome::Tasks { batch, .. } => batch.warnings.iter().map(read_warning).collect(),
        Outcome::Tags { warnings, .. } => warnings.iter().map(read_warning).collect(),
        Outcome::Properties { warnings, .. } => warnings.iter().map(read_warning).collect(),
        Outcome::Write(outcome) => match &outcome.ui_sync_warning {
            Some(detail) => vec![MachineWarning {
                code: UI_SYNC_WARNING_LIST_CODE,
                message: detail.clone(),
                note_id: Some(uuid(&outcome.note_id)),
            }],
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn read_warning(warning: &ReadWarning) -> MachineWarning {
    MachineWarning {
        code: read_warning_code(warning.kind),
        message: warning.message.clone(),
        note_id: warning.note_id.as_ref().map(uuid),
    }
}

fn data_of(outcome: &Outcome) -> MachineData {
    match outcome {
        Outcome::Welcome => MachineData::Welcome(WelcomeData {
            version: env!("CARGO_PKG_VERSION"),
            machine_interface: true,
        }),
        Outcome::Help(help) => MachineData::Help(HelpData {
            usage: "noteit [--json] <comando> [opções]",
            help: match help {
                // Rendered for this adapter, never handed over from the other
                // one: the machine document must not contain styling even when
                // the process is attached to a terminal.
                HelpText::Own => crate::output::render_help(&OutputContext::plain()),
                HelpText::Sub(text) => text.clone(),
            },
        }),
        Outcome::Version => MachineData::Version(VersionData {
            version: env!("CARGO_PKG_VERSION"),
        }),
        Outcome::Status(paths) => MachineData::Status(status_data(paths)),
        Outcome::Notes(batch) => {
            let notes: Vec<NoteSummaryData> = batch.items.iter().map(note_summary).collect();
            MachineData::Notes(NotesData {
                count: notes.len(),
                notes,
            })
        }
        Outcome::Note(document) => MachineData::Note(NoteEnvelopeData {
            note: note_data(document),
        }),
        Outcome::Search { query, batch } => {
            let results: Vec<SearchResultData> = batch.items.iter().map(search_result).collect();
            MachineData::Search(SearchData {
                query: query.clone(),
                count: results.len(),
                results,
            })
        }
        Outcome::Tags { catalog, .. } => MachineData::Tags(tags_data(catalog)),
        Outcome::Properties { catalog, .. } => MachineData::Properties(properties_data(catalog)),
        Outcome::Tasks { state, batch } => {
            let tasks: Vec<TaskData> = batch.items.iter().map(task_data).collect();
            MachineData::Tasks(TasksData {
                state: task_state_token(*state),
                count: tasks.len(),
                tasks,
            })
        }
        Outcome::Trash(entries) => {
            let entries: Vec<TrashEntryData> = entries.iter().map(trash_entry).collect();
            MachineData::Trash(TrashData {
                count: entries.len(),
                entries,
            })
        }
        Outcome::Write(outcome) => MachineData::Write(WriteEnvelopeData {
            write: write_data(outcome),
        }),
    }
}

fn status_data(paths: &StorePaths) -> StatusData {
    StatusData {
        version: env!("CARGO_PKG_VERSION"),
        cli_ready: true,
        core_available: true,
        store_exists: paths.store_exists(),
        data_path: paths.data_dir.display().to_string(),
        config_path: paths.config_dir.display().to_string(),
        state_path: paths.state_dir.display().to_string(),
    }
}

fn note_summary(summary: &NoteSummary) -> NoteSummaryData {
    NoteSummaryData {
        note_id: uuid(&summary.id),
        label: summary.label.clone(),
        snippet: summary.snippet.clone(),
        tags: summary.tags.clone(),
        properties: summary
            .properties
            .iter()
            .map(|(key, value)| PropertyData {
                key: key.clone(),
                value: value.clone(),
            })
            .collect(),
        created_at: timestamp(summary.created_at),
        updated_at: timestamp(summary.updated_at),
    }
}

fn note_data(document: &NoteDocument) -> NoteData {
    NoteData {
        note_id: uuid(&document.metadata.id),
        label: noteit_core::search::label_for(&document.content),
        content: document.content.clone(),
        tags: document.user_metadata.tags.as_slice().to_vec(),
        properties: document
            .user_metadata
            .properties
            .as_slice()
            .iter()
            .map(|property| PropertyData {
                key: property.key.clone(),
                value: property.value.clone(),
            })
            .collect(),
        created_at: timestamp(document.metadata.created_at),
        updated_at: timestamp(document.metadata.updated_at),
    }
}

fn search_result(result: &SearchResult) -> SearchResultData {
    SearchResultData {
        note_id: uuid(&result.note_id),
        label: result.label.clone(),
        snippet: result.snippet.clone(),
        match_count: result.match_count,
        matched_text: result.matched_text.clone(),
    }
}

fn tags_data(catalog: &MetadataCatalog) -> TagsData {
    let tags: Vec<TagData> = catalog
        .tags
        .iter()
        .map(|entry| TagData {
            name: entry.tag.clone(),
            note_count: entry.note_count,
        })
        .collect();
    TagsData {
        count: tags.len(),
        tags,
    }
}

fn properties_data(catalog: &MetadataCatalog) -> PropertiesData {
    let properties: Vec<PropertyKeyData> = catalog
        .property_keys
        .iter()
        .map(|entry| PropertyKeyData {
            key: entry.key.clone(),
            note_count: entry.note_count,
        })
        .collect();
    PropertiesData {
        count: properties.len(),
        properties,
    }
}

fn task_data(task: &TaskEntry) -> TaskData {
    TaskData {
        task_ref: task.task_ref.as_str().to_string(),
        note_id: uuid(&task.note_id),
        note_label: task.note_label.clone(),
        text: task.text.clone(),
        checked: task.checked,
        completed_at: timestamp(task.completed_at),
        depth: task.depth,
    }
}

fn trash_entry(entry: &TrashEntry) -> TrashEntryData {
    TrashEntryData {
        note_id: uuid(&entry.note_id),
        label: entry.label.clone(),
        snippet: entry.snippet.clone(),
        deleted_at: timestamp(entry.deleted_at),
    }
}

fn write_data(outcome: &WriteOutcome) -> WriteData {
    WriteData {
        note_id: uuid(&outcome.note_id),
        kind: outcome_kind_token(outcome.kind),
        changed: outcome.changed,
        // A write that reached this point committed. `changed == false` is the
        // other successful answer: the store already said exactly that, so
        // nothing needed writing. Neither is a failure.
        commit_state: if outcome.changed {
            CommitState::Committed
        } else {
            CommitState::NotNeeded
        },
        ui_sync: match &outcome.ui_sync_warning {
            Some(detail) => UiSyncData::not_confirmed(detail),
            None => UiSyncData::in_step(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::UsageError;
    use serde_json::Value;

    fn parse(text: &str) -> Value {
        assert!(text.ends_with('\n'), "a document must end in a newline");
        serde_json::from_str(text).expect("the channel must hold one JSON document")
    }

    #[test]
    fn the_serialization_failure_document_is_itself_a_valid_document() {
        let value = parse(SERIALIZATION_FAILURE_DOCUMENT);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["status"], "error");
        assert_eq!(value["error"]["code"], "internal_error");
    }

    #[test]
    fn a_committed_write_says_so_in_typed_fields_only() {
        let note = Uuid::new_v4();
        let executed = Executed::ok(
            Command::Append,
            Outcome::Write(Box::new(WriteOutcome::new(
                note,
                WriteOutcomeKind::ContentAppended,
                true,
            ))),
        );
        let response = render(&executed);
        assert_eq!(response.exit_code, 0);
        assert!(response.stderr.is_empty());

        let value = parse(&response.stdout);
        assert_eq!(value["status"], "ok");
        assert_eq!(value["command"], "append");
        assert_eq!(value["data"]["write"]["note_id"], note.to_string());
        assert_eq!(value["data"]["write"]["kind"], "content_appended");
        assert_eq!(value["data"]["write"]["changed"], Value::Bool(true));
        assert_eq!(value["data"]["write"]["commit_state"], "committed");
        assert_eq!(value["data"]["write"]["ui_sync"]["status"], "ok");
        assert_eq!(value["warnings"], Value::Array(vec![]));
        assert_eq!(value["error"], Value::Null);
    }

    #[test]
    fn a_no_op_write_is_a_success_that_needed_no_commit() {
        let executed = Executed::ok(
            Command::TagAdd,
            Outcome::Write(Box::new(WriteOutcome::new(
                Uuid::new_v4(),
                WriteOutcomeKind::TagAdded,
                false,
            ))),
        );
        let value = parse(&render(&executed).stdout);
        assert_eq!(value["status"], "ok");
        assert_eq!(value["data"]["write"]["changed"], Value::Bool(false));
        assert_eq!(value["data"]["write"]["commit_state"], "not_needed");
    }

    #[test]
    fn a_committed_write_whose_window_lagged_stays_committed_and_succeeds() {
        let executed = Executed::ok(
            Command::Append,
            Outcome::Write(Box::new(
                WriteOutcome::new(Uuid::new_v4(), WriteOutcomeKind::ContentAppended, true)
                    .with_ui_sync_warning("a janela não confirmou"),
            )),
        );
        let response = render(&executed);
        assert_eq!(response.exit_code, 0, "a warning is not a failure");
        assert!(response.stderr.is_empty(), "a warning is data, not stderr");

        let value = parse(&response.stdout);
        assert_eq!(value["status"], "warning");
        assert_eq!(value["data"]["write"]["commit_state"], "committed");
        assert_eq!(value["data"]["write"]["ui_sync"]["status"], "warning");
        assert_eq!(
            value["data"]["write"]["ui_sync"]["code"],
            UI_SYNC_WARNING_CODE
        );
        assert_eq!(value["warnings"][0]["code"], UI_SYNC_WARNING_LIST_CODE);
    }

    #[test]
    fn an_indeterminate_write_is_neither_success_nor_a_failed_commit() {
        let executed = Executed::failed(
            Some(Command::Append),
            CommandError::Write(WriteError::Indeterminate {
                detail: "a resposta não chegou".to_string(),
            }),
        );
        let response = render(&executed);
        assert_ne!(response.exit_code, 0);
        assert!(response.stdout.is_empty());

        let value = parse(&response.stderr);
        assert_eq!(value["status"], "indeterminate");
        assert_eq!(value["error"]["code"], "indeterminate");
        assert_eq!(value["error"]["commit_state"], "unknown");
        assert_eq!(value["data"], Value::Null);
    }

    #[test]
    fn every_other_write_refusal_committed_nothing() {
        for error in [
            WriteError::InvalidInput { detail: "x".into() },
            WriteError::NotFound {
                selector: "x".into(),
            },
            WriteError::AmbiguousSelector {
                selector: "x".into(),
                matches: 2,
            },
            WriteError::Validation { detail: "x".into() },
            WriteError::StaleTaskRef {
                task_ref: "a1b2c3d4".into(),
            },
            WriteError::AmbiguousTaskRef {
                task_ref: "a1b2c3d4".into(),
            },
            WriteError::WriterBusy { detail: "x".into() },
            WriteError::AuthorityUnavailable { detail: "x".into() },
            WriteError::TrashTargetOccupied {
                note_id: Uuid::new_v4(),
            },
            WriteError::Persistence { detail: "x".into() },
            WriteError::StoreUnavailable { detail: "x".into() },
        ] {
            let code = write_error_code(&error);
            let executed =
                Executed::failed(Some(Command::Append), CommandError::Write(error.clone()));
            let value = parse(&render(&executed).stderr);
            assert_eq!(value["status"], "error", "{error:?}");
            assert_eq!(value["error"]["code"], code, "{error:?}");
            assert_eq!(value["error"]["commit_state"], "not_committed", "{error:?}");
        }
    }

    #[test]
    fn a_usage_error_on_a_read_command_has_no_commit_state() {
        let executed = Executed::failed(
            None,
            CommandError::Usage(UsageError::UnknownCommand {
                name: Some("batata".into()),
            }),
        );
        let response = render(&executed);
        assert_eq!(response.exit_code, crate::EXIT_USAGE_ERROR);
        let value = parse(&response.stderr);
        assert_eq!(value["status"], "error");
        assert_eq!(value["command"], Value::Null);
        assert_eq!(value["error"]["code"], "usage_error");
        assert_eq!(value["error"]["commit_state"], Value::Null);
    }

    #[test]
    fn timestamps_are_rfc3339_in_utc_or_null() {
        use noteit_core::chrono::TimeZone;
        let instant = Utc
            .with_ymd_and_hms(2026, 9, 1, 22, 30, 0)
            .single()
            .expect("instant");
        assert_eq!(
            timestamp(Some(instant)).as_deref(),
            Some("2026-09-01T22:30:00Z")
        );
        assert_eq!(timestamp(None), None);
    }

    #[test]
    fn read_warnings_are_data_rather_than_a_sentence_on_stderr() {
        let batch = noteit_core::ReadBatch::new(
            Vec::<NoteSummary>::new(),
            vec![ReadWarning {
                note_id: Some(Uuid::new_v4()),
                kind: ReadWarningKind::UnreadableNote,
                message: "front matter inválido".to_string(),
            }],
        );
        let response = render(&Executed::ok(Command::List, Outcome::Notes(batch)));
        assert_eq!(response.exit_code, 0);
        assert!(response.stderr.is_empty());

        let value = parse(&response.stdout);
        assert_eq!(value["status"], "warning");
        assert_eq!(value["warnings"][0]["code"], "unreadable_note");
        assert_eq!(value["data"]["count"], 0);
        assert_eq!(value["data"]["notes"], Value::Array(vec![]));
    }
}
