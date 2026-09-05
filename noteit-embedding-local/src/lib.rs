//! The local embedding provider: a table, a tokenizer, and no network.
//!
//! This crate exists so that `noteit-core` never learns what runs behind
//! [`noteit_core::semantic::EmbeddingProvider`]. The Core defines the contract
//! and reasons in its terms; the tokenizer, the weight format and the pooling
//! live here, on the other side of a crate boundary that a type cannot cross
//! by accident.
//!
//! ## What this is not
//!
//! Not a sandbox. `EmbeddingProvider` is an API and data-minimisation
//! boundary: the Context Engine hands this crate text to embed and nothing
//! else — no path, no revision, no store root, no write authority — but a
//! provider running in-process holds the process's privileges like any other
//! module. Process isolation is 4.3D's problem, for remote providers, and this
//! one has no remote half to isolate.
//!
//! ## No network, structurally
//!
//! Nothing here opens a socket, and nothing in the dependency graph can: the
//! tokenizer is built without its `http` feature, the weights are a byte
//! layout rather than a download, and `scripts/check-embedding-boundary`
//! fails the build if an HTTP, TLS or socket crate appears. Obtaining the
//! artifact in the first place is a separate, explicit act —
//! `scripts/fetch-embedding-artifact` — that this crate never performs and
//! cannot trigger.

pub mod artifact;
mod table;

pub use artifact::{ArtifactError, ArtifactExpectation};

use noteit_core::embedding::{
    Embedding, EmbeddingRole, EmbeddingSpaceId, EmbeddingVector, SemanticError,
};
use noteit_core::semantic::EmbeddingProvider;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use table::EmbeddingTable;
use tokenizers::Tokenizer;

/// How the space names this provider. A label that travels, never a switch.
pub const PROVIDER_ID: &str = "local";

/// The version of the **pair** of recipes — document and query together.
///
/// One number for both halves, because changing only the query side
/// invalidates comparison with already-indexed documents exactly as much as
/// changing the document side.
///
/// Recipe 1, in full, so that a later version is a decision and not a drift:
///
/// * the text reaches the tokenizer exactly as the chunker produced it, with
///   **no prefix on either side**. A static table has no `passage: ` / `query: `
///   protocol to honour — the model was trained without one, and inventing a
///   prefix would move both halves out of the space the weights describe;
/// * the token sequence is truncated to [`MAX_TOKENS`];
/// * identifiers equal to the tokenizer's declared unknown token, when it
///   declares one, are dropped;
/// * the surviving rows are averaged and the average is L2-normalised.
///
/// Document and query are therefore prepared identically under recipe 1. They
/// remain two functions, because which of them a caller needs is not this
/// crate's to guess and the next model may well need them to differ.
pub const EMBEDDING_RECIPE_VERSION: u32 = 1;

/// The version of Note-it's own text normalisation before embedding.
///
/// Version 1 is **none**: the visible text goes to the tokenizer unchanged.
/// `search::fold` is deliberately not applied — it lowercases and strips
/// accents for the lexical side, and feeding a multilingual model text with
/// its accents removed asks it about words nobody wrote.
pub const NORMALIZATION_VERSION: u32 = 1;

/// The token ceiling one text may contribute.
///
/// The chunker already caps a chunk at 800 characters and the engine caps a
/// query, so this is a backstop rather than a routine cut.
pub const MAX_TOKENS: usize = 512;

/// The artifact this build was written against.
///
/// Pinned by revision **and** by the digest of each file, because a revision
/// is a promise made by a server and a digest is a fact about bytes. The
/// digests are a second check; the identity the space carries is always
/// recomputed from what was read.
pub const POTION_MULTILINGUAL_128M: ArtifactExpectation = ArtifactExpectation {
    model: "potion-multilingual-128M",
    revision: "73908c3438cf03b6a01bcb9611d62b23d0726f08",
    dimension: 256,
    rows: 500_353,
    weights_sha256: "14b5eb39cb4ce5666da8ad1f3dc6be4346e9b2d601c073302fa0a31bf7943397",
    tokenizer_sha256: "19f1909063da3cfe3bd83a782381f040dccea475f4816de11116444a73e1b6a1",
};

/// Where the artifact is looked for, under the XDG cache root.
///
/// The cache and not the data directory, and the reason is not taste: the data
/// directory *is* the note store, and half a gigabyte of model inside it would
/// be swept into backups, counted by every integrity check and confused with
/// the user's own writing. The model is re-obtainable; the notes are not. The
/// revision is part of the path so a new one cannot land on top of an old one
/// — though it is the digest, never the path, that decides identity.
pub fn artifact_directory(expectation: &ArtifactExpectation) -> Option<PathBuf> {
    Some(
        dirs::cache_dir()?
            .join("note-it")
            .join("embedding")
            .join(expectation.model)
            .join(expectation.revision),
    )
}

/// The one field this crate reads out of `tokenizer.json` itself.
///
/// A typed peek and not a `serde_json::Value`: the file is 17.8 MiB and
/// materialising it as a tree costs hundreds of megabytes, measured. Serde
/// skips what is not named here without allocating it.
#[derive(Deserialize)]
struct TokenizerPeek {
    model: TokenizerModelPeek,
}

#[derive(Deserialize)]
struct TokenizerModelPeek {
    #[serde(default)]
    unk_token: Option<String>,
}

/// A loaded, verified, local provider.
///
/// Holds no path, no store handle and no write authority — there is no field
/// one could go in, which is why "the provider cannot write a note" needs no
/// runtime check.
pub struct LocalProvider {
    tokenizer: Tokenizer,
    table: EmbeddingTable,
    unknown_token: Option<u32>,
    space: EmbeddingSpaceId,
}

