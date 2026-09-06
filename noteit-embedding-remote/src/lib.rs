//! The remote embedding provider: an AF_UNIX client, a worker's lifecycle, and
//! a derived vector cache.
//!
//! ## Where this crate sits, and what it deliberately is not
//!
//! ```text
//! noteit-mcp ─────► noteit-core ─────► EmbeddingProvider (trait)
//!   no HTTP           no HTTP              │
//!                                          ├── noteit-embedding-local
//!                                          │     in process, no socket at all
//!                                          │
//!                                          └── noteit-embedding-remote  ← here
//!                                                   │  AF_UNIX, no HTTP
//!                                                   ▼
//!                                            noteit-embed
//!                                                   │  the only HTTP client
//!                                                   ▼
//!                                            the provider's API
//! ```
//!
//! **This crate has no HTTP client and no TLS.** It is a client of the
//! *worker*, not of a vendor: it opens a Unix socket and writes a frame. That
//! is the whole reason the process split is worth its cost, and
//! `scripts/check-embed-boundary` fails the build if a network stack appears
//! in this graph.
//!
//! It also never sees a credential. [`worker::spawn`] builds the child's
//! environment from nothing and puts back a list that contains no key, and the
//! worker resolves its own from a mode-`0600` file. Nothing in this crate reads
//! one, has a field for one, or could print one.
//!
//! ## The three parts
//!
//! | | |
//! | --- | --- |
//! | [`space`] | the recipe, and how honest a remote identity is |
//! | [`worker`] | who launches `noteit-embed`, and what it is allowed to inherit |
//! | [`client`] | one frame out, one frame in, one connection |
//! | [`cache`] | the versioned, atomic, owner-only index on disk |

pub mod cache;
pub mod client;
pub mod credential_presence;
pub mod space;
pub mod worker;

use client::ClientError;
use noteit_core::embedding::{
    Embedding, EmbeddingRole, EmbeddingSpaceId, EmbeddingVector, SemanticError,
};
use noteit_core::semantic::EmbeddingProvider;
use noteit_embed_protocol::{ProviderId, Role};
use std::path::PathBuf;
use std::sync::Arc;
use worker::{WorkerHandle, WorkerPaths};

/// How the space names this family of providers.
pub use noteit_embed_protocol::ProviderId as RemoteProviderId;

pub use credential_presence::{presence as credential_presence, CredentialPresence};

/// Whether the worker would find a key for this provider. Never the key.
pub fn credential_present(provider: ProviderId, config_dir: &std::path::Path) -> bool {
    credential_presence::presence(provider, config_dir).is_present()
}

/// What a remote provider was configured to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteConfig {
    pub provider: ProviderId,
    pub model: String,
    /// The dimension asked for, and therefore the one the space declares.
    ///
    /// Always concrete by the time it reaches here: a space with an unknown
    /// dimension is a space nothing can be compared in, so resolving the
    /// model's default is the configuration layer's job and not a `None` this
    /// type has to carry (§31).
    pub dimension: usize,
    /// Whether the dimension was asked for explicitly.
    ///
    /// `text-embedding-ada-002` refuses a `dimensions` field, so "1536 because
    /// that is what this model gives" and "1536 because somebody asked" have to
    /// be told apart on the wire even though they name the same number.
    pub dimension_requested: bool,
}

/// A provider whose vectors are made in another process.
pub struct RemoteProvider {
    config: RemoteConfig,
    space: EmbeddingSpaceId,
    worker: Arc<WorkerHandle>,
}

impl RemoteProvider {
    /// Builds a provider and the handle to the worker it will use.
    ///
    /// **No process is started here.** A provider that spawned on construction
    /// would make "is remote retrieval configured" and "is a worker running"
    /// the same question, and §44 says they are not: a configured
    /// remote provider that is never asked anything must cost no process.
    pub fn new(config: RemoteConfig, config_dir: PathBuf, runtime_dir: PathBuf) -> Self {
        let space = space::space_for(config.provider, &config.model, config.dimension);
        let worker = Arc::new(WorkerHandle::new(WorkerPaths::resolve(
            config_dir,
            &runtime_dir,
        )));
        Self {
            config,
            space,
            worker,
        }
    }

    /// The same, with a worker handle the caller made. Used by the suite to
    /// point at a worker built for a test.
    pub fn with_worker(config: RemoteConfig, worker: Arc<WorkerHandle>) -> Self {
        let space = space::space_for(config.provider, &config.model, config.dimension);
        Self {
            config,
            space,
            worker,
        }
    }

