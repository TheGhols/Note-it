//! Context retrieval orchestration for the CLI.
//!
//! Bridges Note-it's Second Brain contextual retrieval with the CLI. Follows the
//! identical semantics and guarantees as the MCP server:
//! - Checks configured retrieval mode, provider and fallback policies
//! - If semantic channel does not apply or is disabled, runs lexical retrieval
//! - Supports local provider and remote workers over Unix domain socket
//! - Restores from and saves to the remote vector cache on disk
//! - Synchronizes the in-memory index against the live note store
//! - Handles automatic fallback and strict `semantic_required` policy

use noteit_core::chunking::CHUNKER_VERSION;
use noteit_core::context::{self as engine, ContextRequest, RetrievalMode, SemanticStatus};
use noteit_core::embedding::EmbeddingSpaceId;
use noteit_core::semantic::{
    synchronise, EmbeddingProvider, EmbeddingRecord, InMemoryIndex, SemanticFallback,
    SemanticIndex, SemanticRuntime,
};
use noteit_core::settings::{AppConfig, SemanticFallbackPolicy, SemanticRetrievalConfig};
use noteit_core::{NoteItCore, StorePaths, Uuid};
use noteit_embedding_local::LocalProvider;
use noteit_embedding_remote::worker::{WorkerHandle, WorkerPaths};
use noteit_embedding_remote::{cache, RemoteConfig, RemoteProvider, RemoteProviderId};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// What one retrieval got out of the contextual retrieval engine.
pub enum Retrieved {
    /// An answer, and what the semantic channel contributed to it.
    Answer(engine::ContextResult, SemanticStatus),
    /// The request itself was invalid or could not be served.
    Refused(engine::ContextError),
    /// The caller asked for the semantic channel and it was unavailable.
    SemanticRequired,
}

/// Retrieves context for a CLI command according to the store's configuration.
pub fn retrieve_context(
    core: &NoteItCore,
    paths: &StorePaths,
    request: &ContextRequest,
) -> Retrieved {
    let settings = AppConfig::read_only(&paths.config_file_path()).semantic_retrieval;

    if !settings.semantic_is_enabled() || !engine::semantic_channel_applies(request) {
        return match engine::retrieve(core, request) {
            Ok(answer) => Retrieved::Answer(answer, SemanticStatus::NotRequested),
            Err(error) => Retrieved::Refused(error),
        };
    }

    match RemoteProviderId::parse(settings.provider.as_str()) {
        Some(provider_id) => {
            let (dimension, dimension_requested) = settings.resolved_dimension();
            let remote_config = RemoteConfig {
                provider: provider_id,
                model: settings.resolved_model(),
                dimension,
                dimension_requested,
            };
            let worker = Arc::new(WorkerHandle::new(WorkerPaths::resolve(
                paths.config_dir.clone(),
                &paths.runtime_dir,
            )));
            let provider = RemoteProvider::with_worker(remote_config, worker);
            execute_with_provider(core, &provider, request, &settings, true)
        }
        None => match LocalProvider::load_default() {
            Ok(provider) => execute_with_provider(core, &provider, request, &settings, false),
            Err(_) => degraded(core, request, &settings),
        },
    }
}

fn degraded(
    core: &NoteItCore,
    request: &ContextRequest,
    settings: &SemanticRetrievalConfig,
) -> Retrieved {
    if settings.fallback == SemanticFallbackPolicy::SemanticRequired {
        return Retrieved::SemanticRequired;
    }
    match engine::retrieve(core, request) {
        Ok(answer) => Retrieved::Answer(answer, SemanticStatus::Unavailable),
        Err(error) => Retrieved::Refused(error),
    }
}

