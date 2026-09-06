//! Two SHA-256 implementations in one product, bound to each other.
//!
//! The artifact is verified with `ring` because 489 MiB at 200 MiB/s was the
//! one budget 4.3C could not meet, and `ring` is the only maintained
//! implementation measured faster than the Core's own. Everything else in
//! Note-it — `NoteRevision` above all — still hashes with
//! `noteit_core::hashing::sha256_hex`.
//!
//! Two implementations of a primitive is two sets of bugs unless something
//! makes them one, so this is that something: if they ever disagree on a byte,
//! the build fails here rather than the artifact being rejected on somebody's
//! machine with no explanation.

use noteit_core::hashing::sha256_hex;

/// `ring`, spelled the way `artifact.rs` spells it.
fn ring_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The published vectors, so that "they agree" is anchored to something
/// outside this repository.
#[test]
fn both_implementations_answer_the_published_vectors() {
    // FIPS 180-4 / NIST examples, and the empty string.
    for (input, expected) in [
        (
            "",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
    ] {
        assert_eq!(sha256_hex(input.as_bytes()), expected, "core, {input:?}");
        assert_eq!(ring_hex(input.as_bytes()), expected, "ring, {input:?}");
    }
}

/// Every length that changes how the padding lands, and then some.
///
/// One block, one block minus a byte, the two lengths where the length field
/// no longer fits and a second block appears, and a megabyte to leave the
/// small-input paths behind. The bytes are generated, not typed, so this
/// covers content no vector table would.
#[test]
fn both_implementations_agree_on_every_shape_of_input() {
    let mut lengths: Vec<usize> = (0..130).collect();
    lengths.extend([255, 256, 257, 1023, 1024, 4096, 65_536, 1_048_576]);

    // A cheap deterministic generator: no dependency, and the same bytes on
    // every machine, which is what makes a failure reproducible.
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut bytes = Vec::with_capacity(1_048_576);
    for _ in 0..1_048_576 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes.push((state >> 24) as u8);
    }

    for length in lengths {
        let slice = &bytes[..length];
        assert_eq!(
            sha256_hex(slice),
            ring_hex(slice),
            "the two implementations disagreed at {length} bytes"
        );
    }
}

/// The artifact itself, when this machine has one.
///
/// The two tests above are about a primitive; this one is about the file the
/// primitive was brought in for. It is `#[ignore]`d because it needs 489 MiB
/// that a clean checkout does not have — `scripts/fetch-embedding-artifact`
/// installs them — and it asserts the whole chain in one line: the accelerated
/// digest, the Core's digest, and the constant this build was written against
/// are the same sixty-four characters.
///
/// ```text
/// cargo test -p noteit-embedding-local --release --test digest_agreement -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs the provisioned artifact"]
fn both_implementations_agree_on_the_real_artifact() {
    let Some(directory) = noteit_embedding_local::artifact_directory(
        &noteit_embedding_local::POTION_MULTILINGUAL_128M,
    ) else {
        panic!("no cache directory to look in");
    };
    let weights = directory.join(noteit_embedding_local::artifact::WEIGHTS_FILE);
    let tokenizer = directory.join(noteit_embedding_local::artifact::TOKENIZER_FILE);
    assert!(
        weights.is_file() && tokenizer.is_file(),
        "the artifact is not provisioned at {}; run scripts/fetch-embedding-artifact",
        directory.display()
    );

    for (path, pinned) in [
        (
            &weights,
            noteit_embedding_local::POTION_MULTILINGUAL_128M.weights_sha256,
        ),
        (
            &tokenizer,
            noteit_embedding_local::POTION_MULTILINGUAL_128M.tokenizer_sha256,
        ),
    ] {
        let bytes = std::fs::read(path).expect("read");
        let core = sha256_hex(&bytes);
        let ring = ring_hex(&bytes);
        println!(
            "  {:>18}  {} bytes  {core}",
            path.file_name().expect("name").to_string_lossy(),
            bytes.len()
        );
        assert_eq!(core, ring, "the two implementations disagreed on {path:?}");
        assert_eq!(
            core, pinned,
            "the artifact at {path:?} is not the one this build pins"
        );
    }
}
