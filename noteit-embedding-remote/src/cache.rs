//! The remote index on disk: derived, versioned, atomic, and never a note.
//!
//! ## Why this exists at all, and only for remote
//!
//! §15 of `docs/semantic-retrieval.md` measured the two modes and refused to
//! pretend they are the same question. Rebuilding a local index costs CPU
//! nobody is billed for; rebuilding a remote one costs tokens and latency. So
//! the local provider has no cache and this one is mandatory:
//! **the user must not pay to embed the whole store on every start**
//! (§20, §33).
//!
//! ## Where it lives
//!
//! `$XDG_CACHE_HOME/note-it/semantic/<space digest>/index.bin`. The cache
//! directory and never `notes/`, `trash/` or `backups/`, by the same rule the
//! model artifact follows: the data directory *is* the store, and derived data
//! inside it would be swept into backups, counted by an integrity check and
//! confused with somebody's writing. One directory per
//! `EmbeddingSpaceId` (§15), so keeping more than one space is a
//! retention policy and not a format change.
//!
//! ## What it may hold, and what it may not
//!
//! Holds: the vector, `note_id`, `source_revision`, `chunk_id`,
//! `chunker_version` and the whole `EmbeddingSpaceId`. Does **not** hold: the
//! note's text, a snippet, front matter, a credential, an HTTP request, an HTTP
//! response or a provider's error body (§34). The snippet always comes
//! from a fresh read of the note, and `remote_cache_holds_no_text` asserts that
//! no byte of a note's text is in the file.
//!
//! `source_revision` is stored because it is **a cache key** — the thing that
//! says a vector is about the note as it is now. It is never published, never
//! reaches an agent and never authorises a write (§41).
//!
//! ## The format, and every check on the way in
//!
//! ```text
//! [8]  magic          b"NTIRVEC\0"
//! [4]  format_version big-endian u32
//! [4]  header_len     big-endian u32
//! [N]  header         canonical JSON: space, chunker, dimension, count, records
//! [M]  body           count × dimension little-endian f32
//! [32] digest         SHA-256 of every byte above
//! ```
//!
//! A trailing digest rather than a length check alone, because a length check
//! catches truncation and trailing garbage and says nothing about a flipped
//! bit in the middle. Every one of §35's clauses is a refusal here:
//! magic, version, size, count, dimension, finiteness, space, chunker,
//! structural integrity, truncation and trailing bytes. An incompatible or
//! damaged cache is **rebuilt**, never reinterpreted.

use noteit_core::atomic_file::write_atomic;
use noteit_core::chunking::ChunkId;
use noteit_core::embedding::{Embedding, EmbeddingRole, EmbeddingSpaceId, EmbeddingVector};
use noteit_core::hashing::sha256_hex;
use noteit_core::revision::NoteRevision;
use noteit_core::semantic::EmbeddingRecord;
use noteit_core::Uuid;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The eight bytes at the start of every cache file.
pub const MAGIC: &[u8; 8] = b"NTIRVEC\0";

/// The version of the layout above.
///
/// Bumped whenever a byte moves. A file announcing another number is rebuilt
/// rather than read — there is no "compatible enough" reading of a format that
/// changed (§35).
pub const FORMAT_VERSION: u32 = 1;

/// The most bytes a cache file may be before it is refused unread.
///
/// Ten thousand notes of two paragraphs at 3 072 dimensions is about 250 MiB,
/// so this is generous for anything real and finite for anything that is not.
pub const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;

/// The mode a cache file is written with.
///
/// A vector is derived from a private note, and derived is not "not sensitive"
/// (§37). Owner only.
pub const CACHE_MODE: u32 = 0o600;

/// The most spaces kept on disk at once.
///
/// Two, and the number is a decision recorded in ADR-060 rather than a default.
/// The realistic case for keeping more than the active space is a person
/// trying one provider against another and going back, which needs exactly one
/// other; keeping every space a store has ever had grows without bound, and in
/// the remote mode each one was paid for once, so deleting them all on every
/// switch is the *other* way to charge somebody twice.
pub const MAX_RETAINED_SPACES: usize = 2;

