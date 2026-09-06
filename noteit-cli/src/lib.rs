pub mod cli;
pub mod machine;
pub mod outcome;
pub mod output;
pub mod welcome;

/// The one write authority, re-exported under the name this crate has always
/// used for it.
///
/// The implementation moved into the Core when the MCP server became a second
/// programmatic writer: two copies of "who may write now" would eventually be
/// two answers, and the lease only works because there is one.
pub use noteit_core::authority;

use clap::error::ErrorKind;
use clap::Parser;
use cli::{CliArgs, CliCommand, PropertiesCommand, TagsCommand, TasksCommand, TrashCommand};
use noteit_core::revision::NoteRevision;
use noteit_core::settings::AppConfig;
use noteit_core::write::{NoteDraft, NoteMutation, WriteOperation};
use noteit_core::{NoteFilter, NoteItCore, NoteProperty, StorePaths};
use outcome::{
    CliResponse, Command, CommandError, Executed, HelpText, Outcome, ReadError,
    SemanticStatusReport, StatusReport, UsageError,
};
use output::Channels;
use std::io::Read;

pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_EXECUTION_ERROR: u8 = 1;
pub const EXIT_USAGE_ERROR: u8 = 2;

/// The option that asks for the machine interface.
const MACHINE_FLAG: &str = "--json";

/// The token that ends option parsing, after which everything is a value.
const ARGUMENT_ESCAPE: &str = "--";

fn parse_filter(tags: Vec<String>, properties_raw: &[String]) -> Result<NoteFilter, String> {
    let mut properties = Vec::new();
    for raw in properties_raw {
        let (k, v) = NoteFilter::parse_property_arg(raw)?;
        properties.push((k, v));
    }
    Ok(NoteFilter::new(tags, properties))
}

/// Where `--stdin` gets its text.
///
/// A parameter rather than a direct read of the process's own input, so the
/// tests can hand a command its standard input without a pipe and without a
/// child process. The real one is [`read_process_stdin`].
pub type StdinSource<'a> = &'a dyn Fn() -> Result<String, String>;

/// Executes the CLI with the provided argument list and output context.
///
/// Returns everything the process has to say as data: the exit code and both
/// channels. Nothing in here prints.
pub fn run_with_args<I, T>(args: I, channels: &Channels) -> CliResponse
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    run_with_args_and_stdin(args, channels, &read_process_stdin)
}

/// The whole dispatcher, with standard input supplied explicitly.
pub fn run_with_args_and_stdin<I, T>(
    args: I,
    channels: &Channels,
    stdin: StdinSource<'_>,
) -> CliResponse
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args_vec: Vec<std::ffi::OsString> = args.into_iter().map(|a| a.into()).collect();

    let parsed = CliArgs::try_parse_from(&args_vec);

    // Which adapter answers has to survive an argument list clap could not
    // read at all: a script that asked for JSON and got a paragraph of
    // Portuguese on stderr has no way to find out what went wrong. When the
    // parse succeeded the parsed flag is the authority; when it failed the
    // same decision is made from the raw arguments, under the same rule.
    let machine = match &parsed {
        Ok(parsed) => parsed.json,
        Err(_) => machine_mode_requested(&args_vec),
    };

    let executed = match parsed {
        Ok(parsed) => execute(parsed, stdin),
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp => {
                // Help for a specific subcommand is clap's to write; the bare
                // `--help` is ours. The machine flag is not part of that
                // question, so it does not count towards it.
                let help = if significant_argument_count(&args_vec) > 2 {
                    HelpText::Sub(error.render().to_string())
                } else {
                    HelpText::Own
                };
                Executed::ok(Command::Help, Outcome::Help(help))
            }
            ErrorKind::DisplayVersion => Executed::ok(Command::Version, Outcome::Version),
            _ => Executed::failed(None, CommandError::Usage(UsageError::from_clap(&error))),
        },
    };

    if machine {
        machine::render(&executed)
    } else {
        output::render(&executed, channels)
    }
}

