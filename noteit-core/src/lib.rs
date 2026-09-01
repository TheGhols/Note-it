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
pub mod filter;
pub mod metadata;
pub mod model;
pub mod search;
pub mod settings;
pub mod state;
pub mod storage;
pub mod study;
pub mod task;
pub mod timer;
pub mod trash;

mod atomic_file;
mod visible_text;

pub use chrono;
pub use filter::{NoteFilter, NoteSelectorError};
pub use metadata::{
    MetadataCatalog, NoteMetadata, NoteProperties, NoteProperty, NoteTags, PropertyKeyCatalogEntry,
    TagCatalogEntry,
};
pub use model::{NoteDocument, NoteFrontMatter, NoteSummary};
pub use search::SearchResult;
pub use storage::{StorageManager, StorePaths};
pub use study::StudyState;
pub use task::{TaskEntry, TaskStateFilter};
pub use trash::TrashEntry;
pub use uuid::Uuid;

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
    /// Purely resolves canonical Note-it paths without performing filesystem I/O or directory creation.
    pub fn resolve_paths() -> StorePaths {
        StorePaths::resolve()
    }

    /// Opens the XDG-backed Note-it store with directory creation enabled (for desktop/write flows).
    pub fn new() -> Result<Self, String> {
        StorageManager::new().map(Self::from_storage)
    }

    /// Opens the XDG-backed Note-it store in strictly read-only mode.
    /// Does NOT create missing directories, state files, or backups.
    pub fn open_read_only() -> Self {
        Self::from_storage(StorageManager::open_read_only(StorePaths::resolve()))
    }

    /// Opens the store at custom paths in strictly read-only mode.
    pub fn open_read_only_at(paths: StorePaths) -> Self {
        Self::from_storage(StorageManager::open_read_only(paths))
    }

    /// Wraps an already configured store, including a synthetic test store.
    pub fn from_storage(storage: StorageManager) -> Self {
        Self { storage }
    }

    /// The shared persistence implementation used by existing write flows.
    pub fn storage(&self) -> &StorageManager {
        &self.storage
    }

    /// Access the resolved storage paths for this store.
    pub fn paths(&self) -> &StorePaths {
        self.storage.paths()
    }

    /// Resolves a human-provided note selector (UUID or >= 8 hex prefix) to a unique live note UUID.
    pub fn resolve_note_id(&self, selector: &str) -> Result<Uuid, NoteSelectorError> {
        let trimmed = selector.trim();
        if trimmed.is_empty()
            || trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains("..")
        {
            return Err(NoteSelectorError::InvalidFormat(trimmed.to_string()));
        }

        // Must consist only of hex characters and optional hyphens
        if !trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err(NoteSelectorError::InvalidFormat(trimmed.to_string()));
        }

        let hex_only: String = trimmed
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .map(|c| c.to_ascii_lowercase())
            .collect();

        if hex_only.len() < 8 {
            return Err(NoteSelectorError::InvalidFormat(trimmed.to_string()));
        }

        // If it parses as a full UUID directly, check if it exists
        if let Ok(full_uuid) = Uuid::parse_str(trimmed) {
            if self.storage.note_exists(&full_uuid) {
                return Ok(full_uuid);
            }
        }

        let live_ids = self
            .storage
            .list_notes_by_recency()
            .map_err(NoteSelectorError::StoreUnavailable)?;

        let mut matches = Vec::new();
        for id in live_ids {
            let id_simple = id.as_simple().to_string();
            if id_simple.starts_with(&hex_only) {
                matches.push(id);
            }
        }

        match matches.len() {
            0 => Err(NoteSelectorError::NotFound(trimmed.to_string())),
            1 => Ok(matches[0]),
            _ => Err(NoteSelectorError::Ambiguous(trimmed.to_string(), matches)),
        }
    }

    /// Lists live note identifiers in the canonical recency order.
    pub fn list_notes(&self) -> Result<Vec<Uuid>, String> {
        self.storage.list_notes_by_recency()
    }

    /// Lists note summaries in recency order matching an optional filter and limit.
    pub fn list_summaries(
        &self,
        filter: &NoteFilter,
        limit: Option<usize>,
    ) -> Result<Vec<NoteSummary>, String> {
        let ids = self.storage.list_notes_by_recency()?;
        let max = limit.unwrap_or(20).clamp(1, 100);
        let mut summaries = Vec::new();

        for id in ids {
            if summaries.len() >= max {
                break;
            }
            match self.storage.load_note(&id) {
                Ok(doc) => {
                    if filter.matches(&doc.user_metadata) {
                        summaries.push(NoteSummary::from_document(&doc));
                    }
                }
                Err(err) => {
                    eprintln!("Aviso: nota {id} ignorada por erro de leitura: {err}");
                }
            }
        }

        Ok(summaries)
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

    /// Searches live notes with tag and property filtering applied.
    pub fn search_notes_filtered(
        &self,
        query: &str,
        filter: &NoteFilter,
        limit: Option<usize>,
    ) -> Result<Vec<SearchResult>, String> {
        let max = limit.unwrap_or(20).clamp(1, 100);

        if filter.is_empty() {
            let results = self.search_notes(query);
            return Ok(results.into_iter().take(max).collect());
        }

        let ids = self.storage.list_notes_by_recency()?;
        let mut matching_bodies = Vec::new();

        for id in ids {
            match self.storage.load_note(&id) {
                Ok(doc) => {
                    if filter.matches(&doc.user_metadata) {
                        matching_bodies.push((id, doc.content));
                    }
                }
                Err(err) => {
                    eprintln!("Aviso: nota {id} ignorada por erro de leitura: {err}");
                }
            }
        }

        let borrowed: Vec<(Uuid, &str)> = matching_bodies
            .iter()
            .map(|(id, text)| (*id, text.as_str()))
            .collect();

        let results = if query.trim().is_empty() {
            search::recent_notes(borrowed)
        } else {
            search::search_notes(query, borrowed)
        };

        Ok(results.into_iter().take(max).collect())
    }

    /// Lists tasks matching the state filter, metadata filter, and limit.
    pub fn list_tasks(
        &self,
        state: TaskStateFilter,
        filter: &NoteFilter,
        limit: Option<usize>,
    ) -> Result<Vec<TaskEntry>, String> {
        let ids = self.storage.list_notes_by_recency()?;
        let max = limit.unwrap_or(20).clamp(1, 100);
        let mut results = Vec::new();

        for id in ids {
            if results.len() >= max {
                break;
            }
            match self.storage.load_note(&id) {
                Ok(doc) => {
                    if filter.matches(&doc.user_metadata) {
                        let label = search::label_for(&doc.content);
                        let tasks = task::parse_tasks(id, &label, &doc.content);
                        for t in tasks {
                            if state.matches(t.checked) {
                                results.push(t);
                                if results.len() >= max {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    eprintln!("Aviso: nota {id} ignorada por erro de leitura: {err}");
                }
            }
        }

        Ok(results)
    }

    /// Lists recoverable deleted notes without opening or mutating them.
    pub fn list_trash(&self) -> Vec<TrashEntry> {
        self.storage.list_trash()
    }

    /// Derives tag and property-key suggestions from live notes only.
    pub fn metadata_catalog(&self) -> MetadataCatalog {
        self.storage.metadata_catalog()
    }

    /// Loads the persisted study schedule without a WebView or editor.
    pub fn study_state(&self) -> Result<StudyState, String> {
        self.storage.load_study()
    }
}
