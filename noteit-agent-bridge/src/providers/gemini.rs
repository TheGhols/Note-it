//! Gemini CLI, in headless mode with a streaming JSON transcript.
//!
//! `gemini --prompt … --output-format stream-json` is the documented
//! non-interactive mode; `--approval-mode plan` is how this client is told to
//! read and not write.

use crate::adapter::{Adapter, Request};
use crate::event::{AgentEvent, ProviderId};
use crate::providers::{field, text_of};
use std::process::{Command, Stdio};

pub struct Gemini;

impl Adapter for Gemini {
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    fn command(&self, request: &Request) -> Command {
        let mut command = Command::new(ProviderId::Gemini.program());
        command.arg("--output-format").arg("stream-json");
        if request.read_only {
            command.arg("--approval-mode").arg("plan");
        }
        if let Some(server) = &request.mcp_server {
            // This client keeps its MCP servers in its own settings; naming
            // the one Note-it installs is how a conversation is limited to it.
            command
                .arg("--allowed-mcp-server-names")
                .arg(server.file_name().unwrap_or_default());
        }
        command
            .arg("--prompt")
            .arg(request.composed())
            .envs(request.store_env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn parse(&self, line: &str) -> Option<AgentEvent> {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        match field(&value, "type")? {
            "thought" => Some(AgentEvent::Thinking),
            "tool_call" | "tool_use" => Some(AgentEvent::ToolUse {
                name: field(&value, "name").unwrap_or("ferramenta").to_owned(),
            }),
            "content" | "assistant" => value
                .get("content")
                .or_else(|| value.get("text"))
                .and_then(text_of)
                .map(|text| AgentEvent::Delta { text }),
            "result" | "final" => Some(AgentEvent::Finished {
                text: value
                    .get("response")
                    .or_else(|| value.get("text"))
                    .and_then(text_of)
                    .unwrap_or_default(),
            }),
            _ => None,
        }
    }
}
