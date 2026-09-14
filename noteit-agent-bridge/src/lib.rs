//! Running an AI client on Note-it's behalf, and nothing else.
//!
//! ## What this is for
//!
//! Somebody wants to ask questions about their own notes without looking at a
//! terminal. The model does not live here: it lives in an AI command-line
//! client the person has already installed and already authenticated — Claude
//! Code, Gemini CLI, Codex. This crate starts one of those as a hidden
//! subprocess, feeds it a question, and turns whatever it streams back into
//! typed events a graphical interface can draw as a conversation.
//!
//! ## What this is deliberately not
//!
//! * **Not a model client.** There is no HTTP client here, no vendor SDK, no
//!   API key, no token storage and no billing. If the AI client the person
//!   chose talks to a remote service, it is that client doing it, with that
//!   person's own account.
//! * **Not a writer.** This crate does not depend on `noteit-core` at all, and
//!   that is the point: a component that cannot link the store cannot own it.
//!   Reading and writing notes happens through `noteit-mcp`, which the AI
//!   client is *told about* and which enforces `expected_revision` and the
//!   write authority exactly as the desktop does.
//! * **Not a terminal.** No pseudoterminal is allocated, no ANSI is parsed, no
//!   spinner is scraped and no interactive prompt is answered. An AI client
//!   without a documented non-interactive, structured mode is not adapted here
//!   — it is refused, by name, with the reason.
//!
//! ## Why lines of JSON
//!
//! Every adapter in this crate drives its client in a *documented* headless
//! mode that emits one JSON object per line. That is the whole contract: a
//! line arrives, an adapter turns it into an [`AgentEvent`], and a malformed
//! line is an event too ([`Failure::Protocol`]) rather than a panic or a
//! guess. Screen-scraping an interactive client would make Note-it's
//! behaviour depend on somebody else's spinner.

pub mod adapter;
pub mod detect;
pub mod event;
pub mod providers;
pub mod session;

pub use adapter::{Adapter, Focus, Request};
pub use detect::{available, Installed};
pub use event::{AgentEvent, Failure, ProviderId};
pub use session::Session;
