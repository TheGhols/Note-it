pub mod authority;
pub mod cli;
pub mod machine;
pub mod outcome;
pub mod output;

use clap::error::ErrorKind;
use clap::Parser;
use cli::{CliArgs, CliCommand, PropertiesCommand, TagsCommand, TasksCommand, TrashCommand};
use noteit_core::write::{NoteDraft, NoteMutation, WriteOperation};
use noteit_core::{NoteFilter, NoteItCore, NoteProperty, StorePaths};
use outcome::{
    CliResponse, Command, CommandError, Executed, HelpText, Outcome, ReadError, UsageError,
};
use output::OutputContext;
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
pub fn run_with_args<I, T>(args: I, ctx: &OutputContext) -> CliResponse
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    run_with_args_and_stdin(args, ctx, &read_process_stdin)
}

/// The whole dispatcher, with standard input supplied explicitly.
pub fn run_with_args_and_stdin<I, T>(
    args: I,
    ctx: &OutputContext,
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
        output::render(&executed, ctx)
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
fn execute(parsed: CliArgs, stdin: StdinSource<'_>) -> Executed {
    let Some(command) = parsed.command else {
        return Executed::ok(Command::Welcome, Outcome::Welcome);
    };

    match command {
        CliCommand::Ajuda => Executed::ok(Command::Help, Outcome::Help(HelpText::Own)),
        CliCommand::Versao => Executed::ok(Command::Version, Outcome::Version),
        CliCommand::Status => Executed::ok(
            Command::Status,
            Outcome::Status(Box::new(StorePaths::resolve())),
        ),

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
                Ok(document) => Executed::ok(Command::Read, Outcome::Note(Box::new(document))),
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
            Executed::ok(Command::Tags, Outcome::Tags(core.metadata_catalog()))
        }

        CliCommand::Propriedades { command: None } => {
            let core = NoteItCore::open_read_only();
            Executed::ok(
                Command::Properties,
                Outcome::Properties(core.metadata_catalog()),
            )
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
        } => {
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
                },
            )
        }

        CliCommand::Editar {
            id,
            texto,
            stdin: from_stdin,
            vazio,
        } => {
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
                },
            )
        }

        CliCommand::Tags {
            command: Some(TagsCommand::Adicionar { id, tag }),
        } => perform(
            Command::TagAdd,
            WriteOperation::MutateNote {
                selector: id,
                mutation: NoteMutation::AddTag { tag },
            },
        ),

        CliCommand::Tags {
            command: Some(TagsCommand::Remover { id, tag }),
        } => perform(
            Command::TagRemove,
            WriteOperation::MutateNote {
                selector: id,
                mutation: NoteMutation::RemoveTag { tag },
            },
        ),

        CliCommand::Propriedades {
            command: Some(PropertiesCommand::Definir { id, atribuicao }),
        } => {
            let (key, value) = match NoteFilter::parse_property_arg(&atribuicao) {
                Ok(pair) => pair,
                Err(error) => return usage_failure(Command::PropertySet, error),
            };
            perform(
                Command::PropertySet,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::SetProperty { key, value },
                },
            )
        }

        CliCommand::Propriedades {
            command: Some(PropertiesCommand::Remover { id, chave }),
        } => perform(
            Command::PropertyRemove,
            WriteOperation::MutateNote {
                selector: id,
                mutation: NoteMutation::RemoveProperty { key: chave },
            },
        ),

        CliCommand::Tarefas {
            command: Some(TasksCommand::Concluir { id, referencia }),
            ..
        } => perform(
            Command::TaskComplete,
            WriteOperation::MutateNote {
                selector: id,
                mutation: NoteMutation::CompleteTask {
                    task_ref: referencia,
                },
            },
        ),

        CliCommand::Tarefas {
            command: Some(TasksCommand::Reabrir { id, referencia }),
            ..
        } => perform(
            Command::TaskReopen,
            WriteOperation::MutateNote {
                selector: id,
                mutation: NoteMutation::ReopenTask {
                    task_ref: referencia,
                },
            },
        ),

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
        let ctx = OutputContext::plain();
        let response = run_with_args(args.to_vec(), &ctx);
        assert_eq!(response.exit_code, EXIT_SUCCESS, "{}", response.stderr);
        assert!(response.stderr.is_empty(), "{}", response.stderr);
        response.stdout
    }

    #[test]
    fn dispatch_no_args_renders_welcome_with_success() {
        let result = stdout_of(&["noteit"]);
        assert!(result.contains("Note-it"));
        assert!(result.contains("Use `noteit ajuda` para começar."));
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
        let ctx = OutputContext::plain();
        let response = run_with_args(["noteit", "batata"], &ctx);
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
        let ctx = OutputContext::plain();
        let response = run_with_args(["noteit", "--flag-desconhecida"], &ctx);
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
        let ctx = OutputContext::plain();
        let response = run_with_args(["noteit", "status", "argumento-inesperado"], &ctx);
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

        let response = run_with_args(args, &OutputContext::plain());
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
        // `OutputContext::styled()` is what an attached terminal produces, and
        // the machine interface must be indifferent to it: JSON is data, and a
        // consumer never asked for colour. `NO_COLOR` is beside the point here
        // for the same reason — there is nothing to turn off.
        let styled = OutputContext::styled();
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
