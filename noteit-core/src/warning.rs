//! Structured, typed warnings for non-fatal read issues encountered by noteit-core.
//!
//! The Core returns warnings as structured data inside [`ReadBatch`]. Adapters
//! (CLI, JSON, MCP) decide how to present or route them without Core printing
//! to stdout/stderr.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Classification of non-fatal read anomalies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadWarningKind {
    UnreadableNote,
    CorruptedFrontMatter,
    SymlinkRefused,
    IoError,
}

/// A single non-fatal read warning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadWarning {
    pub note_id: Option<Uuid>,
    pub kind: ReadWarningKind,
    pub message: String,
}

/// A read batch returning successfully processed items alongside any non-fatal warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadBatch<T> {
    pub items: Vec<T>,
    pub warnings: Vec<ReadWarning>,
}

impl<T> ReadBatch<T> {
    pub fn new(items: Vec<T>, warnings: Vec<ReadWarning>) -> Self {
        Self { items, warnings }
    }

    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            warnings: Vec::new(),
        }
    }
}