/// Why a cache could not be used.
///
/// Every variant means the same thing to the caller — rebuild — and they are
/// distinct so a test can say which refusal fired and a diagnostic can say
/// something true. None carries a path or a byte of content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheError {
    /// There is no cache for this space.
    Absent,
    /// The path is not a regular file, or is a symlink, or cannot be read.
    Unreadable,
    /// The first bytes are not this format's.
    NotOurs,
    /// A version this build does not read.
    Version,
    /// The file is shorter or longer than its own header says it is.
    Size,
    /// The header is not the JSON this version defines.
    Header,
    /// The digest does not match the bytes.
    Corrupt,
    /// The cache is for a different space, or a different chunker.
    Incompatible,
    /// A vector is not finite, or is not the dimension the header declares.
    Vector,
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Absent => "não há cache para este espaço",
            Self::Unreadable => "o arquivo de cache não pôde ser lido",
            Self::NotOurs => "o arquivo não é um cache do Note-it",
            Self::Version => "o cache está numa versão de formato desconhecida",
            Self::Size => "o cache não tem o tamanho que o próprio cabeçalho declara",
            Self::Header => "o cabeçalho do cache não tem a forma esperada",
            Self::Corrupt => "o digest do cache não confere com os bytes",
            Self::Incompatible => "o cache é de outro espaço ou de outro chunker",
            Self::Vector => "o cache contém um vetor inválido",
        })
    }
}

impl std::error::Error for CacheError {}

/// One record's metadata, as it appears in the header.
///
/// Structural only. There is no field for text, and that is the enforcement
/// (§34).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RecordHeader {
    note_id: String,
    source_revision: String,
    chunk_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SpaceHeader {
    provider: String,
    model: String,
    artifact: String,
    dimension: usize,
    embedding_recipe: u32,
    normalization: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Header {
    format_version: u32,
    space: SpaceHeader,
    space_digest: String,
    chunker_version: u32,
    dimension: usize,
    count: usize,
    records: Vec<RecordHeader>,
}

fn space_header(space: &EmbeddingSpaceId) -> SpaceHeader {
    SpaceHeader {
        provider: space.provider.clone(),
        model: space.model.clone(),
        artifact: format!("{:?}", space.artifact),
        dimension: space.dimension,
        embedding_recipe: space.embedding_recipe,
        normalization: space.normalization,
    }
}

/// Where a space's cache lives.
pub fn cache_directory(root: &Path, space: &EmbeddingSpaceId) -> PathBuf {
    root.join("semantic")
        .join(crate::space::space_digest(space))
}

/// The file inside it.
pub fn cache_file(root: &Path, space: &EmbeddingSpaceId) -> PathBuf {
    cache_directory(root, space).join("index.bin")
}

/// The cache root, under XDG and never inside the store.
pub fn default_root() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("note-it"))
}

/// Writes every record for one space, atomically.
///
/// **The commit point is the rename**, which is the discipline Phase 3.4R.2
/// established for a note and which this does not get to have a second opinion
/// about: `noteit_core::atomic_file::write_atomic` is called rather than
/// reimplemented. A crash before it leaves the previous cache whole and valid;
/// a crash after it leaves the new one complete and recognisable. There is no
/// moment at which a half-written index is announced as a finished one
/// (§36, §79).
///
/// The bytes are validated *before* they are written — the digest is computed
/// over what is about to be committed — so a cache this process wrote is one
/// this process can read back.
pub fn save(
    root: &Path,
    space: &EmbeddingSpaceId,
    chunker_version: u32,
    records: &[EmbeddingRecord],
) -> Result<(), CacheError> {
    let directory = cache_directory(root, space);
    fs::create_dir_all(&directory).map_err(|_| CacheError::Unreadable)?;
    // The directory holds vectors derived from private notes, so it is no more
    // public than they are.
    let _ = fs::set_permissions(&directory, fs::Permissions::from_mode(0o700));

    let dimension = space.dimension;
    let mut headers = Vec::with_capacity(records.len());
    let mut body = Vec::with_capacity(records.len() * dimension * 4);
    for record in records {
        if record.space != *space {
            return Err(CacheError::Incompatible);
        }
        let values = record.vector.vector();
        if values.dimension() != dimension {
            return Err(CacheError::Vector);
        }
        headers.push(RecordHeader {
            note_id: record.note_id.hyphenated().to_string(),
            source_revision: record.source_revision.as_str().to_string(),
            chunk_id: record.chunk_id.as_str().to_string(),
        });
        for value in values.components() {
            if !value.is_finite() {
                return Err(CacheError::Vector);
            }
            body.extend_from_slice(&value.to_le_bytes());
        }
    }

    let header = Header {
        format_version: FORMAT_VERSION,
        space: space_header(space),
        space_digest: crate::space::space_digest(space),
        chunker_version,
        dimension,
        count: records.len(),
        records: headers,
    };
    let header = serde_json::to_vec(&header).map_err(|_| CacheError::Header)?;

    let mut file = Vec::with_capacity(16 + header.len() + body.len() + 32);
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
    file.extend_from_slice(&(header.len() as u32).to_be_bytes());
    file.extend_from_slice(&header);
    file.extend_from_slice(&body);
    let digest = sha256_hex(&file);
    file.extend_from_slice(&hex_to_bytes(&digest));

    let path = cache_file(root, space);
    write_atomic(&path, &file, "o cache semântico remoto").map_err(|_| CacheError::Unreadable)?;
    // Applied after the commit, so a reader either sees the old file or the new
    // one — never a new file at a mode nobody chose.
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(CACHE_MODE));
    Ok(())
}

