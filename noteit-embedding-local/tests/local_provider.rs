//! What `LocalProvider` promises, attacked rather than described.
//!
//! The properties here are the ones the rest of the system is allowed to
//! assume: that a document and a question land in the same space, that two
//! different artifacts never share one, that a changed byte changes the
//! identity, that a hostile file is a typed refusal, and that a provider has
//! no way to touch a note.

mod support;

use noteit_core::embedding::{
    cosine, ArtifactIdentity, ArtifactManifestV1, EmbeddingRole, SemanticError,
};
use noteit_core::semantic::EmbeddingProvider;
use noteit_embedding_local::artifact::artifact_availability;
use noteit_embedding_local::{
    ArtifactError, ArtifactExpectation, LocalProvider, EMBEDDING_RECIPE_VERSION,
    NORMALIZATION_VERSION,
};
use std::fs;
use std::path::Path;
use support::{safetensors, tokenizer_json, write_artifact};
use tempfile::tempdir;

const ROWS: usize = 8;
const DIMENSION: usize = 4;

fn digest(bytes: &[u8]) -> String {
    noteit_core::hashing::sha256_hex(bytes)
}

/// Builds an artifact and the expectation that matches it exactly.
///
/// Leaked on purpose: [`ArtifactExpectation`] holds `&'static str` because in
/// production it is a constant, and a test that wants a *different* artifact
/// needs strings that outlive the call. A handful of leaks in a test process
/// is not a leak in anything that runs.
fn artifact(
    directory: &Path,
    seed: u32,
    extra_token: Option<&str>,
    rows: usize,
    dimension: usize,
) -> ArtifactExpectation {
    let weights = safetensors("embeddings", rows, dimension, seed);
    let tokenizer = tokenizer_json(extra_token);
    write_artifact(directory, &weights, &tokenizer);
    ArtifactExpectation {
        model: "synthetic",
        revision: "test",
        dimension,
        rows,
        weights_sha256: Box::leak(digest(&weights).into_boxed_str()),
        tokenizer_sha256: Box::leak(digest(&tokenizer).into_boxed_str()),
    }
}

fn simple(directory: &Path) -> ArtifactExpectation {
    artifact(directory, 1, None, ROWS, DIMENSION)
}

// ------------------------------------------------- the space, and its edges

#[test]
fn a_document_and_a_question_land_in_the_same_space() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    let provider = LocalProvider::load(home.path(), &expectation).expect("load");

    let document = provider
        .embed_document(&["nota sobre chuva".to_string()])
        .expect("document");
    let query = provider.embed_query("chuva").expect("query");

    assert_eq!(
        document[0].space(),
        query.space(),
        "a search compares a question with documents; different spaces would refuse every search there is"
    );
    assert_eq!(document[0].space(), &provider.space());
    assert_eq!(document[0].role(), EmbeddingRole::Document);
    assert_eq!(query.role(), EmbeddingRole::Query);
    // And the comparison is therefore permitted.
    cosine(&document[0], &query).expect("same space compares");
}

#[test]
fn two_artifacts_of_the_same_shape_do_not_share_a_space() {
    let first_home = tempdir().expect("tempdir");
    let second_home = tempdir().expect("tempdir");
    let first = simple(first_home.path());
    let second = artifact(second_home.path(), 2, None, ROWS, DIMENSION);

    let one = LocalProvider::load(first_home.path(), &first).expect("first");
    let other = LocalProvider::load(second_home.path(), &second).expect("second");

    assert_eq!(
        one.space().dimension,
        other.space().dimension,
        "the point of this test is that the shapes agree"
    );
    assert_ne!(
        one.space(),
        other.space(),
        "different weights must be a different space"
    );

    let left = one.embed_query("chuva").expect("left");
    let right = other.embed_query("chuva").expect("right");
    assert_eq!(
        cosine(&left, &right),
        Err(SemanticError::SpaceMismatch),
        "equal dimension is not compatibility — 4.3A measured R@3 0.133 against 0.933 for exactly this"
    );
}

