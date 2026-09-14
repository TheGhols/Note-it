//! What a conversation looks like from the outside.

use serde::{Deserialize, Serialize};

/// Which AI client is behind a session.
///
/// A closed list. A provider without an adapter is not a `ProviderId` that
/// exists, so an interface cannot offer one that does not work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderId {
    /// Claude Code: `claude --print --output-format stream-json`.
    Claude,
    /// Gemini CLI: `gemini --prompt --output-format stream-json`.
    Gemini,
    /// Codex CLI: `codex exec --json`.
    Codex,
    /// A client supplied by a test, driven by the same JSONL contract.
    Fake,
}

impl ProviderId {
    /// The name a person sees.
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Gemini => "Gemini CLI",
            Self::Codex => "Codex CLI",
            Self::Fake => "Cliente de teste",
        }
    }

    /// The command this provider is, as `PATH` spells it.
    pub fn program(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Codex => "codex",
            Self::Fake => "noteit-fake-agent",
        }
    }
}

/// Why a session could not go on.
///
/// Typed, because the interface has to say something a person can act on. A
/// stack trace is diagnostics, not an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Failure {
    /// The chosen client is not installed.
    ClientNotFound { program: String },
    /// It is installed and would not start.
    StartFailed { reason: String },
    /// It started and said something this adapter cannot read.
    Protocol { line: String },
    /// It ended without finishing the answer.
    Exited { code: Option<i32> },
    /// It was still going when the caller stopped waiting.
    Cancelled,
}

impl Failure {
    /// One sentence, for a person rather than for a log.
    pub fn message(&self) -> String {
        match self {
            Self::ClientNotFound { program } => {
                format!("Cliente de IA não encontrado: `{program}` não está instalado.")
            }
            Self::StartFailed { reason } => {
                format!("Falha ao iniciar o cliente de IA: {reason}")
            }
            Self::Protocol { .. } => {
                "O cliente de IA respondeu em um formato que o Note-it não entende.".into()
            }
            Self::Exited { code } => match code {
                Some(code) => format!("O cliente de IA encerrou com código {code}."),
                None => "O cliente de IA encerrou sem responder.".into(),
            },
            Self::Cancelled => "Geração cancelada.".into(),
        }
    }
}

/// One thing that happened in a conversation.
///
/// Ordered as they arrive. A session always ends with exactly one of
/// [`AgentEvent::Finished`], [`AgentEvent::Failed`] or
/// [`AgentEvent::Cancelled`], so an interface never has to guess whether more
/// is coming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentEvent {
    /// The client started and accepted the question.
    Started { provider: ProviderId },
    /// It is working. Drawn as "Pensando", not as a spinner somebody parsed.
    Thinking,
    /// A piece of the answer, as it is produced.
    Delta { text: String },
    /// A whole assistant message.
    Message { text: String },
    /// The client used one of its tools — an `noteit-mcp` call, usually.
    ///
    /// Surfaced so a person can see *that* their notes were read, without the
    /// interface having to show them JSON.
    ToolUse { name: String },
    /// The answer is complete.
    Finished { text: String },
    /// It is not, and this is why.
    Failed { failure: Failure },
    /// Somebody stopped it.
    Cancelled,
}

impl AgentEvent {
    /// Whether nothing more will arrive after this one.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Finished { .. } | Self::Failed { .. } | Self::Cancelled
        )
    }
}
