use noteit_core::StorePaths;
use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputContext {
    pub color_enabled: bool,
}

impl OutputContext {
    /// Detects whether stdout should receive ANSI styling based on TTY and NO_COLOR.
    pub fn for_stdout() -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let term_dumb = std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false);
        Self {
            color_enabled: is_tty && !no_color && !term_dumb,
        }
    }

    /// Explicitly creates a plain (non-ANSI) output context.
    pub fn plain() -> Self {
        Self {
            color_enabled: false,
        }
    }

    /// Explicitly creates a styled (ANSI enabled) output context.
    pub fn styled() -> Self {
        Self {
            color_enabled: true,
        }
    }

    pub fn bold(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[1m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn dim(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[2m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn green(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[32m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn yellow(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[33m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

pub fn render_welcome(ctx: &OutputContext) -> String {
    let title = ctx.bold("Note-it");
    let subtitle = ctx.dim("Suas notas, também pelo terminal.");
    let hint = format!("Use `{}` para começar.", ctx.bold("noteit ajuda"));

    format!(
        "{title}\n\n{subtitle}\n\n  ajuda      Ver comandos\n  status     Verificar a instalação\n  versao     Mostrar versão\n\n{hint}\n"
    )
}

pub fn render_help(ctx: &OutputContext) -> String {
    let title = ctx.bold("Note-it CLI");
    let section_usage = ctx.bold("Uso:");
    let section_commands = ctx.bold("Comandos:");
    let section_aliases = ctx.bold("Aliases:");

    format!(
        "{title}\n\n{section_usage}\n  noteit <comando> [opções]\n\n{section_commands}\n  ajuda       Mostrar esta ajuda\n  versao      Mostrar a versão\n  status      Verificar o ambiente do Note-it\n\n{section_aliases}\n  help        ajuda\n  version     versao\n"
    )
}

pub fn render_version(_ctx: &OutputContext) -> String {
    format!("Note-it {}\n", env!("CARGO_PKG_VERSION"))
}

pub fn render_status(ctx: &OutputContext, paths: &StorePaths) -> String {
    let version_line = ctx.bold(&format!("Note-it {}", env!("CARGO_PKG_VERSION")));
    let cli_status = ctx.green("pronta");
    let core_status = ctx.green("disponível");
    let store_status = if paths.store_exists() {
        ctx.green("encontrado")
    } else {
        ctx.yellow("ainda não criado")
    };

    let data_path = paths.data_dir.display();
    let config_path = paths.config_dir.display();
    let state_path = paths.state_dir.display();

    format!(
        "{version_line}\n\nCLI       {cli_status}\nCore      {core_status}\nStore     {store_status}\nDados     {data_path}\nConfig    {config_path}\nEstado    {state_path}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn plain_context_outputs_zero_ansi_sequences() {
        let ctx = OutputContext::plain();
        let paths = StorePaths::from_custom_paths(
            PathBuf::from("/mock/data/notes"),
            PathBuf::from("/mock/config"),
            PathBuf::from("/mock/state"),
            PathBuf::from("/mock/runtime"),
        );

        let welcome = render_welcome(&ctx);
        let help = render_help(&ctx);
        let version = render_version(&ctx);
        let status = render_status(&ctx, &paths);

        for output in [&welcome, &help, &version, &status] {
            assert!(
                !output.contains("\x1b["),
                "Plain output must not contain ANSI escape codes: {output:?}"
            );
        }
    }

    #[test]
    fn welcome_presentation_contains_guidance() {
        let ctx = OutputContext::plain();
        let welcome = render_welcome(&ctx);
        assert!(welcome.contains("Note-it"));
        assert!(welcome.contains("Suas notas, também pelo terminal."));
        assert!(welcome.contains("ajuda      Ver comandos"));
        assert!(welcome.contains("status     Verificar a instalação"));
        assert!(welcome.contains("versao     Mostrar versão"));
        assert!(welcome.contains("Use `noteit ajuda` para começar."));
    }

    #[test]
    fn help_presentation_contains_commands_and_aliases() {
        let ctx = OutputContext::plain();
        let help = render_help(&ctx);
        assert!(help.contains("Note-it CLI"));
        assert!(help.contains("noteit <comando> [opções]"));
        assert!(help.contains("ajuda       Mostrar esta ajuda"));
        assert!(help.contains("versao      Mostrar a versão"));
        assert!(help.contains("status      Verificar o ambiente do Note-it"));
        assert!(help.contains("help        ajuda"));
        assert!(help.contains("version     versao"));
    }

    #[test]
    fn version_presentation_matches_package_version() {
        let ctx = OutputContext::plain();
        let version = render_version(&ctx);
        assert_eq!(version, format!("Note-it {}\n", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn status_presentation_with_absent_store() {
        let ctx = OutputContext::plain();
        let paths = StorePaths::from_custom_paths(
            PathBuf::from("/nonexistent/data/notes"),
            PathBuf::from("/nonexistent/config"),
            PathBuf::from("/nonexistent/state"),
            PathBuf::from("/nonexistent/runtime"),
        );
        let status = render_status(&ctx, &paths);
        assert!(status.contains("CLI       pronta"));
        assert!(status.contains("Core      disponível"));
        assert!(status.contains("Store     ainda não criado"));
        assert!(status.contains("Dados     /nonexistent/data"));
        assert!(status.contains("Config    /nonexistent/config"));
        assert!(status.contains("Estado    /nonexistent/state"));
    }
}
