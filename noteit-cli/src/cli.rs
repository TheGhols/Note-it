use clap::{Parser, Subcommand};

#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[command(
    name = "noteit",
    version,
    about = "Note-it — Linha de comando",
    long_about = "Note-it CLI\n\nInterface de linha de comando headless para o Note-it.",
    disable_help_subcommand = true
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    /// Mostrar ajuda sobre os comandos
    #[command(name = "ajuda", alias = "help")]
    Ajuda,

    /// Mostrar a versão do Note-it
    #[command(name = "versao", alias = "version")]
    Versao,

    /// Verificar o ambiente e diretórios do Note-it
    #[command(name = "status")]
    Status,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_primary_commands_and_aliases() {
        let ajuda = CliArgs::try_parse_from(["noteit", "ajuda"]).expect("ajuda");
        assert_eq!(ajuda.command, Some(CliCommand::Ajuda));

        let help = CliArgs::try_parse_from(["noteit", "help"]).expect("help alias");
        assert_eq!(help.command, Some(CliCommand::Ajuda));

        let versao = CliArgs::try_parse_from(["noteit", "versao"]).expect("versao");
        assert_eq!(versao.command, Some(CliCommand::Versao));

        let version = CliArgs::try_parse_from(["noteit", "version"]).expect("version alias");
        assert_eq!(version.command, Some(CliCommand::Versao));

        let status = CliArgs::try_parse_from(["noteit", "status"]).expect("status");
        assert_eq!(status.command, Some(CliCommand::Status));
    }

    #[test]
    fn parse_no_command_returns_none() {
        let empty = CliArgs::try_parse_from(["noteit"]).expect("no subcommand");
        assert_eq!(empty.command, None);
    }
}