/// Whether the raw argument list asked for the machine interface.
///
/// Used only when clap could not parse. Deliberately exact: the option is
/// recognised as a whole argument and never as a substring, and never after
/// the `--` escape — so `noteit adicionar ID -- --json` appends the literal
/// text `--json` to a note and answers in Portuguese, which is what it asked
/// for. Standard input is never looked at.
fn machine_mode_requested(args: &[std::ffi::OsString]) -> bool {
    let mut requested = false;
    for argument in args.iter().skip(1) {
        let Some(text) = argument.to_str() else {
            continue;
        };
        if text == ARGUMENT_ESCAPE {
            break;
        }
        if text == MACHINE_FLAG {
            requested = true;
        }
    }
    requested
}

/// How many arguments there are, not counting the machine flag.
///
/// `noteit --help` shows this CLI's own help and `noteit listar --help` shows
/// clap's. Asking for the same thing in JSON must not change which of the two
/// it is.
fn significant_argument_count(args: &[std::ffi::OsString]) -> usize {
    let mut count = 0usize;
    let mut escaped = false;
    for (index, argument) in args.iter().enumerate() {
        let text = argument.to_str().unwrap_or_default();
        if index > 0 && !escaped {
            if text == ARGUMENT_ESCAPE {
                escaped = true;
            } else if text == MACHINE_FLAG {
                continue;
            }
        }
        count += 1;
    }
    count
}

/// Runs one parsed command and answers with what it produced.
///
/// Nothing here formats anything: every branch ends in an [`Outcome`] or a
/// [`CommandError`], and the renderers decide what that looks like.
/// What the machine looks like, for `noteit status`.
///
/// Reads the configuration and asks the provider crate whether its two files
/// are there. Loads nothing: `status` is a question about the machine, and one
/// that spent seconds verifying half a gigabyte of weights to answer it would
/// be a different command. Whether those bytes are the *right* bytes is decided
/// where a provider is actually built, from the bytes themselves.
///
/// The question is asked through `artifact_availability` and not through
/// `Path::is_file`, because the two do not agree: `is_file` follows a symlink
/// and the loader refuses one. A status that said "present" about a file the
/// loader will reject is a status that lies.
fn status() -> StatusReport {
    let paths = StorePaths::resolve();
    let semantic = AppConfig::read_only(&paths.config_file_path()).semantic_retrieval;
    let directory = noteit_embedding_local::artifact_directory(
        &noteit_embedding_local::POTION_MULTILINGUAL_128M,
    );
    let artifact_present = directory
        .as_deref()
        .map(noteit_embedding_local::artifact::artifact_availability)
        .is_some_and(|availability| availability.is_ok());
    let remote = semantic.provider.is_remote();
    let model = semantic.resolved_model();
    let (dimension, _) = semantic.resolved_dimension();

    // Everything below is answered *without* opening a network, starting a
    // worker or reading a key's value. §53: a diagnostic that had to
    // reach a provider to describe itself would be the one command that turned
    // `lexical_only` into traffic.
    let (space_verifiable, credential_present, cache_directory) = if remote {
        let provider = remote_provider_id(semantic.provider);
        let space =
            provider.map(|id| noteit_embedding_remote::space::space_for(id, &model, dimension));
        (
            space
                .as_ref()
                .map(|space| space.artifact.is_verifiable())
                .unwrap_or(false),
            provider.is_some_and(|id| credential_is_present(id, &paths.config_dir)),
            space.as_ref().and_then(|space| {
                noteit_embedding_remote::cache::default_root()
                    .map(|root| noteit_embedding_remote::cache::cache_directory(&root, space))
            }),
        )
    } else {
        (true, false, None)
    };

    // Read before `paths` is moved into the report.
    let worker_running = remote && worker_is_reachable(&paths.runtime_dir);

    StatusReport {
        paths,
        semantic: SemanticStatusReport {
            mode: semantic.mode,
            provider: semantic.provider,
            fallback: semantic.fallback,
            enabled: semantic.semantic_is_enabled(),
            model,
            artifact_present,
            artifact_directory: directory,
            remote,
            dimension,
            space_verifiable,
            credential_present,
            // The CLI never starts a worker, and until 4.3D.R1 it reported a
            // flat `false` on the grounds that it has none *of its own*. That
            // was the wrong question: the socket is one per session, so a
            // worker started by the MCP server is reachable from here, and a
            // status that said "not started" while one was answering questions
            // described this process's bookkeeping rather than the machine.
            //
            // This connects to a Unix socket and hangs up. It starts nothing,
            // sends no request and costs no money — which is what §53
            // requires of a diagnostic.
            worker_running,
            cache_directory,
        },
    }
}