#[test]
fn a_changed_tokenizer_changes_the_identity() {
    let first_home = tempdir().expect("tempdir");
    let second_home = tempdir().expect("tempdir");
    let first = simple(first_home.path());
    // Same weights, same shape, same name — only the tokenizer differs.
    let second = artifact(second_home.path(), 1, Some("plantao"), ROWS, DIMENSION);
    assert_eq!(first.weights_sha256, second.weights_sha256);
    assert_ne!(first.tokenizer_sha256, second.tokenizer_sha256);

    let one = LocalProvider::load(first_home.path(), &first).expect("first");
    let other = LocalProvider::load(second_home.path(), &second).expect("second");
    assert_ne!(one.space().artifact, other.space().artifact);
    assert_ne!(one.space(), other.space());
}

#[test]
fn the_identity_is_computed_from_the_bytes_and_not_from_the_pin() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    let provider = LocalProvider::load(home.path(), &expectation).expect("load");

    let weights = fs::read(home.path().join("model.safetensors")).expect("weights");
    let tokenizer = fs::read(home.path().join("tokenizer.json")).expect("tokenizer");
    let expected = ArtifactManifestV1 {
        weights_sha256: digest(&weights),
        tokenizer_sha256: digest(&tokenizer),
        embedding_recipe_version: EMBEDDING_RECIPE_VERSION,
        normalization_version: NORMALIZATION_VERSION,
    }
    .identity()
    .expect("identity");

    assert_eq!(provider.space().artifact, expected);
    assert!(matches!(
        provider.space().artifact,
        ArtifactIdentity::LocalVerified(_)
    ));
    assert!(provider.space().artifact.is_verifiable());
}

#[test]
fn a_changed_recipe_or_normalisation_changes_the_identity() {
    let base = ArtifactManifestV1 {
        weights_sha256: "a".repeat(64),
        tokenizer_sha256: "b".repeat(64),
        embedding_recipe_version: EMBEDDING_RECIPE_VERSION,
        normalization_version: NORMALIZATION_VERSION,
    };
    let mut other_recipe = base.clone();
    other_recipe.embedding_recipe_version += 1;
    let mut other_normalisation = base.clone();
    other_normalisation.normalization_version += 1;

    assert_ne!(
        base.identity().expect("base"),
        other_recipe.identity().expect("recipe")
    );
    assert_ne!(
        base.identity().expect("base"),
        other_normalisation.identity().expect("normalisation")
    );
}

// ------------------------------------------------------- swapped artifacts

#[test]
fn weights_swapped_under_the_same_name_are_refused() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    LocalProvider::load(home.path(), &expectation).expect("the honest artifact loads");

    // Same file name, same length, different bytes — the exact substitution
    // that a name-based identity would never notice.
    let original = fs::read(home.path().join("model.safetensors")).expect("read");
    let replacement = safetensors("embeddings", ROWS, DIMENSION, 99);
    assert_eq!(original.len(), replacement.len(), "same size on purpose");
    assert_ne!(original, replacement);
    fs::write(home.path().join("model.safetensors"), &replacement).expect("write");

    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::Unexpected
    );
}

#[test]
fn weights_with_flipped_bytes_in_the_middle_are_refused() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    LocalProvider::load(home.path(), &expectation).expect("the honest artifact loads");

    let mut original = fs::read(home.path().join("model.safetensors")).expect("read");
    let mid = original.len() / 2;
    original[mid] ^= 0x55;
    fs::write(home.path().join("model.safetensors"), &original).expect("write");

    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::Unexpected
    );
}

#[test]
fn concurrent_threads_loading_the_same_artifact_simultaneously() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());

    let p1 = home.path().to_path_buf();
    let e1 = expectation.clone();
    let h1 = std::thread::spawn(move || LocalProvider::load(&p1, &e1).expect("thread 1 load"));

    let p2 = home.path().to_path_buf();
    let e2 = expectation;
    let h2 = std::thread::spawn(move || LocalProvider::load(&p2, &e2).expect("thread 2 load"));

    let provider1 = h1.join().expect("join 1");
    let provider2 = h2.join().expect("join 2");

    assert_eq!(provider1.space(), provider2.space());
    let v1 = provider1.embed_query("chuva").expect("embed 1");
    let v2 = provider2.embed_query("chuva").expect("embed 2");
    assert_eq!(v1.vector().components(), v2.vector().components());
}

