//! A client a test supplies, driven by the same contract as a real one.
//!
//! This exists so that the whole path — start a process, stream its lines,
//! turn them into events, cancel it, reap it — can be tested without a model,
//! without a network and without an account. §32 of the phase requires exactly
//! that: CI must never depend on somebody's API quota.
//!
//! The vocabulary is the neutral one: `{"type":"thinking"}`,
//! `{"type":"delta","text":"…"}`, `{"type":"tool","name":"…"}`,
//! `{"type":"message","text":"…"}`, `{"type":"done","text":"…"}`.

use crate::adapter::{Adapter, Request};
use crate::event::{AgentEvent, ProviderId};
use crate::providers::field;
use std::process::{Command, Stdio};

pub struct Fake;

impl Adapter for Fake {
    fn id(&self) -> ProviderId {
        ProviderId::Fake
    }

    fn command(&self, request: &Request) -> Command {
        let mut command = Command::new(ProviderId::Fake.program());
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
        match field(&value, "type")? {
            "thinking" => Some(AgentEvent::Thinking),
            "delta" => Some(AgentEvent::Delta {
                text: field(&value, "text")?.to_owned(),
            }),
            "tool" => Some(AgentEvent::ToolUse {
                name: field(&value, "name")?.to_owned(),
            }),
            "message" => Some(AgentEvent::Message {
                text: field(&value, "text")?.to_owned(),
            }),
            "done" => Some(AgentEvent::Finished {
                text: field(&value, "text").unwrap_or_default().to_owned(),
            }),
            _ => None,
        }
    }
}