fn execute_with_provider(
    core: &NoteItCore,
    provider: &dyn EmbeddingProvider,
    request: &ContextRequest,
    settings: &SemanticRetrievalConfig,
    is_remote: bool,
) -> Retrieved {
    let fallback = match settings.fallback {
        SemanticFallbackPolicy::SemanticRequired => SemanticFallback::Required,
        _ => SemanticFallback::Automatic,
    };

    let space = provider.space();
    let mut index = InMemoryIndex::new(space.clone());
    let cache_root = if is_remote {
        cache::default_root()
    } else {
        None
    };

    if let Some(root) = &cache_root {
        restore_from_cache(root, &space, &mut index);
    }

    let synced = match synchronise(core, provider, &mut index) {
        Ok(s) => s,
        Err(err) => return Retrieved::Refused(err),
    };
    let mut cache_dirty = synced.changed();

    let before = SemanticIndex::vector_count(&index);
    let runtime = SemanticRuntime::new(provider, &mut index).with_fallback(fallback);

    let outcome = match engine::retrieve_with(core, request, RetrievalMode::Semantic(runtime)) {
        Ok(outcome) => outcome,
        Err(engine::RetrievalError::Context(err)) => return Retrieved::Refused(err),
        Err(engine::RetrievalError::Semantic(_)) => {
            return if settings.fallback == SemanticFallbackPolicy::SemanticRequired {
                Retrieved::SemanticRequired
            } else {
                match engine::retrieve(core, request) {
                    Ok(answer) => Retrieved::Answer(answer, SemanticStatus::Unavailable),
                    Err(err) => Retrieved::Refused(err),
                }
            };
        }
    };

    let after = SemanticIndex::vector_count(&index);
    if after < before {
        // A note was edited and forgotten during retrieval, run one retry pass
        let synced2 = match synchronise(core, provider, &mut index) {
            Ok(s) => s,
            Err(err) => return Retrieved::Refused(err),
        };
        if synced2.changed() {
            cache_dirty = true;
        }
        let runtime2 = SemanticRuntime::new(provider, &mut index).with_fallback(fallback);
        match engine::retrieve_with(core, request, RetrievalMode::Semantic(runtime2)) {
            Ok(outcome2) => {
                if is_remote && cache_dirty {
                    if let Some(root) = &cache_root {
                        persist(root, &space, &index);
                    }
                }
                return Retrieved::Answer(outcome2.result, outcome2.semantic_status);
            }
            Err(engine::RetrievalError::Context(err)) => return Retrieved::Refused(err),
            Err(engine::RetrievalError::Semantic(_)) => {
                return if settings.fallback == SemanticFallbackPolicy::SemanticRequired {
                    Retrieved::SemanticRequired
                } else {
                    match engine::retrieve(core, request) {
                        Ok(answer) => Retrieved::Answer(answer, SemanticStatus::Unavailable),
                        Err(err) => Retrieved::Refused(err),
                    }
                };
            }
        }
    }

    if is_remote && cache_dirty {
        if let Some(root) = &cache_root {
            persist(root, &space, &index);
        }
    }

    Retrieved::Answer(outcome.result, outcome.semantic_status)
}

fn restore_from_cache(root: &Path, space: &EmbeddingSpaceId, index: &mut InMemoryIndex) {
    let Ok(records) = cache::load(root, space, CHUNKER_VERSION) else {
        return;
    };
    let mut by_note: BTreeMap<Uuid, Vec<EmbeddingRecord>> = BTreeMap::new();
    for record in records {
        by_note.entry(record.note_id).or_default().push(record);
    }
    for (note_id, records) in by_note {
        let _ = index.replace_note(&note_id, records);
    }
}

fn persist(root: &Path, space: &EmbeddingSpaceId, index: &InMemoryIndex) -> bool {
    let mut records = Vec::with_capacity(SemanticIndex::vector_count(index));
    for note_id in index.note_ids() {
        records.extend(index.records_for(&note_id).iter().cloned());
    }
    if cache::save(root, space, CHUNKER_VERSION, &records).is_err() {
        return false;
    }
    cache::prune(root, space);
    true
}