/// The wire identifier for a configured provider, when it has one.
fn remote_provider_id(
    provider: noteit_core::settings::SemanticProvider,
) -> Option<noteit_embedding_remote::RemoteProviderId> {
    noteit_embedding_remote::RemoteProviderId::parse(provider.as_str())
}

/// Whether a worker is answering on this session's socket.
///
/// Observed, never caused: a connect and a hang-up on a Unix socket. A path
/// that merely *exists* would be the wrong answer — a crashed worker leaves
/// exactly that behind.
fn worker_is_reachable(runtime_dir: &std::path::Path) -> bool {
    noteit_embedding_remote::client::socket_is_live(
        &runtime_dir.join(noteit_embedding_remote::worker::SOCKET_NAME),
    )
}

/// Whether a key exists for this provider — never what it is.
///
/// Reads only enough to answer yes or no, and the value is dropped on the next
/// line. `Credential` has no `Debug` and no `Display`, so there is no way for
/// it to reach a rendered status even by accident (§53, §22).
fn credential_is_present(
    provider: noteit_embedding_remote::RemoteProviderId,
    config_dir: &std::path::Path,
) -> bool {
    noteit_embedding_remote::credential_present(provider, config_dir)
}

fn execute(parsed: CliArgs, stdin: StdinSource<'_>) -> Executed {
    let Some(command) = parsed.command else {
        return Executed::ok(Command::Welcome, Outcome::Welcome);
    };

    match command {
        CliCommand::Ajuda => Executed::ok(Command::Help, Outcome::Help(HelpText::Own)),
        CliCommand::Versao => Executed::ok(Command::Version, Outcome::Version),
        CliCommand::Status => Executed::ok(Command::Status, Outcome::Status(Box::new(status()))),

        CliCommand::Listar {
            limite,
            tag,
            propriedade,
        } => {
            let filter = match parse_filter(tag, &propriedade) {
                Ok(filter) => filter,
                Err(error) => return usage_failure(Command::List, error),
            };
            let core = NoteItCore::open_read_only();
            match core.list_summaries(&filter, limite) {
                Ok(batch) => Executed::ok(Command::List, Outcome::Notes(batch)),
                Err(detail) => Executed::failed(
                    Some(Command::List),
                    CommandError::Read(ReadError::Listing { detail }),
                ),
            }
        }

        CliCommand::Ler { id } => {
            let core = NoteItCore::open_read_only();
            let resolved = match core.resolve_note_id(&id) {
                Ok(resolved) => resolved,
                Err(error) => {
                    return Executed::failed(
                        Some(Command::Read),
                        CommandError::Read(ReadError::Selector(error)),
                    )
                }
            };
            match core.read_note(&resolved) {
                Ok(document) => match NoteRevision::for_document(&document) {
                    Ok(revision) => Executed::ok(
                        Command::Read,
                        Outcome::Note {
                            document: Box::new(document),
                            revision,
                        },
                    ),
                    // A note that cannot be serialised cannot be given a
                    // version, and answering without one would invite a write
                    // built on a base nobody can name.
                    Err(detail) => Executed::failed(
                        Some(Command::Read),
                        CommandError::Read(ReadError::NoteRead { detail }),
                    ),
                },
                Err(detail) => Executed::failed(
                    Some(Command::Read),
                    CommandError::Read(ReadError::NoteRead { detail }),
                ),
            }
        }

        CliCommand::Buscar {
            consulta,
            limite,
            tag,
            propriedade,
        } => {
            let filter = match parse_filter(tag, &propriedade) {
                Ok(filter) => filter,
                Err(error) => return usage_failure(Command::Search, error),
            };
            let core = NoteItCore::open_read_only();
            match core.search_notes_filtered(&consulta, &filter, limite) {
                Ok(batch) => Executed::ok(
                    Command::Search,
                    Outcome::Search {
                        query: consulta,
                        batch,
                    },
                ),
                Err(detail) => Executed::failed(
                    Some(Command::Search),
                    CommandError::Read(ReadError::Listing { detail }),
                ),
            }
        }

        CliCommand::Tags { command: None } => {
            let core = NoteItCore::open_read_only();
            match core.metadata_catalog_with_warnings() {
                Ok((catalog, warnings)) => {
                    Executed::ok(Command::Tags, Outcome::Tags { catalog, warnings })
                }
                Err(detail) => Executed::failed(
                    Some(Command::Tags),
                    CommandError::Read(ReadError::Listing { detail }),
                ),
            }
        }

        CliCommand::Propriedades { command: None } => {
            let core = NoteItCore::open_read_only();
            match core.metadata_catalog_with_warnings() {
                Ok((catalog, warnings)) => Executed::ok(
                    Command::Properties,
                    Outcome::Properties { catalog, warnings },
                ),
                Err(detail) => Executed::failed(
                    Some(Command::Properties),
                    CommandError::Read(ReadError::Listing { detail }),
                ),
            }
        }

        CliCommand::Tarefas {
            estado,
            limite,
            tag,
            propriedade,
            command: None,
        } => {
            let filter = match parse_filter(tag, &propriedade) {
                Ok(filter) => filter,
                Err(error) => return usage_failure(Command::Tasks, error),
            };
            let core = NoteItCore::open_read_only();
            let state = estado.into();
            match core.list_tasks(state, &filter, limite) {
                Ok(batch) => Executed::ok(Command::Tasks, Outcome::Tasks { state, batch }),
                Err(detail) => Executed::failed(
                    Some(Command::Tasks),
                    CommandError::Read(ReadError::Listing { detail }),
                ),
            }
        }

        CliCommand::Lixeira { command: None } => {
            let core = NoteItCore::open_read_only();
            Executed::ok(Command::Trash, Outcome::Trash(core.list_trash()))
        }

        // ---- Write API. Everything below changes the store, and every one of
        // them goes through the same authority decision.
        CliCommand::Criar {
            texto,
            stdin: from_stdin,
            tag,
            propriedade,
        } => {
            // A note with nothing in it is a legitimate thing to ask for, and
            // is exactly what the interface's own new note is.
            let content = match read_payload(texto, from_stdin, stdin) {
                Ok(payload) => payload.unwrap_or_default(),
                Err(error) => return Executed::failed(Some(Command::Create), error),
            };
            let mut properties = Vec::new();
            for raw in &propriedade {
                match NoteFilter::parse_property_arg(raw) {
                    Ok((key, value)) => properties.push(NoteProperty { key, value }),
                    Err(error) => return usage_failure(Command::Create, error),
                }
            }
            perform(
                Command::Create,
                WriteOperation::CreateNote {
                    draft: NoteDraft {
                        content,
                        tags: tag,
                        properties,
                    },
                },
            )
        }

        CliCommand::Adicionar {
            id,
            texto,
            stdin: from_stdin,
            if_revision,
        } => {
            let expected_revision = match precondition(Command::Append, if_revision) {
                Ok(expected) => expected,
                Err(failure) => return failure,
            };
            let payload = match read_payload(texto, from_stdin, stdin) {
                Ok(Some(payload)) => payload,
                Ok(None) => {
                    return usage_failure(
                        Command::Append,
                        "informe o texto a acrescentar, como argumento ou com `--stdin`.",
                    )
                }
                Err(error) => return Executed::failed(Some(Command::Append), error),
            };
            perform(
                Command::Append,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::Append { payload },
                    expected_revision,
                },
            )
        }

        CliCommand::Editar {
            id,
            texto,
            stdin: from_stdin,
            vazio,
            if_revision,
        } => {
            let expected_revision = match precondition(Command::Edit, if_revision) {
                Ok(expected) => expected,
                Err(failure) => return failure,
            };
            // Emptying a note is asked for by name and never by accident. An
            // empty pipe is a mistake far more often than it is an
            // instruction, and the note it would destroy is not recoverable
            // from the command line.
            if vazio && (texto.is_some() || from_stdin) {
                return usage_failure(
                    Command::Edit,
                    "`--vazio` esvazia a nota e por isso não aceita texto junto.",
                );
            }
            let mutation = if vazio {
                NoteMutation::ClearBody
            } else {
                let body = match read_payload(texto, from_stdin, stdin) {
                    Ok(Some(body)) => body,
                    Ok(None) => {
                        return usage_failure(
                            Command::Edit,
                            "informe o novo corpo, como argumento ou com `--stdin`. \
                             Para esvaziar a nota use `--vazio`.",
                        )
                    }
                    Err(error) => return Executed::failed(Some(Command::Edit), error),
                };
                if noteit_core::NoteDocument::canonical_content(&body).is_empty() {
                    return usage_failure(
                        Command::Edit,
                        "o novo corpo está vazio. Para esvaziar a nota de propósito use `--vazio`.",
                    );
                }
                NoteMutation::ReplaceBody { body }
            };
            perform(
                Command::Edit,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation,
                    expected_revision,
                },
            )
        }

        CliCommand::Tags {
            command:
                Some(TagsCommand::Adicionar {
                    id,
                    tag,
                    if_revision,
                }),
        } => {
            let expected_revision = match precondition(Command::TagAdd, if_revision) {
                Ok(expected) => expected,
                Err(failure) => return failure,
            };
            perform(
                Command::TagAdd,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::AddTag { tag },
                    expected_revision,
                },
            )
        }

        CliCommand::Tags {
            command:
                Some(TagsCommand::Remover {
                    id,
                    tag,
                    if_revision,
                }),
        } => {
            let expected_revision = match precondition(Command::TagRemove, if_revision) {
                Ok(expected) => expected,
                Err(failure) => return failure,
            };
            perform(
                Command::TagRemove,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::RemoveTag { tag },
                    expected_revision,
                },
            )
        }

        CliCommand::Propriedades {
            command:
                Some(PropertiesCommand::Definir {
                    id,
                    atribuicao,
                    if_revision,
                }),
        } => {
            let expected_revision = match precondition(Command::PropertySet, if_revision) {
                Ok(expected) => expected,
                Err(failure) => return failure,
            };
            let (key, value) = match NoteFilter::parse_property_arg(&atribuicao) {
                Ok(pair) => pair,
                Err(error) => return usage_failure(Command::PropertySet, error),
            };
            perform(
                Command::PropertySet,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::SetProperty { key, value },
                    expected_revision,
                },
            )
        }

        CliCommand::Propriedades {
            command:
                Some(PropertiesCommand::Remover {
                    id,
                    chave,
                    if_revision,
                }),
        } => {
            let expected_revision = match precondition(Command::PropertyRemove, if_revision) {
                Ok(expected) => expected,
                Err(failure) => return failure,
            };
            perform(
                Command::PropertyRemove,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::RemoveProperty { key: chave },
                    expected_revision,
                },
            )
        }

        CliCommand::Tarefas {
            command:
                Some(TasksCommand::Concluir {
                    id,
                    referencia,
                    if_revision,
                }),
            ..
        } => {
            let expected_revision = match precondition(Command::TaskComplete, if_revision) {
                Ok(expected) => expected,
                Err(failure) => return failure,
            };
            perform(
                Command::TaskComplete,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::CompleteTask {
                        task_ref: referencia,
                    },
                    expected_revision,
                },
            )
        }

        CliCommand::Tarefas {
            command:
                Some(TasksCommand::Reabrir {
                    id,
                    referencia,
                    if_revision,
                }),
            ..
        } => {
            let expected_revision = match precondition(Command::TaskReopen, if_revision) {
                Ok(expected) => expected,
                Err(failure) => return failure,
            };
            perform(
                Command::TaskReopen,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::ReopenTask {
                        task_ref: referencia,
                    },
                    expected_revision,
                },
            )
        }

        CliCommand::Lixeira {
            command: Some(TrashCommand::Restaurar { id }),
        } => perform(
            Command::TrashRestore,
            WriteOperation::RestoreFromTrash { selector: id },
        ),
    }
}

