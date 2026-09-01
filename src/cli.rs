use clap::{Parser, Subcommand};

#[derive(Parser, Debug, Clone)]
#[command(
    name = "note-it",
    author,
    version,
    about = "Minimalist sticky notes for Linux Wayland",
    long_about = "Note-it is a local-first, distraction-free desktop note application built with Wayland Layer Shell.\n\nRun without a subcommand to summon Note-it: the notes are restored and brought to the front, reusing the instance already running. If every note has been closed, the one used last comes back rather than a blank note."
)]
pub struct CliArgs {
    /// Start the application in background daemon mode without creating a note window
    #[arg(long, default_value_t = false)]
    pub background: bool,

    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    /// Create a new sticky note immediately
    New,
    /// Toggle notes between desktop background and overlay mode
    Toggle,
    /// Bring all notes into overlay mode and keep that as the preference
    Show,
    /// Hide all notes
    Hide,
    /// Collapse every note to its header bar, or expand them all when they
    /// are already collapsed
    ToggleCollapseAll,
    /// Save all notes and terminate the Note-it daemon
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_lifecycle_commands_keep_their_public_spelling() {
        for (name, expected) in [
            ("new", CliCommand::New),
            ("toggle", CliCommand::Toggle),
            ("show", CliCommand::Show),
            ("hide", CliCommand::Hide),
            ("toggle-collapse-all", CliCommand::ToggleCollapseAll),
            ("quit", CliCommand::Quit),
        ] {
            let parsed = CliArgs::try_parse_from(["note-it", name]).expect("existing command");
            assert_eq!(parsed.command, Some(expected));
            assert!(!parsed.background);
        }
    }

    #[test]
    fn background_and_commandless_summon_remain_accepted() {
        let background =
            CliArgs::try_parse_from(["note-it", "--background"]).expect("background mode");
        assert!(background.background);
        assert_eq!(background.command, None);

        let summon = CliArgs::try_parse_from(["note-it"]).expect("commandless summon");
        assert!(!summon.background);
        assert_eq!(summon.command, None);
    }
}