#[test]
fn a_tokenizer_swapped_under_the_same_name_is_refused() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    fs::write(
        home.path().join("tokenizer.json"),
        tokenizer_json(Some("plantao")),
    )
    .expect("write");
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::Unexpected
    );
}

// ----------------------------------------------------------- hostile files

#[test]
fn an_absent_artifact_is_a_typed_state_and_not_a_fault() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    fs::remove_file(home.path().join("model.safetensors")).expect("remove");
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::Missing
    );

    let empty = tempdir().expect("tempdir");
    assert_eq!(
        LocalProvider::load(empty.path(), &expectation).unwrap_err(),
        ArtifactError::Missing
    );
    assert_eq!(
        LocalProvider::load(&empty.path().join("nowhere"), &expectation).unwrap_err(),
        ArtifactError::Missing
    );
}

#[test]
fn an_empty_file_is_refused_before_it_is_parsed() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    fs::write(home.path().join("model.safetensors"), b"").expect("write");
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::ImplausibleSize
    );
}

#[test]
fn a_truncated_file_is_refused() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    let weights = fs::read(home.path().join("model.safetensors")).expect("read");
    fs::write(
        home.path().join("model.safetensors"),
        &weights[..weights.len() / 2],
    )
    .expect("write");
    // Truncation changes the bytes, so the digest check catches it first. That
    // ordering is the point: nothing malformed is ever parsed.
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::Unexpected
    );
}

#[test]
fn a_directory_where_a_file_belongs_is_refused() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    fs::remove_file(home.path().join("model.safetensors")).expect("remove");
    fs::create_dir(home.path().join("model.safetensors")).expect("mkdir");
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::NotARegularFile
    );
}

#[test]
fn a_symlink_is_refused_rather_than_followed() {
    let home = tempdir().expect("tempdir");
    let elsewhere = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    let real = elsewhere.path().join("weights.bin");
    fs::rename(home.path().join("model.safetensors"), &real).expect("move");
    std::os::unix::fs::symlink(&real, home.path().join("model.safetensors")).expect("symlink");
    // The bytes behind the link are the honest ones; the link is still
    // refused, because hashing bytes only means something if the path cannot
    // point somewhere else between two runs.
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::NotARegularFile
    );
}

#[test]
fn an_unreadable_file_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    let path = home.path().join("model.safetensors");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&path, permissions).expect("chmod");
    let outcome = LocalProvider::load(home.path(), &expectation);
    // Running as root defeats the permission, and that is a property of the
    // session rather than of the code. The assertion is that it is refused or
    // it is the honest artifact — never a half-loaded provider.
    match outcome {
        Err(ArtifactError::Unreadable) => {}
        Ok(_) if nix_is_root() => {}
        other => panic!("an unreadable artifact produced {other:?}"),
    }
}

fn nix_is_root() -> bool {
    // Safe: `getuid` takes no arguments, touches no memory and cannot fail.
    unsafe { libc_getuid() == 0 }
}

extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}

#[test]
fn an_invalid_tokenizer_is_a_typed_refusal() {
    let home = tempdir().expect("tempdir");
    let broken = b"{ this is not a tokenizer".to_vec();
    let weights = safetensors("embeddings", ROWS, DIMENSION, 1);
    write_artifact(home.path(), &weights, &broken);
    let expectation = ArtifactExpectation {
        model: "synthetic",
        revision: "test",
        dimension: DIMENSION,
        rows: ROWS,
        weights_sha256: Box::leak(digest(&weights).into_boxed_str()),
        tokenizer_sha256: Box::leak(digest(&broken).into_boxed_str()),
    };
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::Malformed
    );
}

