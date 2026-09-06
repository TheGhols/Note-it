//! The remote cache, against every way §78 says a file on disk can be
//! wrong.
//!
//! The rule under test is one sentence: **an incompatible or damaged cache is
//! rebuilt, never reinterpreted.** So every case here asserts two things — that
//! the load refuses, and that the refusal is a typed one rather than a panic,
//! a partial read or a silently emptier index.

use noteit_core::chunking::{ChunkId, CHUNKER_VERSION};
use noteit_core::embedding::EmbeddingSpaceId;
use noteit_core::model::NoteDocument;
use noteit_core::revision::NoteRevision;
use noteit_core::semantic::EmbeddingRecord;
use noteit_embed_protocol::ProviderId;
use noteit_embedding_remote::cache::{self, CacheError};
use noteit_embedding_remote::space;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const DIMENSION: usize = 4;
const PRIVATE_TEXT: &str = "NOTE_TEXT_PRIVATE_456 o corpo secreto de uma nota";

fn root(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "noteit-remote-cache-{}-{name}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("mkdir");
    base
}

fn space() -> EmbeddingSpaceId {
    space::space_for(ProviderId::OpenAi, "text-embedding-3-small", DIMENSION)
}

fn other_space() -> EmbeddingSpaceId {
    space::space_for(ProviderId::Voyage, "voyage-4", DIMENSION)
}

/// A record whose note really exists, so `source_revision` is a real revision.
fn record(space: &EmbeddingSpaceId, body: &str, ordinal: u32, seed: f32) -> EmbeddingRecord {
    let mut document = NoteDocument::new_empty();
    document.content = body.to_string();
    let revision = NoteRevision::for_document(&document).expect("revision");
    let note_id = document.metadata.id;
    let chunk_id = ChunkId::of(&note_id, &revision, ordinal, CHUNKER_VERSION, body).expect("chunk");
    let values: Vec<f32> = (0..DIMENSION)
        .map(|index| seed + index as f32 * 0.25 + 0.5)
        .collect();
    cache::record_for(space, note_id, revision, chunk_id, CHUNKER_VERSION, values).expect("record")
}

fn saved(name: &str, count: usize) -> (PathBuf, EmbeddingSpaceId, Vec<EmbeddingRecord>) {
    let root = root(name);
    let space = space();
    let records: Vec<EmbeddingRecord> = (0..count)
        .map(|index| {
            record(
                &space,
                &format!("{PRIVATE_TEXT} {index}"),
                index as u32,
                index as f32,
            )
        })
        .collect();
    cache::save(&root, &space, CHUNKER_VERSION, &records).expect("save");
    (root, space, records)
}

fn file_of(root: &Path, space: &EmbeddingSpaceId) -> PathBuf {
    cache::cache_file(root, space)
}

// ------------------------------------------------------------ the round trip

#[test]
fn what_was_saved_is_what_comes_back() {
    let (root, space, records) = saved("roundtrip", 3);
    let back = cache::load(&root, &space, CHUNKER_VERSION).expect("load");
    assert_eq!(back.len(), records.len());
    for (original, restored) in records.iter().zip(&back) {
        assert_eq!(original.note_id, restored.note_id);
        assert_eq!(original.source_revision, restored.source_revision);
        assert_eq!(original.chunk_id, restored.chunk_id);
        assert_eq!(original.chunker_version, restored.chunker_version);
        assert_eq!(original.space, restored.space);
        assert_eq!(
            original.vector.vector().components(),
            restored.vector.vector().components()
        );
    }
    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_empty_cache_is_a_cache_and_not_an_absence() {
    let root = root("empty");
    let space = space();
    cache::save(&root, &space, CHUNKER_VERSION, &[]).expect("save");
    assert!(cache::load(&root, &space, CHUNKER_VERSION)
        .expect("load")
        .is_empty());
    fs::remove_dir_all(&root).ok();
}

// ----------------------------------------------------------------- privacy

#[test]
fn the_cache_holds_no_note_text_anywhere_in_its_bytes() {
    // §34. Not a review of the struct: a search of the file.
    let (root, space, _) = saved("notext", 4);
    let bytes = fs::read(file_of(&root, &space)).expect("read");
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        !text.contains("NOTE_TEXT_PRIVATE_456"),
        "the cache file carries a note's text"
    );
    for forbidden in ["snippet", "front_matter", "content", "Bearer", "api_key"] {
        assert!(
            !text.contains(forbidden),
            "the cache file carries `{forbidden}`"
        );
    }
    fs::remove_dir_all(&root).ok();
}

