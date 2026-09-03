//! What a command produced, before anyone decides how to say it.
//!
//! Phase 4.0F gave this CLI a second audience. The sentences a person reads
//! and the document a script parses have to agree about what happened, and the
//! only way to guarantee that is for both to be rendered from the same typed
//! value — never one from the other. So the dispatcher stops here: it runs the
//! operation, puts the result in [`Outcome`] or [`CommandError`], and hands
//! that to whichever renderer was asked for.
//!
//! ```text
//! domain  ──►  Outcome / CommandError  ──►  output::  (human sentences)
//!                                      └─►  machine:: (JSON document)
//! ```
//!
//! Nothing here formats anything. In particular nothing here sanitizes
//! anything: untrusted text is carried exactly as the store or the argument
//! list holds it, and it is the *human* renderer that neutralises terminal
//! escapes on its way to a terminal. JSON is data and gets the real value.

use noteit_core::revision::NoteRevision;
use noteit_core::write::{WriteError, WriteOutcome};
use noteit_core::{
    MetadataCatalog, NoteDocument, NoteSelectorError, NoteSummary, ReadBatch, ReadWarning,
    SearchResult, StorePaths, TaskEntry, TaskStateFilter, TrashEntry,
};

/// Everything one execution of the CLI has to say, as data.
///
/// The two channels are values rather than side effects on purpose: a warning
/// that escaped to stderr through an `eprint!` somewhere in the middle of a
/// command is invisible to a function-level test and fatal to a machine
/// interface, and this is what makes it impossible to write one by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliResponse {
    pub exit_code: u8,
    pub stdout: String,
    pub stderr: String,
}

impl CliResponse {
    pub fn success(stdout: String) -> Self {
        Self {
            exit_code: crate::EXIT_SUCCESS,
            stdout,
            stderr: String::new(),
        }
    }

    pub fn failure(exit_code: u8, stderr: String) -> Self {
        Self {
            exit_code,
            stdout: String::new(),
            stderr,
        }
    }
}

/// One logical command, named once.
///
/// The identifier a machine consumer sees is decided here and nowhere else, so
/// `listar` and `list` cannot drift into two different contracts: they are the
/// same variant before either renderer runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Welcome,
    Help,
    Version,
    Status,
    List,
    Read,
    Search,
    Tags,
    Properties,
    Tasks,
    Trash,
    Create,
    Append,
    Edit,
    TagAdd,
    TagRemove,
    PropertySet,
    PropertyRemove,
    TaskComplete,
    TaskReopen,
    TrashRestore,
}

impl Command {
    /// The stable public name. Never translated, never derived from what the
    /// user typed.
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::Help => "help",
            Self::Version => "version",
            Self::Status => "status",
            Self::List => "list",
            Self::Read => "read",
            Self::Search => "search",
            Self::Tags => "tags",
            Self::Properties => "properties",
            Self::Tasks => "tasks",
            Self::Trash => "trash",
            Self::Create => "create",
            Self::Append => "append",
            Self::Edit => "edit",
            Self::TagAdd => "tag_add",
            Self::TagRemove => "tag_remove",
            Self::PropertySet => "property_set",
            Self::PropertyRemove => "property_remove",
            Self::TaskComplete => "task_complete",
            Self::TaskReopen => "task_reopen",
            Self::TrashRestore => "trash_restore",
        }
    }

    /// Whether this command can change the store.
    ///
    /// Read by the machine renderer to decide whether a failure has a commit
    /// state at all: a listing that failed did not fail to commit anything.
    pub fn writes(self) -> bool {
        matches!(
            self,
            Self::Create
                | Self::Append
                | Self::Edit
                | Self::TagAdd
                | Self::TagRemove
                | Self::PropertySet
                | Self::PropertyRemove
                | Self::TaskComplete
                | Self::TaskReopen
                | Self::TrashRestore
        )
    }

    /// Every command, for the tests that assert the contract is complete.
    pub const ALL: [Command; 21] = [
        Command::Welcome,
        Command::Help,
        Command::Version,
        Command::Status,
        Command::List,
        Command::Read,
        Command::Search,
        Command::Tags,
        Command::Properties,
        Command::Tasks,
        Command::Trash,
        Command::Create,
        Command::Append,
        Command::Edit,
        Command::TagAdd,
        Command::TagRemove,
        Command::PropertySet,
        Command::PropertyRemove,
        Command::TaskComplete,
        Command::TaskReopen,
        Command::TrashRestore,
    ];
}

/// Which help a `help` outcome carries.
///
/// The CLI's own help is rendered by whichever adapter asks for it, so the
/// human one keeps its styling and the machine one never sees an escape.
/// Clap's help for a specific subcommand is already plain text and travels as
/// it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpText {
    Own,
    Sub(String),
}

