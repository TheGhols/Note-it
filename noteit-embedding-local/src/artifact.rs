//! The artifact on disk, and what proves it is the artifact it claims to be.
//!
//! Every guarantee here is about **bytes that were read**, never about a file
//! name, a directory or a string somebody typed. The model's name does not
//! establish that two sets of weights are the same weights: §5.1 of
//! `docs/semantic-retrieval.md` measured the failure that follows when it is
//! trusted — perfectly calculable cosines over a ranking that had collapsed,
//! with nothing in the arithmetic to say so.

use noteit_core::embedding::{ArtifactIdentity, ArtifactManifestV1};
use noteit_core::hashing::sha256_hex;
use std::fs;
use std::path::{Path, PathBuf};

/// The weights file, by the name the model's publisher gives it.
pub const WEIGHTS_FILE: &str = "model.safetensors";
/// The tokenizer file, likewise.
pub const TOKENIZER_FILE: &str = "tokenizer.json";

/// Ceilings on what will be read into memory.
///
/// A directory is configuration, and configuration is not trusted with the
/// process's memory: without these, a file that claims to be a tokenizer and
/// is forty gigabytes of anything is an out-of-memory kill rather than a typed
/// refusal. Both are far above the real artifact — the weights are 489 MiB and
/// the tokenizer 17.8 MiB — and far below a machine's patience.
pub const MAX_WEIGHTS_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_TOKENIZER_BYTES: u64 = 256 * 1024 * 1024;

/// What the running code expects to find, so that finding something else is an
/// event rather than a silent substitution.
///
/// The digests here are a **second** check and never the first one: the
/// identity that reaches [`noteit_core::embedding::EmbeddingSpaceId`] is
/// always computed from the bytes that were read. A pinned digest cannot make
/// an artifact verified; it can only say that the verified artifact is not the
/// one this build was written against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactExpectation {
    /// How the space names the model. Not a switch — nothing branches on it.
    pub model: &'static str,
    /// The publisher's immutable revision for that name.
    pub revision: &'static str,
    /// The dimension the table must have.
    pub dimension: usize,
    /// How many rows the table must have.
    pub rows: usize,
    pub weights_sha256: &'static str,
    pub tokenizer_sha256: &'static str,
}

/// Why an artifact could not be turned into a provider.
///
/// Facts, not sentences, and nothing here carries a path or a library's own
/// error text: the caller decides what a person is told, and
/// `scripts/check-mcp-boundary` already forbids a vendor string reaching the
/// wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactError {
    /// One of the two files is not there. The ordinary state of a machine that
    /// never provisioned the model, and not an error in itself.
    Missing,
    /// Something is at the path and it is not a regular file: a directory, a
    /// device, a FIFO, or a symlink. A symlink is refused rather than followed
    /// because the point of hashing the bytes is to know which bytes, and a
    /// link is an invitation to change that answer between two runs.
    NotARegularFile,
    /// It is a regular file and it could not be read.
    Unreadable,
    /// Empty, or larger than this crate will read.
    ImplausibleSize,
    /// The bytes are not the container they claim to be, or the tensor inside
    /// is not a two-dimensional `f32` table.
    Malformed,
    /// It parsed, and it is not the artifact this build expects: a different
    /// digest, a different shape.
    Unexpected,
}

/// The two files, read, checked and hashed.
pub struct LoadedArtifact {
    pub weights: Vec<u8>,
    pub tokenizer: Vec<u8>,
    pub weights_sha256: String,
    pub tokenizer_sha256: String,
}

/// The manifest over what was read, and the identity that follows from it.
///
/// The two digests are always of the buffers the tokenizer and the table are
/// then built from, so there is no window in which the thing hashed and the
/// thing used could differ. Nothing here can be handed a model's *name* and
/// asked to call it verified.
pub fn identity_of(
    weights_sha256: &str,
    tokenizer_sha256: &str,
    embedding_recipe_version: u32,
    normalization_version: u32,
) -> Result<ArtifactIdentity, ArtifactError> {
    ArtifactManifestV1 {
        weights_sha256: weights_sha256.to_string(),
        tokenizer_sha256: tokenizer_sha256.to_string(),
        embedding_recipe_version,
        normalization_version,
    }
    .identity()
    .map_err(|_| ArtifactError::Malformed)
}