/// Reads a space's cache, or says why it will not.
///
/// Fail closed at every step. Nothing here repairs, truncates or reinterprets:
/// the caller's answer to every one of these is to rebuild, which is always
/// correct because the notes were never in danger.
pub fn load(
    root: &Path,
    space: &EmbeddingSpaceId,
    chunker_version: u32,
) -> Result<Vec<EmbeddingRecord>, CacheError> {
    let path = cache_file(root, space);
    // `symlink_metadata`, and a symlink is refused rather than followed: what
    // a name points at may change between the check and the read, which is the
    // same rule the artifact loader applies.
    let metadata = fs::symlink_metadata(&path).map_err(|_| CacheError::Absent)?;
    if metadata.file_type().is_symlink() {
        return Err(CacheError::Unreadable);
    }
    if !metadata.is_file() {
        return Err(CacheError::Unreadable);
    }
    if metadata.len() > MAX_CACHE_BYTES {
        return Err(CacheError::Size);
    }
    if metadata.len() < (16 + 32) {
        return Err(CacheError::Size);
    }
    let bytes = fs::read(&path).map_err(|_| CacheError::Unreadable)?;
    decode(&bytes, space, chunker_version)
}

/// The whole reading, separated from the filesystem so a test can hand it any
/// bytes at all.
pub fn decode(
    bytes: &[u8],
    space: &EmbeddingSpaceId,
    chunker_version: u32,
) -> Result<Vec<EmbeddingRecord>, CacheError> {
    if bytes.len() < 16 + 32 {
        return Err(CacheError::Size);
    }
    if &bytes[0..8] != MAGIC {
        return Err(CacheError::NotOurs);
    }
    let version = u32::from_be_bytes(bytes[8..12].try_into().map_err(|_| CacheError::Size)?);
    if version != FORMAT_VERSION {
        return Err(CacheError::Version);
    }
    let header_len =
        u32::from_be_bytes(bytes[12..16].try_into().map_err(|_| CacheError::Size)?) as usize;
    // Checked against the file's own length before it is used to slice, which
    // is the same ordering the wire protocol's frame ceiling uses and for the
    // same reason.
    let header_end = 16usize.checked_add(header_len).ok_or(CacheError::Size)?;
    if header_end + 32 > bytes.len() {
        return Err(CacheError::Size);
    }

    let digest_at = bytes.len() - 32;
    let expected = sha256_hex(&bytes[..digest_at]);
    if hex_to_bytes(&expected) != bytes[digest_at..] {
        return Err(CacheError::Corrupt);
    }

    let header: Header =
        serde_json::from_slice(&bytes[16..header_end]).map_err(|_| CacheError::Header)?;
    if header.format_version != FORMAT_VERSION {
        return Err(CacheError::Version);
    }
    if header.chunker_version != chunker_version {
        return Err(CacheError::Incompatible);
    }
    if header.space != space_header(space)
        || header.space_digest != crate::space::space_digest(space)
    {
        return Err(CacheError::Incompatible);
    }
    if header.dimension != space.dimension || header.dimension == 0 {
        return Err(CacheError::Incompatible);
    }
    if header.records.len() != header.count {
        return Err(CacheError::Header);
    }

    // The size the header implies, compared with the size the file has. This
    // is what catches both truncation and trailing bytes — a file that is one
    // vector short and a file with a kilobyte of garbage appended both fail
    // here rather than being read as far as they go.
    let body_len = header
        .count
        .checked_mul(header.dimension)
        .and_then(|values| values.checked_mul(4))
        .ok_or(CacheError::Size)?;
    if digest_at != header_end + body_len {
        return Err(CacheError::Size);
    }

    let body = &bytes[header_end..digest_at];
    let mut records = Vec::with_capacity(header.count);
    for (index, meta) in header.records.iter().enumerate() {
        let start = index * header.dimension * 4;
        let mut values = Vec::with_capacity(header.dimension);
        for position in 0..header.dimension {
            let at = start + position * 4;
            let value =
                f32::from_le_bytes(body[at..at + 4].try_into().map_err(|_| CacheError::Size)?);
            // Refused here, before it can become an index entry and a ranking.
            if !value.is_finite() {
                return Err(CacheError::Vector);
            }
            values.push(value);
        }
        let note_id = Uuid::parse_str(&meta.note_id).map_err(|_| CacheError::Header)?;
        let source_revision =
            NoteRevision::parse(&meta.source_revision).map_err(|_| CacheError::Header)?;
        let chunk_id = ChunkId::from_digest(&meta.chunk_id).ok_or(CacheError::Header)?;
        // `EmbeddingVector::new` refuses a zero norm as well as a non-finite
        // component, so this is the last of §35's numeric clauses.
        let vector = EmbeddingVector::new(values).map_err(|_| CacheError::Vector)?;
        let embedded = Embedding::new(space.clone(), EmbeddingRole::Document, vector)
            .map_err(|_| CacheError::Vector)?;
        records.push(EmbeddingRecord {
            note_id,
            source_revision,
            chunk_id,
            chunker_version: header.chunker_version,
            space: space.clone(),
            vector: embedded,
        });
    }
    Ok(records)
}