/// A command that produced a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Welcome,
    Help(HelpText),
    Version,
    Status(Box<StorePaths>),
    Notes(ReadBatch<NoteSummary>),
    /// One note, and the exact version this answer describes.
    ///
    /// The revision travels with the document rather than being recomputed by
    /// each adapter: a client that builds a conditional write from this reply
    /// must send back the version the reply was made from, and two independent
    /// computations of "which version was that" is exactly how they drift.
    Note {
        document: Box<NoteDocument>,
        revision: NoteRevision,
    },
    Search {
        query: String,
        batch: ReadBatch<SearchResult>,
    },
    Tags {
        catalog: MetadataCatalog,
        warnings: Vec<ReadWarning>,
    },
    Properties {
        catalog: MetadataCatalog,
        warnings: Vec<ReadWarning>,
    },
    Tasks {
        state: TaskStateFilter,
        batch: ReadBatch<TaskEntry>,
    },
    Trash(Vec<TrashEntry>),
    Write(Box<WriteOutcome>),
}

/// Which of the three closing lines a usage error carries.
///
/// Three, because the CLI has always had three, and Phase 4.0F does not get to
/// reword what a person reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageHint {
    /// "para ver os comandos disponíveis"
    Commands,
    /// "para ver os comandos e opções disponíveis"
    CommandsAndOptions,
    /// "para ver o formato correto de uso"
    UsageFormat,
    /// "para ver o formato correto"
    Format,
}

impl UsageHint {
    pub fn phrase(self) -> &'static str {
        match self {
            Self::Commands => "para ver os comandos disponíveis",
            Self::CommandsAndOptions => "para ver os comandos e opções disponíveis",
            Self::UsageFormat => "para ver o formato correto de uso",
            Self::Format => "para ver o formato correto",
        }
    }
}

/// A request that was not well formed.
///
/// Carries the offending text exactly as it arrived. The human renderer
/// neutralises it before it reaches a terminal; the machine renderer does not,
/// because JSON escaping already makes a control character harmless and
/// destroying the value would tell a script something untrue about its own
/// arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageError {
    UnknownCommand {
        name: Option<String>,
    },
    UnknownOption {
        argument: String,
    },
    UnexpectedArgument {
        argument: String,
    },
    MissingArgument {
        argument: Option<String>,
    },
    UnknownArgumentOrOption,
    Invalid,
    /// One of this CLI's own usage sentences.
    Detail {
        message: String,
    },
}

impl UsageError {
    /// A usage error this CLI raised itself, with its own sentence.
    pub fn detail(message: impl Into<String>) -> Self {
        Self::Detail {
            message: message.into(),
        }
    }

    /// The sentence, without the prefix and without the closing hint, with the
    /// untrusted fragment passed through `prepare` first.
    ///
    /// The transform is applied to the fragment rather than to the finished
    /// sentence on purpose. The human renderer neutralises terminal escapes,
    /// and an argument ending in an unterminated escape sequence would
    /// otherwise swallow the punctuation written around it — which is not what
    /// this CLI printed before Phase 4.0F, and a machine interface is no reason
    /// to change what a person reads.
    pub fn sentence_with(&self, prepare: impl Fn(&str) -> String) -> String {
        match self {
            Self::UnknownCommand { name: Some(name) } => {
                format!("comando desconhecido `{}`.", prepare(name))
            }
            Self::UnknownCommand { name: None } => "comando desconhecido.".to_string(),
            Self::UnknownOption { argument } => {
                format!("opção desconhecida `{}`.", prepare(argument))
            }
            Self::UnexpectedArgument { argument } => {
                format!("argumento inesperado `{}`.", prepare(argument))
            }
            Self::MissingArgument {
                argument: Some(argument),
            } => format!(
                "argumento obrigatório `{}` não fornecido.",
                prepare(argument)
            ),
            Self::MissingArgument { argument: None } => {
                "argumento obrigatório não fornecido.".to_string()
            }
            Self::UnknownArgumentOrOption => "argumento ou opção desconhecida.".to_string(),
            Self::Invalid => "uso inválido.".to_string(),
            // This CLI's own sentences carry no untrusted fragment of their
            // own, so the whole message is what gets prepared — exactly as the
            // dispatcher has always done with it.
            Self::Detail { message } => prepare(message),
        }
    }

    /// The sentence exactly as the arguments spelled it.
    pub fn sentence(&self) -> String {
        self.sentence_with(str::to_string)
    }

    pub fn hint(&self) -> UsageHint {
        match self {
            Self::UnknownCommand { .. } => UsageHint::Commands,
            Self::UnknownOption { .. } | Self::UnknownArgumentOrOption | Self::Invalid => {
                UsageHint::CommandsAndOptions
            }
            Self::UnexpectedArgument { .. } | Self::MissingArgument { .. } => {
                UsageHint::UsageFormat
            }
            Self::Detail { .. } => UsageHint::Format,
        }
    }