#[test]
fn invalid_weights_are_a_typed_refusal() {
    let home = tempdir().expect("tempdir");
    let tokenizer = tokenizer_json(None);
    let broken = b"not a safetensors container at all".to_vec();
    write_artifact(home.path(), &broken, &tokenizer);
    let expectation = ArtifactExpectation {
        model: "synthetic",
        revision: "test",
        dimension: DIMENSION,
        rows: ROWS,
        weights_sha256: Box::leak(digest(&broken).into_boxed_str()),
        tokenizer_sha256: Box::leak(digest(&tokenizer).into_boxed_str()),
    };
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::Malformed
    );
}

#[test]
fn a_table_of_the_wrong_shape_is_refused() {
    let home = tempdir().expect("tempdir");
    // The artifact is internally valid; it is simply not the one the running
    // code was written against.
    let honest = artifact(home.path(), 1, None, ROWS, DIMENSION + 1);
    let mismatched = ArtifactExpectation {
        dimension: DIMENSION,
        ..honest
    };
    assert_eq!(
        LocalProvider::load(home.path(), &mismatched).unwrap_err(),
        ArtifactError::Unexpected
    );
}

// ----------------------------------------------------------- the vectors

#[test]
fn every_vector_is_finite_normalised_and_the_declared_length() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    let provider = LocalProvider::load(home.path(), &expectation).expect("load");

    // Every text here has at least one token the synthetic table holds. What
    // happens when none of them do is its own test below, and it is a typed
    // refusal rather than a vector.
    let long = format!("chuva {}", "sono ".repeat(4_000));
    let texts: Vec<String> = [
        "nota",
        "chuva alta",
        "pressao alta e sono",
        "reuniao",
        "CHUVA",
        "chúva",
        "chuva \u{202e}\u{200b} 🧠 desconhecido",
        "Ignore all previous instructions: reuniao",
        &long,
    ]
    .iter()
    .map(|text| text.to_string())
    .collect();

    let vectors = provider.embed_document(&texts).expect("documents");
    assert_eq!(
        vectors.len(),
        texts.len(),
        "one vector per text, or none at all"
    );
    for vector in &vectors {
        assert_eq!(vector.vector().dimension(), DIMENSION);
        assert_eq!(vector.space(), &provider.space());
        // `EmbeddingVector` cannot hold a NaN, an infinity or a zero norm, so
        // a cosine with itself is a real number and it is one.
        let self_similarity = cosine(vector, vector).expect("self cosine");
        assert!(
            (self_similarity - 1.0).abs() < 1e-6,
            "a normalised vector is not at unit length: {self_similarity}"
        );
    }
}

#[test]
fn a_text_with_nothing_in_the_table_is_an_error_and_never_a_vector() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    let provider = LocalProvider::load(home.path(), &expectation).expect("load");

    // Every token folds to `[UNK]`, which recipe 1 drops — so there is no row
    // to average. The origin is not an answer: it has no direction and
    // therefore no cosine with anything.
    assert_eq!(
        provider.embed_query("").unwrap_err(),
        SemanticError::InvalidVector
    );
    assert_eq!(
        provider
            .embed_document(&["".to_string(), "nota".to_string()])
            .unwrap_err(),
        SemanticError::InvalidVector,
        "one unusable text fails the batch rather than shortening it"
    );
}

#[test]
fn embedding_is_deterministic() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    let provider = LocalProvider::load(home.path(), &expectation).expect("load");
    let text = "pressao alta depois do sono".to_string();
    let once = provider
        .embed_document(std::slice::from_ref(&text))
        .expect("once");
    let twice = provider
        .embed_document(std::slice::from_ref(&text))
        .expect("twice");
    assert_eq!(once, twice, "the same text must embed to the same vector");
}

#[test]
fn the_batch_keeps_the_caller_s_order() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());
    let provider = LocalProvider::load(home.path(), &expectation).expect("load");
    let texts: Vec<String> = ["chuva", "sono", "reuniao"]
        .iter()
        .map(|text| text.to_string())
        .collect();
    let batch = provider.embed_document(&texts).expect("batch");
    for (index, text) in texts.iter().enumerate() {
        let alone = provider
            .embed_document(std::slice::from_ref(text))
            .expect("alone");
        assert_eq!(
            batch[index], alone[0],
            "position {index} does not hold {text}"
        );
    }
}

