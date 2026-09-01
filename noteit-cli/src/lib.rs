pub mod cli;
pub mod output;

use clap::error::ErrorKind;
use clap::Parser;
use cli::{CliArgs, CliCommand};
use noteit_core::StorePaths;
use output::OutputContext;

pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_EXECUTION_ERROR: u8 = 1;
pub const EXIT_USAGE_ERROR: u8 = 2;

/// Executes the CLI with the provided argument list and output context.
///
/// Returns `Ok(stdout_string)` on success, or `Err((exit_code, stderr_string))` on failure.
pub fn run_with_args<I, T>(args: I, ctx: &OutputContext) -> Result<String, (u8, String)>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match CliArgs::try_parse_from(args) {
        Ok(parsed) => match parsed.command {
            None => Ok(output::render_welcome(ctx)),
            Some(CliCommand::Ajuda) => Ok(output::render_help(ctx)),
            Some(CliCommand::Versao) => Ok(output::render_version(ctx)),
            Some(CliCommand::Status) => {
                let paths = StorePaths::resolve();
                Ok(output::render_status(ctx, &paths))
            }
        },
        Err(err) => match err.kind() {
            ErrorKind::DisplayHelp => Ok(output::render_help(ctx)),
            ErrorKind::DisplayVersion => Ok(output::render_version(ctx)),
            _ => Err((EXIT_USAGE_ERROR, err.to_string())),
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
    fn dispatch_invalid_command_returns_usage_error() {
        let ctx = OutputContext::plain();
        let (exit_code, stderr) =
            run_with_args(["noteit", "nonexistent"], &ctx).expect_err("should fail");
        assert_eq!(exit_code, EXIT_USAGE_ERROR);
        assert!(!stderr.is_empty());
    }
}