#[test]
fn the_cache_is_readable_only_by_its_owner() {
    // §37: a vector is derived from a private note, and derived is not
    // "not sensitive".
    let (root, space, _) = saved("modes", 2);
    let file = file_of(&root, &space);
    let mode = fs::symlink_metadata(&file)
        .expect("stat")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "the cache file is {mode:o}");
    let directory = cache::cache_directory(&root, &space);
    let directory_mode = fs::symlink_metadata(&directory)
        .expect("stat")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        directory_mode & 0o077,
        0,
        "the directory is {directory_mode:o}"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn the_cache_never_lives_inside_the_note_store() {
    let (root, space, _) = saved("location", 1);
    let file = file_of(&root, &space);
    let shown = file.to_string_lossy();
    for forbidden in ["/notes/", "/trash/", "/backups/"] {
        assert!(!shown.contains(forbidden), "{shown}");
    }
    assert!(shown.contains("semantic"));
    fs::remove_dir_all(&root).ok();
}

// -------------------------------------------------------------- corruption

fn expect_refusal(root: &Path, space: &EmbeddingSpaceId, expected: CacheError, what: &str) {
    match cache::load(root, space, CHUNKER_VERSION) {
        Err(actual) => assert_eq!(actual, expected, "{what}"),
        Ok(records) => panic!(
            "{what}: accepted a bad cache with {} records",
            records.len()
        ),
    }
}