/// Reads everything on standard input, as bytes that are text.
///
/// Nothing is trimmed, reflowed or sanitized here. What arrives is what the
/// person piped in, and Markdown is allowed to contain anything Markdown
/// contains — see [`output::sanitize_for_terminal`], which is about *showing*
/// text and is deliberately not applied to text on its way into a note.
pub fn read_process_stdin() -> Result<String, String> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|error| format!("não foi possível ler a entrada padrão: {error}"))?;
    Ok(buffer)
}

/// Reads the `--if-revision` argument into a precondition.
///
/// A revision that is not one is a usage error and never `None`. That
/// distinction is the whole safety of the flag: if a malformed token quietly
/// became "no precondition", a client with a corrupted revision would be handed
/// an unconditional write over a note it has not looked at.
fn precondition(command: Command, raw: Option<String>) -> Result<Option<NoteRevision>, Executed> {
    match raw {
        None => Ok(None),
        Some(raw) => match NoteRevision::parse(&raw) {
            Ok(revision) => Ok(Some(revision)),
            Err(error) => Err(usage_failure(command, error.to_string())),
        },
    }
}

/// A usage error, spelled the way every other one in this CLI is.
fn usage_failure(command: Command, message: impl Into<String>) -> Executed {
    Executed::failed(
        Some(command),
        CommandError::Usage(UsageError::detail(message)),
    )
}