// ------------------------------------------------- what a provider cannot do

#[test]
fn loading_and_embedding_write_nothing() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());

    let before = fingerprint(home.path());
    let provider = LocalProvider::load(home.path(), &expectation).expect("load");
    provider
        .embed_document(&["nota sobre chuva".to_string(), "sono".to_string()])
        .expect("documents");
    provider.embed_query("chuva").expect("query");
    let after = fingerprint(home.path());

    assert_eq!(
        before, after,
        "loading a model or embedding text changed the directory it read from"
    );
}

// ------------------------------------- what a diagnostic is allowed to claim

/// The cheap answer and the real one must not be able to disagree.
///
/// `noteit status` and the MCP session's report both say whether the artifact
/// is there, and both must answer by the loader's rule. The bug this pins was
/// real: they asked `Path::is_file`, which **follows** a symlink, while the
/// loader calls `symlink_metadata` and refuses one — so a symlinked artifact
/// was reported available and then rejected on the first question.
#[test]
fn the_availability_report_agrees_with_the_loader() {
    let home = tempdir().expect("tempdir");
    let elsewhere = tempdir().expect("tempdir");
    let expectation = simple(home.path());

    // A provisioned artifact: both say yes.
    assert!(
        artifact_availability(home.path()).is_ok(),
        "a provisioned artifact must be reported available"
    );
    LocalProvider::load(home.path(), &expectation).expect("load");

    // The weights gone: both say no, and for the same reason.
    let weights = home.path().join("model.safetensors");
    let kept = elsewhere.path().join("weights.bin");
    fs::rename(&weights, &kept).expect("move");
    assert_eq!(
        artifact_availability(home.path()).unwrap_err(),
        ArtifactError::Missing
    );
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::Missing
    );

    // The weights back, but as a symlink to the very same honest bytes. The
    // loader refuses the path, so the report has to refuse it too — this is
    // the case `Path::is_file` got wrong.
    std::os::unix::fs::symlink(&kept, &weights).expect("symlink");
    assert_eq!(
        artifact_availability(home.path()).unwrap_err(),
        ArtifactError::NotARegularFile,
        "a symlinked artifact must not be reported available: the loader refuses it"
    );
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::NotARegularFile
    );
}

/// The other shapes the loader refuses, refused by the report as well.
///
/// A directory where a file belongs, an empty file, and a tokenizer that is
/// there while the weights are not. None of these needs a byte to be read to
/// be answered, which is why the report is allowed to answer them.
#[test]
fn the_availability_report_refuses_every_shape_the_loader_refuses() {
    let home = tempdir().expect("tempdir");
    simple(home.path());

    let tokenizer = home.path().join("tokenizer.json");
    let saved = fs::read(&tokenizer).expect("read");

    fs::remove_file(&tokenizer).expect("remove");
    fs::create_dir(&tokenizer).expect("directory");
    assert_eq!(
        artifact_availability(home.path()).unwrap_err(),
        ArtifactError::NotARegularFile
    );

    fs::remove_dir(&tokenizer).expect("remove");
    fs::write(&tokenizer, b"").expect("empty");
    assert_eq!(
        artifact_availability(home.path()).unwrap_err(),
        ArtifactError::ImplausibleSize,
        "an empty file is refused by the loader, so it is not an available artifact"
    );

    fs::write(&tokenizer, &saved).expect("restore");
    assert!(artifact_availability(home.path()).is_ok());

    // One of the two is not both of them.
    fs::remove_file(home.path().join("model.safetensors")).expect("remove");
    assert_eq!(
        artifact_availability(home.path()).unwrap_err(),
        ArtifactError::Missing
    );
}