#[test]
fn an_empty_file_is_refused() {
    let (root, space, _) = saved("emptyfile", 2);
    fs::write(file_of(&root, &space), b"").expect("truncate");
    expect_refusal(&root, &space, CacheError::Size, "empty file");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_truncated_file_is_refused() {
    for cut in [1usize, 8, 20, 64] {
        let (root, space, _) = saved(&format!("trunc{cut}"), 4);
        let file = file_of(&root, &space);
        let mut bytes = fs::read(&file).expect("read");
        let keep = bytes.len().saturating_sub(cut);
        bytes.truncate(keep);
        fs::write(&file, &bytes).expect("write");
        assert!(
            cache::load(&root, &space, CHUNKER_VERSION).is_err(),
            "a file cut by {cut} bytes was accepted"
        );
        fs::remove_dir_all(&root).ok();
    }
}

#[test]
fn trailing_garbage_is_refused() {
    let (root, space, _) = saved("garbage", 3);
    let file = file_of(&root, &space);
    let mut bytes = fs::read(&file).expect("read");
    bytes.extend_from_slice(b"e mais um pouco de lixo no fim");
    fs::write(&file, &bytes).expect("write");
    // The digest is over everything before the last 32 bytes, so appending
    // moves what is read as the digest: `Corrupt` is the honest name for it.
    expect_refusal(&root, &space, CacheError::Corrupt, "trailing garbage");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_wrong_magic_is_refused() {
    let (root, space, _) = saved("magic", 2);
    let file = file_of(&root, &space);
    let mut bytes = fs::read(&file).expect("read");
    bytes[0..8].copy_from_slice(b"OUTRACOI");
    fs::write(&file, &bytes).expect("write");
    expect_refusal(&root, &space, CacheError::NotOurs, "wrong magic");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_unknown_format_version_is_refused_rather_than_read_anyway() {
    let (root, space, _) = saved("version", 2);
    let file = file_of(&root, &space);
    let mut bytes = fs::read(&file).expect("read");
    bytes[8..12].copy_from_slice(&99u32.to_be_bytes());
    fs::write(&file, &bytes).expect("write");
    expect_refusal(&root, &space, CacheError::Version, "unknown version");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_flipped_byte_in_the_middle_is_caught_by_the_digest() {
    // What a size check alone cannot see, and the reason the format carries a
    // digest rather than only a length.
    let (root, space, _) = saved("flip", 4);
    let file = file_of(&root, &space);
    let mut bytes = fs::read(&file).expect("read");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xff;
    fs::write(&file, &bytes).expect("write");
    expect_refusal(&root, &space, CacheError::Corrupt, "a flipped byte");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_header_that_lies_about_its_length_is_refused() {
    let (root, space, _) = saved("headerlen", 3);
    let file = file_of(&root, &space);
    let mut bytes = fs::read(&file).expect("read");
    bytes[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
    fs::write(&file, &bytes).expect("write");
    expect_refusal(
        &root,
        &space,
        CacheError::Size,
        "a header length beyond the file",
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_cache_from_another_space_is_refused() {
    // The load is asked for `other_space`, whose directory has no file at all.
    let (root, _, _) = saved("space", 2);
    let other = other_space();
    expect_refusal(&root, &other, CacheError::Absent, "another space");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_cache_whose_header_names_another_space_is_refused_even_in_the_right_directory() {
    // Somebody moved a file, or a digest collided in somebody's imagination.
    let (root, space, records) = saved("crossspace", 2);
    let other = other_space();
    // Write a valid cache for `other`, then move its bytes into `space`'s path.
    let other_records: Vec<_> = records
        .iter()
        .enumerate()
        .map(|(index, _)| record(&other, &format!("outro {index}"), index as u32, 1.0))
        .collect();
    cache::save(&root, &other, CHUNKER_VERSION, &other_records).expect("save other");
    let stolen = fs::read(file_of(&root, &other)).expect("read");
    fs::write(file_of(&root, &space), &stolen).expect("write");
    expect_refusal(
        &root,
        &space,
        CacheError::Incompatible,
        "a file whose header names another space",
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_cache_from_another_chunker_is_refused() {
    let (root, space, _) = saved("chunker", 2);
    match cache::load(&root, &space, CHUNKER_VERSION + 1) {
        Err(CacheError::Incompatible) => {}
        other => panic!("a cache from another chunker was accepted: {other:?}"),
    }
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_non_finite_vector_in_the_body_is_refused() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let (root, space, _) = saved("nonfinite", 2);
        let file = file_of(&root, &space);
        let mut bytes = fs::read(&file).expect("read");
        // The body starts after magic, version, header length and header.
        let header_len = u32::from_be_bytes(bytes[12..16].try_into().expect("len")) as usize;
        let body_at = 16 + header_len;
        bytes[body_at..body_at + 4].copy_from_slice(&bad.to_le_bytes());
        // Re-sign, so the digest is not what fails: the numeric check must be.
        let digest_at = bytes.len() - 32;
        let digest = noteit_core::hashing::sha256_hex(&bytes[..digest_at]);
        let signed: Vec<u8> = (0..32)
            .map(|index| u8::from_str_radix(&digest[index * 2..index * 2 + 2], 16).expect("hex"))
            .collect();
        bytes[digest_at..].copy_from_slice(&signed);
        fs::write(&file, &bytes).expect("write");
        expect_refusal(&root, &space, CacheError::Vector, "a non-finite vector");
        fs::remove_dir_all(&root).ok();
    }
}

#[test]
fn a_symlink_at_the_cache_path_is_refused_and_not_followed() {
    let (root, space, _) = saved("symlink", 2);
    let file = file_of(&root, &space);
    let elsewhere = root.join("elsewhere.bin");
    fs::rename(&file, &elsewhere).expect("move");
    std::os::unix::fs::symlink(&elsewhere, &file).expect("symlink");
    expect_refusal(&root, &space, CacheError::Unreadable, "a symlink");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_directory_at_the_cache_path_is_refused() {
    let root = root("cachedir");
    let space = space();
    let file = file_of(&root, &space);
    fs::create_dir_all(&file).expect("mkdir");
    expect_refusal(&root, &space, CacheError::Unreadable, "a directory");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_unreadable_file_is_refused_rather_than_read_as_empty() {
    let (root, space, _) = saved("unreadable", 2);
    let file = file_of(&root, &space);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).expect("chmod");
    let outcome = cache::load(&root, &space, CHUNKER_VERSION);
    // Running as root would read it anyway, which is not a defect in the cache.
    if let Ok(records) = &outcome {
        assert!(!records.is_empty(), "a mode-000 file was read as empty");
    }
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).expect("chmod");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn an_absent_cache_is_absent_and_not_an_error_to_report() {
    let root = root("absent");
    expect_refusal(&root, &space(), CacheError::Absent, "no cache at all");
    fs::remove_dir_all(&root).ok();
}

// -------------------------------------------------------------- atomicity

#[test]
fn a_failed_save_leaves_the_previous_cache_whole() {
    // §36 and §79: the commit point is the rename, and before it the
    // stored file is untouched. Provoked by making the new content
    // unwritable — a record from another space — after a good one exists.
    let (root, space, records) = saved("atomic", 3);
    let before = fs::read(file_of(&root, &space)).expect("read");

    let foreign = record(&other_space(), "de outro espaço", 0, 9.0);
    let outcome = cache::save(&root, &space, CHUNKER_VERSION, &[foreign]);
    assert_eq!(outcome, Err(CacheError::Incompatible));

    let after = fs::read(file_of(&root, &space)).expect("read");
    assert_eq!(before, after, "a refused save changed the stored cache");
    assert_eq!(
        cache::load(&root, &space, CHUNKER_VERSION)
            .expect("load")
            .len(),
        records.len()
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn no_temporary_file_is_left_behind_by_a_successful_save() {
    let (root, space, _) = saved("notemp", 3);
    let directory = cache::cache_directory(&root, &space);
    let leftovers: Vec<String> = fs::read_dir(&directory)
        .expect("read_dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "index.bin")
        .collect();
    assert!(leftovers.is_empty(), "left behind {leftovers:?}");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn a_second_save_replaces_the_first_and_is_readable_immediately() {
    let (root, space, _) = saved("replace", 2);
    let replacement: Vec<_> = (0..5)
        .map(|index| record(&space, &format!("outro texto {index}"), index, 3.0))
        .collect();
    cache::save(&root, &space, CHUNKER_VERSION, &replacement).expect("save");
    assert_eq!(
        cache::load(&root, &space, CHUNKER_VERSION)
            .expect("load")
            .len(),
        5
    );
    fs::remove_dir_all(&root).ok();
}

// -------------------------------------------------------------- retention

#[test]
fn pruning_keeps_the_active_space_and_bounds_the_rest() {
    let root = root("prune");
    let active = space();
    let spaces: Vec<EmbeddingSpaceId> = [
        ("openai", "text-embedding-3-small"),
        ("voyage", "voyage-4"),
        ("gemini", "gemini-embedding-001"),
        ("voyage", "voyage-4-lite"),
    ]
    .into_iter()
    .map(|(provider, model)| {
        space::space_for(
            noteit_embed_protocol::ProviderId::parse(provider).expect("provider"),
            model,
            DIMENSION,
        )
    })
    .collect();
    for one in &spaces {
        let records = vec![record(one, "algum texto", 0, 1.0)];
        cache::save(&root, one, CHUNKER_VERSION, &records).expect("save");
    }
    assert_eq!(cache::save(&root, &active, CHUNKER_VERSION, &[]), Ok(()));
    cache::prune(&root, &active);

    let remaining = fs::read_dir(root.join("semantic"))
        .expect("read_dir")
        .flatten()
        .count();
    assert!(
        remaining <= cache::MAX_RETAINED_SPACES,
        "{remaining} spaces kept, ceiling is {}",
        cache::MAX_RETAINED_SPACES
    );
    // And the one in use survived.
    assert!(cache::load(&root, &active, CHUNKER_VERSION).is_ok());
    fs::remove_dir_all(&root).ok();
}

#[test]
fn clearing_a_space_is_the_rebuild_path_and_removes_no_note() {
    let (root, space, _) = saved("clear", 3);
    // A file standing in for a note, next to the cache root, to prove the
    // clear reaches only what it made.
    let bystander = root.join("uma-nota.md");
    fs::write(&bystander, b"esta nota nao pode sumir").expect("write");

    cache::clear(&root, &space).expect("clear");
    assert_eq!(
        cache::load(&root, &space, CHUNKER_VERSION),
        Err(CacheError::Absent)
    );
    assert!(bystander.is_file(), "clearing a cache removed a note");
    // And clearing again is not an error: rebuild has to be repeatable.
    cache::clear(&root, &space).expect("clear twice");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn pruning_never_removes_a_directory_it_did_not_make() {
    let root = root("prunealien");
    let active = space();
    cache::save(&root, &active, CHUNKER_VERSION, &[]).expect("save");
    let alien = root.join("semantic").join("nao-e-um-digest");
    fs::create_dir_all(&alien).expect("mkdir");
    fs::write(alien.join("importante.txt"), b"nao apague").expect("write");
    cache::prune(&root, &active);
    assert!(
        alien.join("importante.txt").is_file(),
        "prune removed a directory it did not create"
    );
    fs::remove_dir_all(&root).ok();
}
