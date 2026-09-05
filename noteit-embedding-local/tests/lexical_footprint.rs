//! What a process pays for *not* using the feature.
//!
//! Its own test binary, and that is the whole point: Cargo runs the tests of
//! one file in one process, so a measurement of "this process never loaded a
//! model" cannot share an address space with a test that loads one. The first
//! version of this lived beside the performance suite and measured the other
//! test's gigabyte, which is exactly the kind of number that would have been
//! quoted for years.
//!
//! Ignored by default, and run in release:
//!
//! ```text
//! cargo test -p noteit-embedding-local --release --test lexical_footprint -- --ignored --nocapture
//! ```

use noteit_core::context::{retrieve, ContextRequest, MAX_CANDIDATES};
use noteit_core::model::NoteDocument;
use noteit_core::{NoteItCore, StorageManager, StorePaths};
use tempfile::tempdir;

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1).map(str::to_string))
        })
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[test]
#[ignore = "measures process memory; run explicitly with --ignored in release"]
fn the_lexical_path_never_allocates_anything_model_sized() {
    let baseline = rss_kib();
    let tmp = tempdir().expect("tempdir");
    let paths = StorePaths::from_custom_paths(
        tmp.path().join("data/note-it/notes"),
        tmp.path().join("config/note-it"),
        tmp.path().join("state/note-it"),
        tmp.path().join("runtime/note-it"),
    );
    let core = NoteItCore::from_storage(StorageManager::from_paths(paths).expect("storage"));
    core.storage().ensure_directories().expect("dirs");
    for index in 0..1_000 {
        let mut document = NoteDocument::new_empty();
        document.content =
            format!("O paciente apresenta hipertensão arterial sistêmica.\n\nRegistro {index}.");
        core.storage().save_note_atomic(&document).expect("save");
    }
    let after_store = rss_kib();
    for _ in 0..20 {
        let answer = retrieve(
            &core,
            &ContextRequest {
                query: "pressão alta".to_string(),
                limit: Some(MAX_CANDIDATES),
                ..ContextRequest::default()
            },
        )
        .expect("lexical");
        std::hint::black_box(answer);
    }
    let after = rss_kib();
    println!(
        "\nRSS lexical-only, 1 000 notas, 20 consultas: {baseline} -> {after_store} -> {after} KiB (delta {} KiB)\n",
        after as i64 - baseline as i64
    );
    // The artifact is 489 MiB and the loaded model is over a gigabyte. Anything
    // approaching that here would mean the lexical path had reached one.
    assert!(
        after.saturating_sub(baseline) < 100_000,
        "the lexical path allocated {} KiB, which is model-sized",
        after - baseline
    );
}