/// A symlink is refused whether or not it points at anything.
///
/// `symlink_metadata` answers about the link itself, so a broken link is a
/// non-regular file and not a missing one — and that distinction is the whole
/// value of not following it. The loader and the report say the same word.
#[test]
fn a_broken_symlink_is_refused_by_the_loader_and_by_the_report() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());

    let weights = home.path().join("model.safetensors");
    fs::remove_file(&weights).expect("remove");
    std::os::unix::fs::symlink(home.path().join("nada-aqui"), &weights).expect("symlink");

    assert_eq!(
        artifact_availability(home.path()).unwrap_err(),
        ArtifactError::NotARegularFile,
        "a broken symlink is a symlink, and a symlink is refused before its target is a question"
    );
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::NotARegularFile
    );
}

/// A symlink pointing at a directory, refused for being a symlink.
///
/// Worth pinning separately: had the rule been `metadata` plus "is it a
/// file?", this case would still be refused — but for the target's reason and
/// not the link's, and the symlink-to-a-regular-file case above would have
/// slipped through. One rule, one reason.
#[test]
fn a_symlink_to_a_directory_is_refused_by_the_loader_and_by_the_report() {
    let home = tempdir().expect("tempdir");
    let elsewhere = tempdir().expect("tempdir");
    let expectation = simple(home.path());

    let tokenizer = home.path().join("tokenizer.json");
    fs::remove_file(&tokenizer).expect("remove");
    std::os::unix::fs::symlink(elsewhere.path(), &tokenizer).expect("symlink");

    assert_eq!(
        artifact_availability(home.path()).unwrap_err(),
        ArtifactError::NotARegularFile
    );
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::NotARegularFile
    );
}

/// The ceiling, refused by both, and refused before anything is allocated.
///
/// The file is sparse — `set_len` and not half a gigabyte of writing — because
/// the rule is about the length the filesystem reports and both callers read
/// that length before they read a byte. That is the point of the ceiling: a
/// directory is configuration, and configuration does not get to decide how
/// much memory this process asks for.
#[test]
fn a_file_over_the_ceiling_is_refused_by_the_loader_and_by_the_report() {
    let home = tempdir().expect("tempdir");
    let expectation = simple(home.path());

    let tokenizer = home.path().join("tokenizer.json");
    let file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&tokenizer)
        .expect("open");
    file.set_len(noteit_embedding_local::artifact::MAX_TOKENIZER_BYTES + 1)
        .expect("grow");
    drop(file);

    assert_eq!(
        artifact_availability(home.path()).unwrap_err(),
        ArtifactError::ImplausibleSize
    );
    assert_eq!(
        LocalProvider::load(home.path(), &expectation).unwrap_err(),
        ArtifactError::ImplausibleSize
    );
}

/// Answering it costs no bytes.
///
/// The point of a separate report is that `noteit status` stays instant on a
/// machine with half a gigabyte of weights installed. Timing is a poor
/// assertion, so this checks the observable consequence instead: the files are
/// not touched, and the call is fast enough that reading 489 MiB could not
/// have happened.
#[test]
fn the_availability_report_reads_no_bytes() {
    let home = tempdir().expect("tempdir");
    simple(home.path());
    let before = fingerprint(home.path());

    let started = std::time::Instant::now();
    for _ in 0..1_000 {
        assert!(artifact_availability(home.path()).is_ok());
    }
    let elapsed = started.elapsed();

    assert_eq!(
        before,
        fingerprint(home.path()),
        "the report wrote something"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "a thousand availability reports took {elapsed:?}; this is a stat, not a read"
    );
}

/// Name, length and modification time of everything under a directory.
fn fingerprint(root: &Path) -> Vec<String> {
    let mut entries: Vec<String> = fs::read_dir(root)
        .expect("read_dir")
        .map(|entry| {
            let entry = entry.expect("entry");
            let metadata = entry.metadata().expect("metadata");
            format!(
                "{} {} {:?}",
                entry.file_name().to_string_lossy(),
                metadata.len(),
                metadata.modified().ok()
            )
        })
        .collect();
    entries.sort();
    entries
}
