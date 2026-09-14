//! Codex CLI, through its non-interactive `exec` subcommand.
//!
//! `codex exec --json` runs a prompt without a terminal and writes a JSON
//! event per line. The approval policy is how this client is told not to run
//! anything on its own.

use crate::adapter::{Adapter, Request};
use crate::event::{AgentEvent, ProviderId};
use crate::providers::{field, text_of};
use std::process::{Command, Stdio};

pub struct Codex;

impl Adapter for Codex {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn command(&self, request: &Request) -> Command {
        let mut command = Command::new(ProviderId::Codex.program());
        command.arg("exec").arg("--json");
        if request.read_only {
            command.arg("--sandbox").arg("read-only");
        }
        command
            .arg(request.composed())
            .envs(request.store_env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn parse(&self, line: &str) -> Option<AgentEvent> {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        // This client nests the interesting part under `msg`, with the kind in
        // `type` either way.
        let body = value.get("msg").unwrap_or(&value);
        match field(body, "type")? {
            "task_started" | "agent_reasoning" => Some(AgentEvent::Thinking),
            "mcp_tool_call_begin" | "tool_call" => Some(AgentEvent::ToolUse {
                name: field(body, "tool").unwrap_or("ferramenta").to_owned(),
            }),
            "agent_message_delta" => body
                .get("delta")
                .and_then(text_of)
                .map(|text| AgentEvent::Delta { text }),
            "agent_message" => body
                .get("message")
                .and_then(text_of)
                .map(|text| AgentEvent::Message { text }),
            "task_complete" => Some(AgentEvent::Finished {
                text: body
                    .get("last_agent_message")
                    .and_then(text_of)
                    .unwrap_or_default(),
            }),
            _ => None,
        }
    }
}
