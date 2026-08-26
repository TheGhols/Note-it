use clap::{Parser, Subcommand};

#[derive(Parser, Debug, Clone)]
#[command(
    name = "note-it",
    author = "Note-it contributors",
    version = "0.1.0",
    about = "Minimalist sticky notes for Linux Wayland",
    long_about = "Note-it is a local-first, distraction-free desktop note application built with Wayland Layer Shell."
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
    /// Bring all notes into overlay mode
    Show,
    /// Hide all notes
    Hide,
    /// Save all notes and terminate the Note-it daemon
    Quit,
}