/// Where the two files live for one model at one revision.
///
/// The directory is the only part a caller chooses, and the two names are
/// constants joined onto it — so there is no component of either path that
/// user input could steer, and `..` in a configured directory can only ever
/// name a directory, never a different file inside this one.
pub fn artifact_files(directory: &Path) -> (PathBuf, PathBuf) {
    (directory.join(WEIGHTS_FILE), directory.join(TOKENIZER_FILE))
}

/// The weights, read and hashed on a thread of their own.
///
/// Verifying the artifact is the dominant cost of loading it — 489 MiB of
/// SHA-256, measured at roughly 200 MiB/s — and it has nothing to say to the
/// tokenizer, which the caller builds meanwhile. Two independent costs become
/// one wait.
pub struct WeightsInFlight {
    handle: std::thread::JoinHandle<Result<(Vec<u8>, String), ArtifactError>>,
}

impl WeightsInFlight {
    /// Waits for the reader and checks the digest it produced.
    pub fn join(self, expected: &str) -> Result<(Vec<u8>, String), ArtifactError> {
        // A panic in the reader is not a diagnosis this crate can improve on,
        // and its message could quote a path. Reported as unreadable.
        let (bytes, digest) = self
            .handle
            .join()
            .unwrap_or(Err(ArtifactError::Unreadable))?;
        if digest != expected {
            return Err(ArtifactError::Unexpected);
        }
        Ok((bytes, digest))
    }
}

/// Starts reading and hashing the weights. Nothing is verified yet.
pub fn read_weights_in_background(directory: &Path) -> WeightsInFlight {
    let path = artifact_files(directory).0;
    WeightsInFlight {
        handle: std::thread::spawn(move || {
            let bytes = read_checked(&path, MAX_WEIGHTS_BYTES)?;
            let digest = sha256_hex(&bytes);
            Ok((bytes, digest))
        }),
    }
}

/// Reads the tokenizer and refuses it unless its bytes are the expected bytes.
///
/// Checked *before* it is parsed, unlike the weights, and the asymmetry is
/// deliberate rather than sloppy: hashing 17.8 MiB costs 90 ms, so there is
/// nothing to gain by parsing first, and refusing before parsing is the
/// narrower door.
pub fn read_verified_tokenizer(
    directory: &Path,
    expected: &str,
) -> Result<(Vec<u8>, String), ArtifactError> {
    let path = artifact_files(directory).1;
    let bytes = read_checked(&path, MAX_TOKENIZER_BYTES)?;
    let digest = sha256_hex(&bytes);
    if digest != expected {
        return Err(ArtifactError::Unexpected);
    }
    Ok((bytes, digest))
}

/// Reads and verifies both files, sequentially.
///
/// The straightforward shape, kept because it is the one a test wants.
/// `LocalProvider::load` overlaps the two halves instead.
pub fn load(
    directory: &Path,
    expectation: &ArtifactExpectation,
) -> Result<LoadedArtifact, ArtifactError> {
    let in_flight = read_weights_in_background(directory);
    let tokenizer = read_verified_tokenizer(directory, expectation.tokenizer_sha256);
    let weights = in_flight.join(expectation.weights_sha256);
    let (tokenizer, tokenizer_sha256) = tokenizer?;
    let (weights, weights_sha256) = weights?;
    Ok(LoadedArtifact {
        weights,
        tokenizer,
        weights_sha256,
        tokenizer_sha256,
    })
}

/// One file, refused for every reason it can be refused for before it is read.
///
/// `symlink_metadata` and not `metadata`: the second follows the link and
/// would answer about the target, which is exactly the substitution this is
/// here to notice.
fn read_checked(path: &Path, ceiling: u64) -> Result<Vec<u8>, ArtifactError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ArtifactError::Missing)
        }
        Err(_) => return Err(ArtifactError::Unreadable),
    };
    if !metadata.is_file() {
        return Err(ArtifactError::NotARegularFile);
    }
    let length = metadata.len();
    if length == 0 || length > ceiling {
        return Err(ArtifactError::ImplausibleSize);
    }
    let bytes = fs::read(path).map_err(|_| ArtifactError::Unreadable)?;
    // The length was checked before the read and is checked again after it:
    // between the two, the file may have been replaced.
    if bytes.is_empty() || bytes.len() as u64 > ceiling {
        return Err(ArtifactError::ImplausibleSize);
    }
    Ok(bytes)
}
