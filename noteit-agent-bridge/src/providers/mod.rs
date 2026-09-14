//! One module per AI client, each driving its own documented headless mode.
//!
//! Every flag here was read from the client's own `--help` on the machine this
//! was written for, and the versions are recorded so a future reader can tell
//! whether the contract has moved:
//!
//! | Client | Version audited | Headless mode | Structured output |
//! | --- | --- | --- | --- |
//! | Claude Code | 2.1.270 | `--print` | `--output-format stream-json` |
//! | Gemini CLI | 0.56.0 | `--prompt` | `--output-format stream-json` |
//! | Codex CLI | 0.154.0 | `codex exec` | `--json` |
//!
//! All three also take an MCP server configuration, which is how `noteit-mcp`
//! reaches them — and the reason this crate never needs to read a note itself.

mod claude;
mod codex;
mod fake;
mod gemini;

pub use claude::Claude;
pub use codex::Codex;
pub use fake::Fake;
pub use gemini::Gemini;

use crate::adapter::Adapter;
use crate::event::ProviderId;

/// The adapter for a provider.
pub fn adapter(provider: ProviderId) -> Box<dyn Adapter> {
    match provider {
        ProviderId::Claude => Box::new(Claude),
        ProviderId::Gemini => Box::new(Gemini),
        ProviderId::Codex => Box::new(Codex),
        ProviderId::Fake => Box::new(Fake),
    }
}

/// The JSON an adapter reads, shared because all three speak the same shape:
/// an object with a `type`, and whatever that type carries.
pub(crate) fn field<'a>(value: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    value.get(name).and_then(serde_json::Value::as_str)
}

/// Every piece of text in a message, whether the client spells it as a string
/// or as the content-block array the three of them have converged on.
pub(crate) fn text_of(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_owned());
    }
    if let Some(blocks) = value.as_array() {
        let joined: String = blocks
            .iter()
            .filter_map(|block| field(block, "text"))
            .collect::<Vec<_>>()
            .join("");
        return (!joined.is_empty()).then_some(joined);
    }
    None
}