/// Removes every cached space but the newest [`MAX_RETAINED_SPACES`].
///
/// Called after a successful save, so the active space is always among the
/// kept. Deleting is safe by construction: a cache is derived, and losing one
/// costs the money to rebuild it and never a note (§83 — clearing a
/// cache never deletes a note).
pub fn prune(root: &Path, keep: &EmbeddingSpaceId) -> usize {
    let semantic = root.join("semantic");
    let Ok(entries) = fs::read_dir(&semantic) else {
        return 0;
    };
    let keeping = crate::space::space_digest(keep);
    let mut spaces: Vec<(std::time::SystemTime, PathBuf, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };
        // Only ever a directory this module made: a name that is not a digest
        // is not ours, and this function does not delete what it did not
        // create.
        if !noteit_core::embedding::is_digest(&name) || !path.is_dir() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        spaces.push((modified, path, name));
    }
    // Newest first, so the one kept beside the active space is the one
    // somebody most recently used.
    spaces.sort_by_key(|(modified, _, _)| std::cmp::Reverse(*modified));

    let mut removed = 0;
    let mut kept = 0;
    for (_, path, name) in spaces {
        if name == keeping {
            continue;
        }
        kept += 1;
        if kept >= MAX_RETAINED_SPACES && fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Forgets one space's cache entirely. The rebuild path (§80).
pub fn clear(root: &Path, space: &EmbeddingSpaceId) -> Result<(), CacheError> {
    let directory = cache_directory(root, space);
    match fs::remove_dir_all(&directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(CacheError::Unreadable),
    }
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).unwrap_or(0) as u8;
            let low = (pair[1] as char).to_digit(16).unwrap_or(0) as u8;
            (high << 4) | low
        })
        .collect()
}

/// Builds a record from a vector and its provenance.
///
/// Public because a caller that already has both — the suite, and anything
/// rebuilding an index from parts it holds — should not be reimplementing the
/// two constructors this wraps. It refuses exactly what they refuse: a
/// non-finite component, a zero norm, and a dimension the space does not
/// declare.
pub fn record_for(
    space: &EmbeddingSpaceId,
    note_id: Uuid,
    source_revision: NoteRevision,
    chunk_id: ChunkId,
    chunker_version: u32,
    values: Vec<f32>,
) -> Result<EmbeddingRecord, noteit_core::embedding::SemanticError> {
    let vector = EmbeddingVector::new(values)?;
    Ok(EmbeddingRecord {
        note_id,
        source_revision,
        chunk_id,
        chunker_version,
        space: space.clone(),
        vector: Embedding::new(space.clone(), EmbeddingRole::Document, vector)?,
    })
}
