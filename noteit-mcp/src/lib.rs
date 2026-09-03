//! A local, headless Model Context Protocol server over the Note-it domain.
//!
//! ```text
//! MCP host
//!    │  spawns, and owns the process's whole lifetime
//!    ▼
//! noteit-mcp                      ← this crate, stdio only
//!    │  typed adapters
//!    ▼
//! noteit_core::authority          ← the one writer decision
//!    │
//!    ├─ the lease is free  → this process writes, through the Core
//!    └─ the lease is held  → the instance holding it is asked, over the
//!                            private control socket
//!    ▼
//! the store
//! ```
//!
//! ## What this is not
//!
//! **Not a daemon.** It speaks on standard input and standard output and lives
//! exactly as long as the host keeps it. It opens no port, binds no listener,
//! serves no HTTP and writes no configuration anywhere.
//!
//! **Not a filesystem.** Every tool is one operation Note-it already performs.
//! There is no tool that takes a path, reads a file, writes a file, lists a
//! directory or runs a command — see [`server`].
//!
//! **Not a second Note-it.** It writes nothing itself. A `.md` file is never
//! opened here, the `noteit` binary is never spawned here, and no output of it
//! is ever parsed here. Everything goes through the same typed operations the
//! command line and the desktop application use.
//!
//! ## The property the whole crate exists for
//!
//! An agent is a programmatic writer, and a programmatic writer that overwrites
//! whatever it did not read is how somebody's paragraph disappears. So every
//! tool that changes an existing note **requires** the revision the change was
//! decided from. Not "accepts"; requires. See [`contract`].

pub mod contract;
pub mod domain;
pub mod schema;
pub mod server;

pub use domain::Store;
pub use server::NoteItMcpServer;