impl LocalProvider {
    /// Loads the artifact this build expects, from the XDG location.
    ///
    /// Returns [`ArtifactError::Missing`] when the machine simply never
    /// provisioned a model. That is an ordinary state and not a fault: the
    /// factory default never asks for one.
    pub fn load_default() -> Result<Self, ArtifactError> {
        let directory =
            artifact_directory(&POTION_MULTILINGUAL_128M).ok_or(ArtifactError::Missing)?;
        Self::load(&directory, &POTION_MULTILINGUAL_128M)
    }

    /// Loads a named artifact from a named directory.
    ///
    /// Nothing is downloaded, here or anywhere below. An absent artifact is a
    /// typed refusal, never a fetch.
    /// The two halves are read, hashed and built at the same time.
    ///
    /// Sequentially the load is the SHA-256 of the weights *plus* the
    /// tokenizer's own read, hash and construction; overlapped it is the
    /// larger of the two. Both halves are still refused before either is
    /// trusted — nothing here returns a provider built from bytes whose digest
    /// did not match.
    pub fn load(
        directory: &Path,
        expectation: &ArtifactExpectation,
    ) -> Result<Self, ArtifactError> {
        let in_flight = artifact::read_weights_in_background(directory);

        // The tokenizer half, entirely, while the weights are being hashed.
        // A refusal here still joins the other thread rather than abandoning
        // half a gigabyte mid-read.
        let tokenising = Self::build_tokenizer(directory, expectation);
        let weights = in_flight.join(expectation.weights_sha256);

        let (tokenizer, unknown_token, tokenizer_sha256) = tokenising?;
        let (weights, weights_sha256) = weights?;

        let identity = artifact::identity_of(
            &weights_sha256,
            &tokenizer_sha256,
            EMBEDDING_RECIPE_VERSION,
            NORMALIZATION_VERSION,
        )?;

        let table = EmbeddingTable::parse(weights, expectation.rows, expectation.dimension)?;

        let space = EmbeddingSpaceId {
            provider: PROVIDER_ID.to_string(),
            model: expectation.model.to_string(),
            artifact: identity,
            dimension: table.dimension(),
            embedding_recipe: EMBEDDING_RECIPE_VERSION,
            normalization: NORMALIZATION_VERSION,
        };

        Ok(Self {
            tokenizer,
            table,
            unknown_token,
            space,
        })
    }

    /// Reads, verifies and builds the tokenizer half.
    fn build_tokenizer(
        directory: &Path,
        expectation: &ArtifactExpectation,
    ) -> Result<(Tokenizer, Option<u32>, String), ArtifactError> {
        let (bytes, digest) =
            artifact::read_verified_tokenizer(directory, expectation.tokenizer_sha256)?;
        let tokenizer = Tokenizer::from_bytes(&bytes).map_err(|_| ArtifactError::Malformed)?;
        let peek: TokenizerPeek =
            serde_json::from_slice(&bytes).map_err(|_| ArtifactError::Malformed)?;
        let unknown_token = peek
            .model
            .unk_token
            .as_deref()
            .and_then(|token| tokenizer.token_to_id(token));
        Ok((tokenizer, unknown_token, digest))
    }

    /// How many rows the table holds. Diagnostics; never published.
    pub fn table_rows(&self) -> usize {
        self.table.rows()
    }

    /// Recipe 1, applied. The one place text becomes token identifiers.
    fn token_ids(&self, text: &str) -> Result<Vec<u32>, SemanticError> {
        let encoding = self
            .tokenizer
            .encode_fast(text, false)
            .map_err(|_| SemanticError::InvalidResponse)?;
        let mut ids: Vec<u32> = encoding.get_ids().to_vec();
        if let Some(unknown) = self.unknown_token {
            ids.retain(|id| *id != unknown);
        }
        ids.truncate(MAX_TOKENS);
        Ok(ids)
    }

    fn embed(&self, text: &str, role: EmbeddingRole) -> Result<Embedding, SemanticError> {
        let ids = self.token_ids(text)?;
        let values = self.table.pool(&ids).ok_or(SemanticError::InvalidVector)?;
        let vector = EmbeddingVector::new(values)?;
        Embedding::new(self.space.clone(), role, vector)
    }
}

/// Redacted on purpose.
///
/// A derived `Debug` here would put half a gigabyte of weights into whatever
/// formatted it — a log line, a panic message, an `assert_eq!` that failed.
/// The shape and the space are what help somebody debugging; the table is the
/// part that only costs.
impl std::fmt::Debug for LocalProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalProvider")
            .field("model", &self.space.model)
            .field("dimension", &self.space.dimension)
            .field("rows", &self.table.rows())
            .field("table", &"<redacted>")
            .finish()
    }
}

impl EmbeddingProvider for LocalProvider {
    fn space(&self) -> EmbeddingSpaceId {
        self.space.clone()
    }

    /// One vector per text, in order, or nothing at all.
    ///
    /// The batch is all-or-nothing because the count is the only thing that
    /// says which vector belongs to which chunk. A batch that came back one
    /// short and lined up by accident would attach paragraph three's meaning
    /// to paragraph four's identity.
    fn embed_document(&self, texts: &[String]) -> Result<Vec<Embedding>, SemanticError> {
        let mut vectors = Vec::with_capacity(texts.len());
        for text in texts {
            vectors.push(self.embed(text, EmbeddingRole::Document)?);
        }
        Ok(vectors)
    }

    fn embed_query(&self, text: &str) -> Result<Embedding, SemanticError> {
        self.embed(text, EmbeddingRole::Query)
    }
}
