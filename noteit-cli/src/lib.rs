pub mod authority;
pub mod cli;
pub mod output;

use clap::error::ErrorKind;
use clap::Parser;
use cli::{CliArgs, CliCommand, PropertiesCommand, TagsCommand, TasksCommand, TrashCommand};
use noteit_core::write::{NoteDraft, NoteMutation, WriteOperation};
use noteit_core::{NoteFilter, NoteItCore, NoteProperty, NoteSelectorError, StorePaths};
use output::OutputContext;
use std::io::Read;

pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_EXECUTION_ERROR: u8 = 1;
pub const EXIT_USAGE_ERROR: u8 = 2;

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
/// Returns `Ok(stdout_string)` on success, or `Err((exit_code, stderr_string))` on failure.
pub fn run_with_args<I, T>(args: I, ctx: &OutputContext) -> Result<String, (u8, String)>
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
) -> Result<String, (u8, String)>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args_vec: Vec<std::ffi::OsString> = args.into_iter().map(|a| a.into()).collect();

    match CliArgs::try_parse_from(&args_vec) {
        Ok(parsed) => match parsed.command {
            None => Ok(output::render_welcome(ctx)),
            Some(CliCommand::Ajuda) => Ok(output::render_help(ctx)),
            Some(CliCommand::Versao) => Ok(output::render_version(ctx)),
            Some(CliCommand::Status) => {
                let paths = StorePaths::resolve();
                Ok(output::render_status(ctx, &paths))
            }
            Some(CliCommand::Listar {
                limite,
                tag,
                propriedade,
            }) => {
                let filter = parse_filter(tag, &propriedade).map_err(|err| {
                    let sanitized = output::sanitize_for_terminal(&err);
                    (
                        EXIT_USAGE_ERROR,
                        format!(
                            "{} {}\n\nUse `{}` para ver o formato correto.\n",
                            ctx.bold("Erro:"),
                            sanitized,
                            ctx.bold("noteit ajuda")
                        ),
                    )
                })?;
                let core = NoteItCore::open_read_only();
                match core.list_summaries(&filter, limite) {
                    Ok(batch) => {
                        for w in &batch.warnings {
                            eprint!("{}", output::render_warning(ctx, w));
                        }
                        Ok(output::render_notes_list(ctx, &batch.items))
                    }
                    Err(err) => {
                        let sanitized = output::sanitize_for_terminal(&err);
                        Err((
                            EXIT_EXECUTION_ERROR,
                            format!("{} {}\n", ctx.bold("Erro:"), sanitized),
                        ))
                    }
                }
            }
            Some(CliCommand::Ler { id }) => {
                let core = NoteItCore::open_read_only();
                let resolved_id = core.resolve_note_id(&id).map_err(|err| {
                    let sanitized_err = match &err {
                        NoteSelectorError::InvalidFormat(sel) => {
                            let s = output::sanitize_for_terminal(sel);
                            format!("formato de seletor inválido `{s}`. Forneça um UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais.")
                        }
                        NoteSelectorError::NotFound(sel) => {
                            let s = output::sanitize_for_terminal(sel);
                            format!("nenhuma nota encontrada para o seletor `{s}`.")
                        }
                        NoteSelectorError::Ambiguous(sel, matches) => {
                            let s = output::sanitize_for_terminal(sel);
                            let count = matches.len();
                            format!("seletor ambíguo `{s}` corresponde a {count} notas vivas.")
                        }
                        NoteSelectorError::SymlinkRefused(sel) => {
                            let s = output::sanitize_for_terminal(sel);
                            format!("a nota `{s}` é um link simbólico e não pode ser aberta.")
                        }
                        NoteSelectorError::StoreUnavailable(reason) => {
                            let r = output::sanitize_for_terminal(reason);
                            format!("repositório indisponível: {r}")
                        }
                    };
                    (
                        EXIT_EXECUTION_ERROR,
                        format!("{} {}\n", ctx.bold("Erro:"), sanitized_err),
                    )
                })?;
                match core.read_note(&resolved_id) {
                    Ok(doc) => Ok(output::render_note_read(ctx, &doc)),
                    Err(err) => {
                        let sanitized = output::sanitize_for_terminal(&err);
                        Err((
                            EXIT_EXECUTION_ERROR,
                            format!("{} {}\n", ctx.bold("Erro:"), sanitized),
                        ))
                    }
                }
            }
            Some(CliCommand::Buscar {
                consulta,
                limite,
                tag,
                propriedade,
            }) => {
                let filter = parse_filter(tag, &propriedade).map_err(|err| {
                    let sanitized = output::sanitize_for_terminal(&err);
                    (
                        EXIT_USAGE_ERROR,
                        format!(
                            "{} {}\n\nUse `{}` para ver o formato correto.\n",
                            ctx.bold("Erro:"),
                            sanitized,
                            ctx.bold("noteit ajuda")
                        ),
                    )
                })?;
                let core = NoteItCore::open_read_only();
                match core.search_notes_filtered(&consulta, &filter, limite) {
                    Ok(batch) => {
                        for w in &batch.warnings {
                            eprint!("{}", output::render_warning(ctx, w));
                        }
                        Ok(output::render_search_results(ctx, &consulta, &batch.items))
                    }
                    Err(err) => {
                        let sanitized = output::sanitize_for_terminal(&err);
                        Err((
                            EXIT_EXECUTION_ERROR,
                            format!("{} {}\n", ctx.bold("Erro:"), sanitized),
                        ))
                    }
                }
            }
            Some(CliCommand::Tags { command: None }) => {
                let core = NoteItCore::open_read_only();
                let catalog = core.metadata_catalog();
                Ok(output::render_tags(ctx, &catalog))
            }
            Some(CliCommand::Propriedades { command: None }) => {
                let core = NoteItCore::open_read_only();
                let catalog = core.metadata_catalog();
                Ok(output::render_properties(ctx, &catalog))
            }
            Some(CliCommand::Tarefas {
                estado,
                limite,
                tag,
                propriedade,
                command: None,
            }) => {
                let filter = parse_filter(tag, &propriedade).map_err(|err| {
                    let sanitized = output::sanitize_for_terminal(&err);
                    (
                        EXIT_USAGE_ERROR,
                        format!(
                            "{} {}\n\nUse `{}` para ver o formato correto.\n",
                            ctx.bold("Erro:"),
                            sanitized,
                            ctx.bold("noteit ajuda")
                        ),
                    )
                })?;
                let core = NoteItCore::open_read_only();
                let state_filter = estado.into();
                match core.list_tasks(state_filter, &filter, limite) {
                    Ok(batch) => {
                        for w in &batch.warnings {
                            eprint!("{}", output::render_warning(ctx, w));
                        }
                        Ok(output::render_tasks(ctx, &batch.items, state_filter))
                    }
                    Err(err) => {
                        let sanitized = output::sanitize_for_terminal(&err);
                        Err((
                            EXIT_EXECUTION_ERROR,
                            format!("{} {}\n", ctx.bold("Erro:"), sanitized),
                        ))
                    }
                }
            }
            Some(CliCommand::Lixeira { command: None }) => {
                let core = NoteItCore::open_read_only();
                let trash = core.list_trash();
                Ok(output::render_trash(ctx, &trash))
            }

            // ---- Write API. Everything below changes the store, and every
            // one of them goes through the same authority decision.
            Some(CliCommand::Criar {
                texto,
                stdin: from_stdin,
                tag,
                propriedade,
            }) => {
                // A note with nothing in it is a legitimate thing to ask for,
                // and is exactly what the interface's own new note is.
                let content: String =
                    read_payload(ctx, texto, from_stdin, stdin)?.unwrap_or_default();
                let mut properties = Vec::new();
                for raw in &propriedade {
                    let (key, value) =
                        NoteFilter::parse_property_arg(raw).map_err(|err| usage(ctx, &err))?;
                    properties.push(NoteProperty { key, value });
                }
                perform(
                    ctx,
                    WriteOperation::CreateNote {
                        draft: NoteDraft {
                            content,
                            tags: tag,
                            properties,
                        },
                    },
                )
            }

            Some(CliCommand::Adicionar {
                id,
                texto,
                stdin: from_stdin,
            }) => {
                let Some(payload) = read_payload(ctx, texto, from_stdin, stdin)? else {
                    return Err(usage(
                        ctx,
                        "informe o texto a acrescentar, como argumento ou com `--stdin`.",
                    ));
                };
                perform(
                    ctx,
                    WriteOperation::MutateNote {
                        selector: id,
                        mutation: NoteMutation::Append { payload },
                    },
                )
            }

            Some(CliCommand::Editar {
                id,
                texto,
                stdin: from_stdin,
                vazio,
            }) => {
                // Emptying a note is asked for by name and never by accident.
                // An empty pipe is a mistake far more often than it is an
                // instruction, and the note it would destroy is not
                // recoverable from the command line.
                if vazio && (texto.is_some() || from_stdin) {
                    return Err(usage(
                        ctx,
                        "`--vazio` esvazia a nota e por isso não aceita texto junto.",
                    ));
                }
                let mutation = if vazio {
                    NoteMutation::ClearBody
                } else {
                    let Some(body) = read_payload(ctx, texto, from_stdin, stdin)? else {
                        return Err(usage(
                            ctx,
                            "informe o novo corpo, como argumento ou com `--stdin`. \
                             Para esvaziar a nota use `--vazio`.",
                        ));
                    };
                    if noteit_core::NoteDocument::canonical_content(&body).is_empty() {
                        return Err(usage(
                            ctx,
                            "o novo corpo está vazio. Para esvaziar a nota de propósito use `--vazio`.",
                        ));
                    }
                    NoteMutation::ReplaceBody { body }
                };
                perform(
                    ctx,
                    WriteOperation::MutateNote {
                        selector: id,
                        mutation,
                    },
                )
            }

            Some(CliCommand::Tags {
                command: Some(TagsCommand::Adicionar { id, tag }),
            }) => perform(
                ctx,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::AddTag { tag },
                },
            ),

            Some(CliCommand::Tags {
                command: Some(TagsCommand::Remover { id, tag }),
            }) => perform(
                ctx,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::RemoveTag { tag },
                },
            ),

            Some(CliCommand::Propriedades {
                command: Some(PropertiesCommand::Definir { id, atribuicao }),
            }) => {
                let (key, value) =
                    NoteFilter::parse_property_arg(&atribuicao).map_err(|err| usage(ctx, &err))?;
                perform(
                    ctx,
                    WriteOperation::MutateNote {
                        selector: id,
                        mutation: NoteMutation::SetProperty { key, value },
                    },
                )
            }

            Some(CliCommand::Propriedades {
                command: Some(PropertiesCommand::Remover { id, chave }),
            }) => perform(
                ctx,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::RemoveProperty { key: chave },
                },
            ),

            Some(CliCommand::Tarefas {
                command: Some(TasksCommand::Concluir { id, referencia }),
                ..
            }) => perform(
                ctx,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::CompleteTask {
                        task_ref: referencia,
                    },
                },
            ),

            Some(CliCommand::Tarefas {
                command: Some(TasksCommand::Reabrir { id, referencia }),
                ..
            }) => perform(
                ctx,
                WriteOperation::MutateNote {
                    selector: id,
                    mutation: NoteMutation::ReopenTask {
                        task_ref: referencia,
                    },
                },
            ),

            Some(CliCommand::Lixeira {
                command: Some(TrashCommand::Restaurar { id }),
            }) => perform(ctx, WriteOperation::RestoreFromTrash { selector: id }),
        },
        Err(err) => match err.kind() {
            ErrorKind::DisplayHelp => {
                // If the user requested help for a specific subcommand, show clap's subcommand help
                if args_vec.len() > 2 {
                    Ok(err.render().to_string())
                } else {
                    Ok(output::render_help(ctx))
                }
            }
            ErrorKind::DisplayVersion => Ok(output::render_version(ctx)),
            _ => Err((EXIT_USAGE_ERROR, output::render_error(ctx, &err))),
        },
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
fn usage(ctx: &OutputContext, message: &str) -> (u8, String) {
    (
        EXIT_USAGE_ERROR,
        format!(
            "{} {}\n\nUse `{}` para ver o formato correto.\n",
            ctx.bold("Erro:"),
            output::sanitize_for_terminal(message),
            ctx.bold("noteit ajuda")
        ),
    )
}

