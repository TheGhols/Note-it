//! `noteit-embed`: the only component of Note-it with an HTTP client, and the
//! only one that ever sees a provider credential.
//!
//! ## Why a separate process at all
//!
//! Because the alternative is putting `rustls` and an API key into the process
//! that holds the note store and talks to an agent, and §9 of
//! `docs/semantic-retrieval.md` says the boundary is **extended, never
//! loosened**. Three things follow from running here instead:
//!
//! 1. `noteit-mcp`'s dependency graph keeps no HTTP or TLS crate, so
//!    `scripts/check-mcp-boundary` passes unchanged rather than being edited
//!    to permit something;
//! 2. the credential is not in the process that speaks to the agent — it is
//!    not even in that process's *environment*, because the spawner clears it;
//! 3. no tool acquires a general ability to make an HTTP request.
//!
//! ## What this process can and cannot do
//!
//! It can: read a framed request from a Unix socket, resolve a credential,
//! build a request for one of three pinned endpoints, speak HTTPS to it, apply
//! timeouts and a retry policy, validate the vectors that came back, and
//! answer with them or with one of Note-it's own error words.
//!
//! It cannot: open a note, list notes, know what a `NoteDocument` is, write
//! anything anywhere, answer an MCP tool, serve general HTTP, proxy to an
//! arbitrary URL, spawn a process, or accept a URL from its client.
//! `scripts/check-embed-boundary` fails the build for each of those
//! separately.
//!
//! ## The modules
//!
//! | | |
//! | --- | --- |
//! | [`endpoint`] | the three pinned hosts, and the reason this is not an SSRF |
//! | [`credential`] | where a key comes from, and the type that cannot print one |
//! | [`http`] | TLS, zero redirects, no proxy, three timeouts, a body ceiling, and the retry policy |
//! | [`provider`] | one adapter per vendor, and every vendor-specific fact inside one |
//! | [`socket`] | where the worker listens, and what must be true of that path |
//! | [`server`] | accept, read one frame, answer, close |

pub mod credential;
pub mod endpoint;
pub mod http;
pub mod provider;
pub mod server;
pub mod socket;

/// The name of the worker binary, in one place.
pub const WORKER_BINARY: &str = "noteit-embed";