    /// Reads one of clap's refusals as one of ours.
    ///
    /// The classification of an unknown argument — an option or a stray value —
    /// is made on the *sanitized* spelling, which is the spelling the human
    /// message has always been chosen by. The raw text is what is kept, so the
    /// machine document reports the argument the process actually received.
    pub fn from_clap(error: &clap::Error) -> Self {
        use clap::error::{ContextKind, ContextValue, ErrorKind};

        let context = |kind: ContextKind| -> Option<String> {
            match error.get(kind) {
                Some(ContextValue::String(value)) => Some(value.clone()),
                Some(ContextValue::Strings(values)) => values.first().cloned(),
                Some(ContextValue::StyledStr(value)) => Some(value.to_string()),
                Some(ContextValue::StyledStrs(values)) => {
                    values.first().map(|value| value.to_string())
                }
                _ => None,
            }
        };

        match error.kind() {
            ErrorKind::InvalidSubcommand => {
                let raw = context(ContextKind::InvalidSubcommand).unwrap_or_default();
                if crate::output::sanitize_for_terminal(&raw).is_empty() {
                    Self::UnknownCommand { name: None }
                } else {
                    Self::UnknownCommand { name: Some(raw) }
                }
            }
            ErrorKind::UnknownArgument => {
                let raw = context(ContextKind::InvalidArg).unwrap_or_default();
                let shown = crate::output::sanitize_for_terminal(&raw);
                if shown.starts_with('-') {
                    Self::UnknownOption { argument: raw }
                } else if !shown.is_empty() {
                    Self::UnexpectedArgument { argument: raw }
                } else {
                    Self::UnknownArgumentOrOption
                }
            }
            ErrorKind::MissingRequiredArgument => {
                let raw = context(ContextKind::InvalidArg).unwrap_or_default();
                if crate::output::sanitize_for_terminal(&raw).is_empty() {
                    Self::MissingArgument { argument: None }
                } else {
                    Self::MissingArgument {
                        argument: Some(raw),
                    }
                }
            }
            _ => Self::Invalid,
        }
    }
}

/// A read that was understood and could not be carried out.
///
/// Reads have no `WriteError`, and inventing one would have made the machine
/// contract claim things about commits that were never attempted. This is the
/// smallest set that keeps every sentence the CLI already says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The note selector could not be resolved.
    Selector(NoteSelectorError),
    /// One note could not be read.
    NoteRead { detail: String },
    /// The store could not be listed or searched.
    Listing { detail: String },
}

/// Why a command produced no result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    Usage(UsageError),
    Read(ReadError),
    Write(WriteError),
}

impl CommandError {
    /// Which exit code this refusal carries.
    ///
    /// Unchanged from every phase before it: `2` is "that is not a valid
    /// request", `1` is "the request was understood and could not be done".
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => crate::EXIT_USAGE_ERROR,
            Self::Read(_) => crate::EXIT_EXECUTION_ERROR,
            Self::Write(error) => crate::output::exit_code_for_write_error(error),
        }
    }
}

/// One finished execution: which command it was, and what came of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executed {
    /// `None` only when the argument list never named a command this build
    /// recognises.
    pub command: Option<Command>,
    pub result: Result<Outcome, CommandError>,
}

impl Executed {
    pub fn ok(command: Command, outcome: Outcome) -> Self {
        Self {
            command: Some(command),
            result: Ok(outcome),
        }
    }

    pub fn failed(command: Option<Command>, error: CommandError) -> Self {
        Self {
            command,
            result: Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_command_has_a_distinct_canonical_name() {
        let names: BTreeSet<&str> = Command::ALL.iter().map(|c| c.canonical()).collect();
        assert_eq!(
            names.len(),
            Command::ALL.len(),
            "two commands share a public identifier"
        );
        for name in &names {
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
                "`{name}` is not a stable snake_case token"
            );
        }
    }

    #[test]
    fn the_human_sentence_keeps_its_punctuation_around_a_hostile_argument() {
        // An argument ending in an unterminated CSI: neutralising the finished
        // sentence instead of the fragment would let it eat the closing
        // backtick and the full stop.
        let error = UsageError::UnknownCommand {
            name: Some("abc\u{1b}[".to_string()),
        };
        assert_eq!(
            error.sentence_with(crate::output::sanitize_for_terminal),
            "comando desconhecido `abc`."
        );
        // And the machine document is handed the argument the process really
        // received, which JSON escaping is what makes safe.
        assert_eq!(error.sentence(), "comando desconhecido `abc\u{1b}[`.");
    }

    #[test]
    fn exactly_the_store_changing_commands_are_writes() {
        let writing: BTreeSet<&str> = Command::ALL
            .iter()
            .filter(|c| c.writes())
            .map(|c| c.canonical())
            .collect();
        assert_eq!(
            writing,
            BTreeSet::from([
                "create",
                "append",
                "edit",
                "tag_add",
                "tag_remove",
                "property_set",
                "property_remove",
                "task_complete",
                "task_reopen",
                "trash_restore",
            ])
        );
    }
}