/// The text a write command was given, from an argument or from standard input.
///
/// The two are mutually exclusive on purpose: a command given both has been
/// asked for two different things, and picking one silently is how the wrong
/// text ends up in a note. `None` means neither was supplied, which each
/// command answers for itself.
fn read_payload(
    ctx: &OutputContext,
    argument: Option<String>,
    from_stdin: bool,
    stdin: StdinSource<'_>,
) -> Result<Option<String>, (u8, String)> {
    match (argument, from_stdin) {
        (Some(_), true) => Err(usage(
            ctx,
            "informe o texto como argumento ou com `--stdin`, nunca os dois.",
        )),
        (Some(text), false) => Ok(Some(text)),
        (None, true) => stdin().map(Some).map_err(|error| usage(ctx, &error)),
        (None, false) => Ok(None),
    }
}

/// Runs one write operation and turns its outcome into what a person reads.
fn perform(ctx: &OutputContext, operation: WriteOperation) -> Result<String, (u8, String)> {
    match authority::perform(&operation) {
        Ok(performed) => {
            // A committed write whose window could not be refreshed is still a
            // committed write. The warning goes to stderr and the success line
            // to stdout, so nothing about it reads as "try that again".
            if let Some(warning) = &performed.outcome.ui_sync_warning {
                eprint!("{}", output::render_write_warning(ctx, warning));
            }
            Ok(output::render_write_outcome(ctx, &performed.outcome))
        }
        Err(error) => Err((
            output::exit_code_for_write_error(&error),
            output::render_write_error(ctx, &error),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_no_args_renders_welcome_with_success() {
        let ctx = OutputContext::plain();
        let result = run_with_args(["noteit"], &ctx).expect("success");
        assert!(result.contains("Note-it"));
        assert!(result.contains("Use `noteit ajuda` para começar."));
    }

    #[test]
    fn dispatch_ajuda_and_help_render_identical_help() {
        let ctx = OutputContext::plain();
        let ajuda = run_with_args(["noteit", "ajuda"], &ctx).expect("ajuda");
        let help = run_with_args(["noteit", "help"], &ctx).expect("help");
        let flag_help = run_with_args(["noteit", "--help"], &ctx).expect("--help");
        let short_help = run_with_args(["noteit", "-h"], &ctx).expect("-h");

        assert_eq!(ajuda, help);
        assert_eq!(ajuda, flag_help);
        assert_eq!(ajuda, short_help);
    }

    #[test]
    fn dispatch_versao_and_version_render_identical_version() {
        let ctx = OutputContext::plain();
        let versao = run_with_args(["noteit", "versao"], &ctx).expect("versao");
        let version = run_with_args(["noteit", "version"], &ctx).expect("version");
        let flag_version = run_with_args(["noteit", "--version"], &ctx).expect("--version");
        let short_version = run_with_args(["noteit", "-V"], &ctx).expect("-V");

        assert_eq!(versao, version);
        assert_eq!(versao, flag_version);
        assert_eq!(versao, short_version);
    }

    #[test]
    fn dispatch_status_renders_status_report() {
        let ctx = OutputContext::plain();
        let status = run_with_args(["noteit", "status"], &ctx).expect("status");
        assert!(status.contains("CLI       pronta"));
        assert!(status.contains("Core      disponível"));
    }

    #[test]
    fn dispatch_invalid_command_returns_usage_error_in_portuguese() {
        let ctx = OutputContext::plain();
        let (exit_code, stderr) =
            run_with_args(["noteit", "batata"], &ctx).expect_err("should fail");
        assert_eq!(exit_code, EXIT_USAGE_ERROR);
        assert!(stderr.contains("Erro: comando desconhecido `batata`."));
        assert!(stderr.contains("Use `noteit ajuda` para ver os comandos disponíveis."));
    }

    #[test]
    fn dispatch_invalid_flag_returns_usage_error_in_portuguese() {
        let ctx = OutputContext::plain();
        let (exit_code, stderr) =
            run_with_args(["noteit", "--flag-desconhecida"], &ctx).expect_err("should fail");
        assert_eq!(exit_code, EXIT_USAGE_ERROR);
        assert!(stderr.contains("Erro: opção desconhecida `--flag-desconhecida`."));
        assert!(stderr.contains("Use `noteit ajuda` para ver os comandos e opções disponíveis."));
    }

    #[test]
    fn dispatch_unexpected_argument_returns_usage_error_in_portuguese() {
        let ctx = OutputContext::plain();
        let (exit_code, stderr) = run_with_args(["noteit", "status", "argumento-inesperado"], &ctx)
            .expect_err("should fail");
        assert_eq!(exit_code, EXIT_USAGE_ERROR);
        assert!(stderr.contains("Erro: argumento inesperado `argumento-inesperado`."));
        assert!(stderr.contains("Use `noteit ajuda` para ver o formato correto de uso."));
    }

    #[test]
    fn version_string_matches_workspace_cargo_pkg_version() {
        let ctx = OutputContext::plain();
        let version_out = run_with_args(["noteit", "versao"], &ctx).expect("success");
        assert_eq!(
            version_out,
            format!("Note-it {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}
