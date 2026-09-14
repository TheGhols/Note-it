//! Claude Code, in print mode with a streaming JSON transcript.
//!
//! `claude --print --output-format stream-json --verbose` writes one JSON
//! object per line: a `system`/`init` line when the session starts, an
//! `assistant` line per message, and a `result` line at the end. That is a
//! documented interface and it is why this adapter exists; the interactive
//! mode is a terminal application and is not parsed here.

use crate::adapter::{Adapter, Request};
use crate::event::{AgentEvent, ProviderId};
use crate::providers::{field, text_of};
use std::process::{Command, Stdio};

pub struct Claude;

impl Adapter for Claude {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn command(&self, request: &Request) -> Command {
        let mut command = Command::new(ProviderId::Claude.program());
        command
            .arg("--print")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose");

        // Reads only, in 5.1B. The permission mode is how this client is told
        // so; an adapter that could not say it would have to refuse.
        if request.read_only {
            command.arg("--permission-mode").arg("plan");
        }
        if let Some(server) = &request.mcp_server {
            command.arg("--mcp-config").arg(mcp_config(server));
        }
        command
            .arg("--")
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
            "system" => Some(AgentEvent::Thinking),
            "assistant" => {
                let content = value.get("message")?.get("content")?;
                if let Some(name) = tool_name(content) {
                    return Some(AgentEvent::ToolUse { name });
                }
                text_of(content).map(|text| AgentEvent::Message { text })
            }
            "result" => Some(AgentEvent::Finished {
                text: field(&value, "result").unwrap_or_default().to_owned(),
            }),
            _ => None,
        }
    }
}

/// The name of the tool a content block is using, if it is using one.
fn tool_name(content: &serde_json::Value) -> Option<String> {
    content.as_array()?.iter().find_map(|block| {
        (field(block, "type") == Some("tool_use"))
            .then(|| field(block, "name").unwrap_or("ferramenta").to_owned())
    })
}

/// The MCP configuration this client takes, as the JSON it expects.
///
/// The server is started by the client, with the environment this crate was
/// given — which is how a conversation reaches the store without this crate
/// ever touching it.
fn mcp_config(server: &std::path::Path) -> String {
    serde_json::json!({
        "mcpServers": {
            "note-it": { "command": server.to_string_lossy(), "args": [] }
        }
    })
    .to_string()
}
