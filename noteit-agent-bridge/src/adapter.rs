//! The contract an AI client has to meet to be driven from here.

use crate::event::{AgentEvent, ProviderId};
use std::path::PathBuf;
use std::process::Command;

/// What the conversation is allowed to look at.
///
/// Honest naming (§15 of the phase): "Esta nota" is a *focus*, not a security
/// boundary. The AI client is told which note the person is looking at and is
/// asked to answer about it; nothing here can stop a client that has been
/// given the tools from reading another one. Calling that a sandbox would be
/// claiming an enforcement that does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Focus {
    /// One note is the priority context.
    ThisNote { id: String },
    /// The whole store, found by retrieval rather than by concatenation.
    AllNotes,
}

impl Focus {
    /// The sentence the model is given about what it is looking at.
    pub fn instruction(&self) -> String {
        match self {
            Self::ThisNote { id } => format!(
                "O foco desta conversa é a nota de id {id}. \
                 Use as ferramentas do Note-it para lê-la antes de responder."
            ),
            Self::AllNotes => "O foco desta conversa é o conjunto de notas. \
                 Use noteit_context e a busca para descobrir quais notas são \
                 relevantes antes de responder; não tente ler todas."
                .into(),
        }
    }
}

/// Everything a session needs to start.
#[derive(Debug, Clone)]
pub struct Request {
    /// What the person typed.
    pub prompt: String,
    /// What the conversation is about.
    pub focus: Focus,
    /// Where `noteit-mcp` is, when the client can speak MCP.
    ///
    /// An installed path, never `target/release`: the bridge is meant to work
    /// after a reboot, from any directory, without the repository.
    pub mcp_server: Option<PathBuf>,
    /// The environment `noteit-mcp` is given, which is how it finds the store.
    ///
    /// Plain variables rather than a `StorePaths`: this crate does not link
    /// `noteit-core`, so it cannot resolve a store even by accident.
    pub store_env: Vec<(String, String)>,
    /// Whether the client may be given tools that change notes.
    ///
    /// `true` in 5.1B, and not a suggestion: an adapter that cannot express
    /// "reads only" to its client must refuse rather than hope.
    pub read_only: bool,
}

impl Request {
    /// A read-only question about one note.
    pub fn about(prompt: impl Into<String>, focus: Focus) -> Self {
        Self {
            prompt: prompt.into(),
            focus,
            mcp_server: None,
            store_env: Vec::new(),
            read_only: true,
        }
    }

    /// The whole prompt the client is given: the question, plus what it is
    /// looking at.
    pub fn composed(&self) -> String {
        format!("{}\n\n{}", self.focus.instruction(), self.prompt)
    }
}

/// One AI client, driven in its own documented headless mode.
pub trait Adapter: Send + Sync {
    fn id(&self) -> ProviderId;

    /// The command to run, fully formed.
    ///
    /// It must be non-interactive and must emit one JSON object per line on
    /// standard output. An adapter that cannot arrange that does not exist:
    /// see the module documentation for why scraping is refused instead.
    fn command(&self, request: &Request) -> Command;

    /// One line of the client's output, as an event — or `None` for a line
    /// that carries nothing a conversation needs.
    ///
    /// Returning `None` rather than failing is what lets a client add fields
    /// and event kinds without breaking Note-it. A line that is not JSON at
    /// all is a [`crate::Failure::Protocol`], which the session raises.
    fn parse(&self, line: &str) -> Option<AgentEvent>;

    /// Whether a line is unreadable rather than merely uninteresting.
    fn is_unreadable(&self, line: &str) -> bool {
        !line.trim().is_empty() && serde_json::from_str::<serde_json::Value>(line).is_err()
    }
}