    pub fn config(&self) -> &RemoteConfig {
        &self.config
    }

    pub fn worker(&self) -> &Arc<WorkerHandle> {
        &self.worker
    }

    /// Whether a worker is running, without starting one.
    pub fn worker_is_running(&self) -> bool {
        self.worker.is_running()
    }

    fn dimension_argument(&self) -> Option<u32> {
        self.config
            .dimension_requested
            .then_some(self.config.dimension as u32)
    }

    /// Embeds a batch, one round trip per protocol batch, in order.
    fn embed_all(&self, texts: &[String], role: Role) -> Result<Vec<Embedding>, SemanticError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let socket = self
            .worker
            .ensure()
            .map_err(|_| SemanticError::Unavailable)?;

        let mut embeddings = Vec::with_capacity(texts.len());
        for run in client::batches(texts.len()) {
            let slice = &texts[run.clone()];
            let vectors = client::request(
                &socket,
                self.config.provider,
                &self.config.model,
                role,
                self.dimension_argument(),
                slice,
            )
            .map_err(to_semantic)?;
            if vectors.len() != slice.len() {
                return Err(SemanticError::InvalidResponse);
            }
            for values in vectors {
                // Checked here as well as by the worker and by the protocol.
                // This is the last gate before a number becomes an index entry,
                // and it is the one that knows what space was expected.
                if values.len() != self.space.dimension {
                    return Err(SemanticError::DimensionMismatch {
                        expected: self.space.dimension,
                        actual: values.len(),
                    });
                }
                let vector = EmbeddingVector::new(values)?;
                let role = match role {
                    Role::Document => EmbeddingRole::Document,
                    Role::Query => EmbeddingRole::Query,
                };
                embeddings.push(Embedding::new(self.space.clone(), role, vector)?);
            }
        }
        if embeddings.len() != texts.len() {
            return Err(SemanticError::InvalidResponse);
        }
        Ok(embeddings)
    }
}

/// A worker's word, in the Core's vocabulary.
///
/// One place, so "429 is `RateLimited`" is decided once. Nothing about a
/// vendor's sentence, request identifier or body survives this function,
/// because none of it was ever carried this far (§25).
pub fn to_semantic(error: ClientError) -> SemanticError {
    use noteit_embed_protocol::WireError;
    match error {
        ClientError::Unavailable => SemanticError::Unavailable,
        ClientError::Protocol => SemanticError::InvalidResponse,
        ClientError::Cancelled => SemanticError::Cancelled,
        ClientError::Reported(reported) => match reported {
            WireError::Unavailable => SemanticError::Unavailable,
            WireError::Timeout => SemanticError::Timeout,
            WireError::Authentication | WireError::CredentialMissing => {
                SemanticError::Authentication
            }
            WireError::ModelUnavailable => SemanticError::ModelUnavailable,
            WireError::RateLimited => SemanticError::RateLimited,
            WireError::InvalidResponse | WireError::Protocol | WireError::Unsupported => {
                SemanticError::InvalidResponse
            }
            WireError::DimensionMismatch => SemanticError::DimensionMismatch {
                expected: 0,
                actual: 0,
            },
            WireError::Cancelled => SemanticError::Cancelled,
        },
    }
}

impl EmbeddingProvider for RemoteProvider {
    fn space(&self) -> EmbeddingSpaceId {
        self.space.clone()
    }

    fn embed_document(&self, texts: &[String]) -> Result<Vec<Embedding>, SemanticError> {
        self.embed_all(texts, Role::Document)
    }

    fn embed_query(&self, text: &str) -> Result<Embedding, SemanticError> {
        let mut answer = self.embed_all(std::slice::from_ref(&text.to_string()), Role::Query)?;
        if answer.len() != 1 {
            return Err(SemanticError::InvalidResponse);
        }
        Ok(answer.remove(0))
    }
}