/// The text a write command was given, from an argument or from standard input.
///
/// The two are mutually exclusive on purpose: a command given both has been
/// asked for two different things, and picking one silently is how the wrong
/// text ends up in a note. `None` means neither was supplied, which each
/// command answers for itself.
fn read_payload(
    argument: Option<String>,
    from_stdin: bool,
    stdin: StdinSource<'_>,
) -> Result<Option<String>, CommandError> {
    match (argument, from_stdin) {
        (Some(_), true) => Err(CommandError::Usage(UsageError::detail(
            "informe o texto como argumento ou com `--stdin`, nunca os dois.",
        ))),
        (Some(text), false) => Ok(Some(text)),
        (None, true) => stdin()
            .map(Some)
            .map_err(|error| CommandError::Usage(UsageError::detail(error))),
        (None, false) => Ok(None),
    }
}

/// Runs one write operation and answers with its typed outcome.
fn perform(command: Command, operation: WriteOperation) -> Executed {
    match authority::perform(&operation) {
        Ok(performed) => Executed::ok(command, Outcome::Write(Box::new(performed.outcome))),
        Err(error) => Executed::failed(Some(command), CommandError::Write(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdout_of(args: &[&str]) -> String {
        let channels = Channels::plain();
        let response = run_with_args(args.to_vec(), &channels);
        assert_eq!(response.exit_code, EXIT_SUCCESS, "{}", response.stderr);
        assert!(response.stderr.is_empty(), "{}", response.stderr);
        response.stdout
    }

    #[test]
    fn dispatch_no_args_renders_the_presentation_with_success() {
        let result = stdout_of(&["noteit"]);
        assert!(result.contains("Note-it"), "{result}");
        assert!(
            result.contains(env!("CARGO_PKG_VERSION")),
            "the presentation must name the version: {result}"
        );
        assert!(result.contains("Comece por:"), "{result}");
        assert!(result.contains("noteit listar"), "{result}");
        assert!(result.contains("noteit ajuda"), "{result}");
    }

    #[test]
    fn dispatch_no_args_is_the_only_command_that_shows_the_presentation() {
        // The wordmark belongs to the empty command line. Anything else —
        // including the help, which is the screen most likely to be mistaken
        // for a place to put it — answers without it.
        let presentation = stdout_of(&["noteit"]);
        assert!(presentation.contains('\u{2588}'), "{presentation}");

        for arguments in [
            vec!["noteit", "ajuda"],
            vec!["noteit", "help"],
            vec!["noteit", "--help"],
            vec!["noteit", "versao"],
            vec!["noteit", "status"],
            vec!["noteit", "listar", "--help"],
        ] {
            let answer = stdout_of(&arguments);
            assert!(
                !answer.contains('\u{2588}') && !answer.contains("Comece por:"),
                "{arguments:?} showed the presentation"
            );
        }
    }

    #[test]
    fn dispatch_ajuda_and_help_render_identical_help() {
        let ajuda = stdout_of(&["noteit", "ajuda"]);
        assert_eq!(ajuda, stdout_of(&["noteit", "help"]));
        assert_eq!(ajuda, stdout_of(&["noteit", "--help"]));
        assert_eq!(ajuda, stdout_of(&["noteit", "-h"]));
    }

    #[test]
    fn dispatch_versao_and_version_render_identical_version() {
        let versao = stdout_of(&["noteit", "versao"]);
        assert_eq!(versao, stdout_of(&["noteit", "version"]));
        assert_eq!(versao, stdout_of(&["noteit", "--version"]));
        assert_eq!(versao, stdout_of(&["noteit", "-V"]));
    }

    #[test]
    fn dispatch_status_renders_status_report() {
        let status = stdout_of(&["noteit", "status"]);
        assert!(status.contains("CLI       pronta"));
        assert!(status.contains("Core      disponível"));
    }

    #[test]
    fn dispatch_invalid_command_returns_usage_error_in_portuguese() {
        let channels = Channels::plain();
        let response = run_with_args(["noteit", "batata"], &channels);
        assert_eq!(response.exit_code, EXIT_USAGE_ERROR);
        assert!(response.stdout.is_empty());
        assert!(response
            .stderr
            .contains("Erro: comando desconhecido `batata`."));
        assert!(response
            .stderr
            .contains("Use `noteit ajuda` para ver os comandos disponíveis."));
    }

    #[test]
    fn dispatch_invalid_flag_returns_usage_error_in_portuguese() {
        let channels = Channels::plain();
        let response = run_with_args(["noteit", "--flag-desconhecida"], &channels);
        assert_eq!(response.exit_code, EXIT_USAGE_ERROR);
        assert!(response
            .stderr
            .contains("Erro: opção desconhecida `--flag-desconhecida`."));
        assert!(response
            .stderr
            .contains("Use `noteit ajuda` para ver os comandos e opções disponíveis."));
    }

    #[test]
    fn dispatch_unexpected_argument_returns_usage_error_in_portuguese() {
        let channels = Channels::plain();
        let response = run_with_args(["noteit", "status", "argumento-inesperado"], &channels);
        assert_eq!(response.exit_code, EXIT_USAGE_ERROR);
        assert!(response
            .stderr
            .contains("Erro: argumento inesperado `argumento-inesperado`."));
        assert!(response
            .stderr
            .contains("Use `noteit ajuda` para ver o formato correto de uso."));
    }

    #[test]
    fn version_string_matches_workspace_cargo_pkg_version() {
        assert_eq!(
            stdout_of(&["noteit", "versao"]),
            format!("Note-it {}\n", env!("CARGO_PKG_VERSION"))
        );
    }

    // --- the machine flag ---------------------------------------------------

    fn os(args: &[&str]) -> Vec<std::ffi::OsString> {
        args.iter().map(std::ffi::OsString::from).collect()
    }

    #[test]
    fn the_raw_scan_and_the_parser_agree_about_what_asked_for_json() {
        for args in [
            vec!["noteit"],
            vec!["noteit", "--json"],
            vec!["noteit", "--json", "listar"],
            vec!["noteit", "listar", "--json"],
            vec!["noteit", "tags", "adicionar", "1234abcd", "x", "--json"],
            vec!["noteit", "criar", "--json", "--", "texto"],
            vec!["noteit", "criar", "--", "--json"],
            vec!["noteit", "adicionar", "1234abcd", "--", "--json"],
            vec!["noteit", "--json", "criar", "--", "--json"],
        ] {
            let scanned = machine_mode_requested(&os(&args));
            let parsed = CliArgs::try_parse_from(&args)
                .unwrap_or_else(|error| panic!("{args:?} must parse: {error}"))
                .json;
            assert_eq!(
                scanned, parsed,
                "{args:?}: the fallback scan and the parser disagree"
            );
        }
    }

    #[test]
    fn the_escape_protects_the_payload_even_when_the_parse_fails() {
        // Two escapes in a row is bad usage, and it is bad usage *in
        // Portuguese*: nothing after the first `--` may turn the machine
        // interface on, including on the path where the fallback scan decides.
        let args = os(&["noteit", "criar", "--", "--", "--json"]);
        assert!(!machine_mode_requested(&args));
        assert!(CliArgs::try_parse_from(&args).is_err());

        let response = run_with_args(args, &Channels::plain());
        assert_eq!(response.exit_code, EXIT_USAGE_ERROR);
        assert!(response.stdout.is_empty());
        assert!(response.stderr.starts_with("Erro: "), "{}", response.stderr);
    }

    #[test]
    fn a_literal_json_argument_after_the_escape_is_payload_and_not_a_mode() {
        let parsed = CliArgs::try_parse_from(["noteit", "adicionar", "1234abcd", "--", "--json"])
            .expect("parse");
        assert!(!parsed.json, "`--` did not protect the payload");
        assert!(
            matches!(parsed.command, Some(CliCommand::Adicionar { texto: Some(text), .. }) if text == "--json")
        );
    }

    #[test]
    fn machine_documents_carry_no_styling_even_in_a_styled_context() {
        // `Channels::styled()` is what an attached terminal produces, and
        // the machine interface must be indifferent to it: JSON is data, and a
        // consumer never asked for colour. `NO_COLOR` is beside the point here
        // for the same reason — there is nothing to turn off.
        let styled = Channels::styled();
        for arguments in [
            vec!["noteit", "--json"],
            vec!["noteit", "--json", "ajuda"],
            vec!["noteit", "--json", "versao"],
            vec!["noteit", "--json", "status"],
            vec!["noteit", "--json", "batata"],
        ] {
            let response = run_with_args(arguments.clone(), &styled);
            for channel in [&response.stdout, &response.stderr] {
                assert!(
                    !channel.contains('\u{1b}'),
                    "{arguments:?} produced styling in machine mode: {channel:?}"
                );
            }
        }

        // And the human adapter still styles, so the test above proves
        // something.
        let human = run_with_args(["noteit", "versao"], &styled);
        let human_help = run_with_args(["noteit", "ajuda"], &styled);
        assert!(!human.stdout.contains('\u{1b}'), "version is never styled");
        assert!(
            human_help.stdout.contains('\u{1b}'),
            "the human help stopped styling"
        );
    }

    #[test]
    fn the_machine_flag_does_not_change_which_help_is_shown() {
        assert_eq!(significant_argument_count(&os(&["noteit", "--help"])), 2);
        assert_eq!(
            significant_argument_count(&os(&["noteit", "--json", "--help"])),
            2
        );
        assert_eq!(
            significant_argument_count(&os(&["noteit", "listar", "--help"])),
            3
        );
        assert_eq!(
            significant_argument_count(&os(&["noteit", "listar", "--json", "--help"])),
            3
        );
        // After the escape the flag is a value like any other and counts.
        assert_eq!(
            significant_argument_count(&os(&["noteit", "criar", "--", "--json"])),
            4
        );
    }
}
