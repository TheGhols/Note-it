//! Shared, headless domain and persistence boundary for Note-it.
//!
//! This crate deliberately has no GTK, GDK, WebKitGTK, layer-shell or
//! compositor dependency. The desktop application is one adapter over this
//! API; future programmatic adapters must use the same operations instead of
//! recreating storage or domain rules.

pub mod assets;
pub mod autopaste;
pub mod backup;
pub mod diagnostics;
pub mod model;
pub mod search;
pub mod settings;
pub mod state;
pub mod storage;
pub mod study;
pub mod timer;
pub mod trash;

mod atomic_file;
mod visible_text;

use model::NoteDocument;
use search::SearchResult;
use storage::StorageManager;
use study::StudyState;
use trash::TrashEntry;
use uuid::Uuid;

/// The shared application boundary over Note-it's existing store.
///
/// It is intentionally a small facade: it gives adapters stable domain
/// operations while keeping `StorageManager` available for existing write and
/// lifecycle coordination. Every method delegates to the one established
/// implementation; this type owns no alternative persistence path.
#[derive(Debug, Clone)]
pub struct NoteItCore {
    storage: StorageManager,
}

impl NoteItCore {
    /// Opens the XDG-backed Note-it store.
    pub fn new() -> Result<Self, String> {
        StorageManager::new().map(Self::from_storage)
    }

    /// Wraps an already configured store, including a synthetic test store.
    pub fn from_storage(storage: StorageManager) -> Self {
        Self { storage }
    }

    /// The shared persistence implementation used by existing write flows.
    pub fn storage(&self) -> &StorageManager {
        &self.storage
    }

    /// Lists live note identifiers in the canonical recency order.
    pub fn list_notes(&self) -> Result<Vec<Uuid>, String> {
        self.storage.list_notes_by_recency()
    }

    /// Reads and parses one live note through the canonical storage path.
    pub fn read_note(&self, id: &Uuid) -> Result<NoteDocument, String> {
        self.storage.load_note(id)
    }

    /// Searches every live note, or lists recent notes for an empty query.
    pub fn search_notes(&self, query: &str) -> Vec<SearchResult> {
        let listing = query.trim().is_empty();
        let bodies = if listing {
            self.storage.read_recent_note_bodies(search::MAX_RESULTS)
        } else {
            self.storage.read_note_bodies_by_recency()
        };
        let notes = bodies.iter().map(|(id, body)| (*id, body.as_str()));
        if listing {
            search::recent_notes(notes)
        } else {
            search::search_notes(query, notes)
        }
    }

    /// Lists recoverable deleted notes without opening or mutating them.
    pub fn list_trash(&self) -> Vec<TrashEntry> {
        self.storage.list_trash()
    }

    /// Loads the persisted study schedule without a WebView or editor.
    pub fn study_state(&self) -> Result<StudyState, String> {
        self.storage.load_study()
    }
}