/// Redacted on purpose, like the local provider's.
///
/// A derived `Debug` would print the space and, worse, invite somebody to add a
/// field later that should not be printed. The model and the dimension are what
/// help whoever is debugging.
impl std::fmt::Debug for RemoteProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteProvider")
            .field("provider", &self.config.provider.as_str())
            .field("model", &self.config.model)
            .field("dimension", &self.config.dimension)
            .field("worker_running", &self.worker.is_running())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noteit_embed_protocol::WireError;

    fn config() -> RemoteConfig {
        RemoteConfig {
            provider: ProviderId::OpenAi,
            model: "text-embedding-3-small".to_string(),
            dimension: 1536,
            dimension_requested: true,
        }
    }

    #[test]
    fn every_worker_word_becomes_one_of_the_cores() {
        assert_eq!(
            to_semantic(ClientError::Reported(WireError::Authentication)),
            SemanticError::Authentication
        );
        assert_eq!(
            to_semantic(ClientError::Reported(WireError::CredentialMissing)),
            SemanticError::Authentication
        );
        assert_eq!(
            to_semantic(ClientError::Reported(WireError::RateLimited)),
            SemanticError::RateLimited
        );
        assert_eq!(
            to_semantic(ClientError::Reported(WireError::Timeout)),
            SemanticError::Timeout
        );
        assert_eq!(
            to_semantic(ClientError::Reported(WireError::ModelUnavailable)),
            SemanticError::ModelUnavailable
        );
        assert_eq!(
            to_semantic(ClientError::Reported(WireError::Cancelled)),
            SemanticError::Cancelled
        );
        assert_eq!(
            to_semantic(ClientError::Unavailable),
            SemanticError::Unavailable
        );
        assert_eq!(
            to_semantic(ClientError::Protocol),
            SemanticError::InvalidResponse
        );
    }

    #[test]
    fn no_translated_error_carries_a_vendors_words() {
        // `WireError` and `SemanticError` are both closed sets of Note-it's own
        // vocabulary, so this is structurally true. Asserted anyway, because
        // the day somebody adds a `String` to a variant is the day it stops
        // being.
        for wire in [
            WireError::Unavailable,
            WireError::Timeout,
            WireError::Authentication,
            WireError::CredentialMissing,
            WireError::ModelUnavailable,
            WireError::RateLimited,
            WireError::InvalidResponse,
            WireError::DimensionMismatch,
            WireError::Cancelled,
            WireError::Protocol,
            WireError::Unsupported,
        ] {
            let translated = to_semantic(ClientError::Reported(wire));
            let printed = format!("{translated:?} {translated}");
            for leak in ["http", "request_id", "Bearer", "api.openai", "429 Too Many"] {
                assert!(
                    !printed.to_lowercase().contains(&leak.to_lowercase()),
                    "{wire:?} produced {printed}"
                );
            }
        }
    }

    #[test]
    fn constructing_a_provider_starts_no_process() {
        // §44: a configured remote provider that is never asked anything
        // must cost no process.
        let temporary = std::env::temp_dir().join(format!(
            "noteit-remote-nostart-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&temporary);
        std::fs::create_dir_all(&temporary).expect("mkdir");
        let provider = RemoteProvider::new(config(), temporary.clone(), temporary.clone());
        assert!(!provider.worker_is_running());
        assert!(provider.worker().child_pid().is_none());
        let _ = std::fs::remove_dir_all(&temporary);
    }

    #[test]
    fn the_space_is_built_from_configuration_and_not_from_an_answer() {
        let temporary = std::env::temp_dir().join("noteit-remote-space-probe");
        let provider = RemoteProvider::new(config(), temporary.clone(), temporary);
        let space = provider.space();
        assert_eq!(space.provider, "openai");
        assert_eq!(space.model, "text-embedding-3-small");
        assert_eq!(space.dimension, 1536);
        assert!(space.artifact.is_verifiable());
    }

    #[test]
    fn an_unrequested_dimension_is_not_sent_as_one() {
        let temporary = std::env::temp_dir().join("noteit-remote-dim-probe");
        let mut settings = config();
        settings.dimension_requested = false;
        let provider = RemoteProvider::new(settings, temporary.clone(), temporary);
        assert_eq!(provider.dimension_argument(), None);
        // And the space still declares the number, because a space with no
        // dimension is a space nothing can be compared in.
        assert_eq!(provider.space().dimension, 1536);
    }

    #[test]
    fn the_debug_of_a_provider_prints_no_vector_and_no_secret() {
        let temporary = std::env::temp_dir().join("noteit-remote-debug-probe");
        let provider = RemoteProvider::new(config(), temporary.clone(), temporary);
        let printed = format!("{provider:?}");
        assert!(printed.contains("text-embedding-3-small"));
        for leak in ["key", "secret", "token", "credential"] {
            assert!(!printed.to_lowercase().contains(leak), "{printed}");
        }
    }
}
