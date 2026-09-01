pub mod cli;
pub mod output;

use clap::error::ErrorKind;
use clap::Parser;
use cli::{CliArgs, CliCommand};
use noteit_core::{NoteFilter, NoteItCore, NoteSelectorError, StorePaths};
use output::OutputContext;

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

/// Executes the CLI with the provided argument list and output context.
///
/// Returns `Ok(stdout_string)` on success, or `Err((exit_code, stderr_string))` on failure.
pub fn run_with_args<I, T>(args: I, ctx: &OutputContext) -> Result<String, (u8, String)>
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
                let sanitized_query = output::sanitize_for_terminal(&consulta);
                let core = NoteItCore::open_read_only();
                match core.search_notes_filtered(&sanitized_query, &filter, limite) {
                    Ok(batch) => {
                        for w in &batch.warnings {
                            eprint!("{}", output::render_warning(ctx, w));
                        }
                        Ok(output::render_search_results(
                            ctx,
                            &sanitized_query,
                            &batch.items,
                        ))
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
            Some(CliCommand::Tags) => {
                let core = NoteItCore::open_read_only();
                let catalog = core.metadata_catalog();
                Ok(output::render_tags(ctx, &catalog))
            }
            Some(CliCommand::Propriedades) => {
                let core = NoteItCore::open_read_only();
                let catalog = core.metadata_catalog();
                Ok(output::render_properties(ctx, &catalog))
            }
            Some(CliCommand::Tarefas {
                estado,
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
            Some(CliCommand::Lixeira) => {
                let core = NoteItCore::open_read_only();
                let trash = core.list_trash();
                Ok(output::render_trash(ctx, &trash))
            }
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
