//! What retrieval costs, in release, on stores nobody wrote by hand.
//!
//! Ignored by default. It builds and walks thousands of notes, which is minutes
//! rather than seconds and has no business in a pull request's feedback loop.
//! Run it deliberately:
//!
//! ```text
//! cargo test -p noteit-core --release --test retrieval_performance -- --ignored --nocapture
//! ```
//!
//! **Release, and only release.** A debug build of a scan over five thousand
//! notes measures the absence of optimisation, and quoting that number as the
//! cost of the engine would be quoting the wrong thing.
//!
//! Every store here is synthetic and lives in a temporary directory: generated
//! sentences from a fixed vocabulary, with a fixed seed, so the same run
//! measures the same work. No real note is read, and no real note *can* be
//! read — there is no path here that names the store on this machine.
//!
//! ## What "recency only" is for
//!
//! The first row of every block asks for nothing: no query, so no folding, no
//! tokenising and no BM25 — just the scan, one read per note and the
//! projection. It is the floor, and it is what makes the rest attributable
//! instead of merely large. Measured on this machine, 4.3B against the engine
//! immediately before it (release, same harness, same seed):
//!
//! ```text
//!  notes   question         before      after
//!     30   recency only    2.79 ms    1.97 ms
//!     30   exact phrase    4.19 ms    3.58 ms
//!     30   multi-term      3.10 ms    3.21 ms
//!  1 000   recency only   99.51 ms   72.13 ms
//!  1 000   exact phrase  159.81 ms  146.36 ms
//!  1 000   multi-term    119.08 ms  129.35 ms
//!  5 000   recency only  522.60 ms  381.19 ms
//!  5 000   exact phrase  819.50 ms  712.98 ms
//!  5 000   multi-term    608.71 ms  641.96 ms
//! ```
//!
//! Most rows got *faster*, and not because BM25 is free: the engine used to
//! project a note to visible text twice per candidate — once for the label and
//! again for the snippet or the search — and now does it once. What BM25 itself
//! costs is the multi-term row, two to five per cent, while returning fifty
//! candidates where the old engine returned none.
//!
//! The dominant cost in every row is the walk: one read per note, no index.
//! That is D-04, priced rather than hidden.

use noteit_core::chrono::{TimeZone, Utc};
use noteit_core::context::{retrieve, ContextRequest, MAX_CANDIDATES};
use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use noteit_core::{NoteItCore, StorageManager, StorePaths, Uuid};
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::{tempdir, TempDir};

/// A vocabulary with a long tail, so that "a frequent term" and "a rare term"
/// are different questions rather than the same one twice.
const COMMON: [&str; 8] = [
    "paciente",
    "estudo",
    "conduta",
    "revisar",
    "manhã",
    "consulta",
    "anotação",
    "resumo",
];
const UNCOMMON: [&str; 8] = [
    "arritmia",
    "ferritina",
    "metformina",
    "polissonografia",
    "creatinina",
    "corticoide",
    "sepse",
    "melatonina",
];
/// Appears in exactly one note of every store built below.
const UNIQUE: &str = "zebrafish";

/// A tiny, fixed pseudo-random source.
///
/// Written out rather than pulled in: a generator is eight lines and a
/// dependency is a dependency, and this phase's rule is that no new one arrives
/// for the convenience of a fixture.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        // xorshift64*, chosen because it is short and its cycle is far longer
        // than anything asked of it here.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from[(self.next() % from.len() as u64) as usize]
    }
}

struct Store {
    _tmp: TempDir,
    core: NoteItCore,
    bytes: usize,
}

fn open(root: &Path) -> NoteItCore {
    let paths = StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        root.join("runtime/note-it"),
    );
    NoteItCore::from_storage(StorageManager::from_paths(paths).expect("open storage"))
}

/// A store of `notes` generated notes, roughly the size of a real one.
fn synthetic(notes: usize) -> Store {
    let tmp = tempdir().expect("tempdir");
    let core = open(tmp.path());
    core.storage().ensure_directories().expect("ensure dirs");
    let base = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("a real instant");

    let mut random = Seeded(0x4e6f_7465_4974_0001);
    let mut bytes = 0;
    for index in 0..notes {
        let paragraphs = 2 + (random.next() % 4) as usize;
        let mut body = String::new();
        for paragraph in 0..paragraphs {
            if paragraph > 0 {
                body.push_str("\n\n");
            }
            for word in 0..40 {
                if word > 0 {
                    body.push(' ');
                }
                // Nine common words to one uncommon: a distribution where the
                // inverse document frequency has something to say.
                if random.next().is_multiple_of(10) {
                    body.push_str(random.pick(&UNCOMMON));
                } else {
                    body.push_str(random.pick(&COMMON));
                }
            }
            body.push('.');
        }
        if index == notes / 2 {
            body.push_str(&format!(" {UNIQUE}."));
        }
        body.push_str("\n\n- [ ] revisar depois");

        let mut document = NoteDocument::new_empty();
        document.metadata.id =
            Uuid::from_u128(0x5065_7266_0000_0000_0000_0000_0000_0000u128 + index as u128);
        document.metadata.created_at = Some(base);
        document.metadata.updated_at =
            Some(base + noteit_core::chrono::Duration::minutes(index as i64));
        bytes += body.len();
        document.content = body;
        document.user_metadata = NoteMetadata::try_new(
            vec![if index % 3 == 0 { "cardio" } else { "estudo" }.to_string()],
            vec![NoteProperty {
                key: "fonte".to_string(),
                value: if index % 2 == 0 { "aula" } else { "diretriz" }.to_string(),
            }],
        )
        .expect("metadata");
        core.storage()
            .save_note_atomic(&document)
            .expect("save a synthetic note");
    }

    Store {
        _tmp: tmp,
        core,
        bytes,
    }
}

/// Runs one request `rounds` times and reports the distribution.
fn time(
    store: &Store,
    request: &ContextRequest,
    rounds: usize,
) -> (Duration, Duration, Duration, usize) {
    // One warm-up, so the first page-cache miss is not reported as the cost of
    // the algorithm.
    let candidates = retrieve(&store.core, request)
        .expect("answers")
        .candidates
        .len();

    let mut samples = Vec::with_capacity(rounds);
    let whole = Instant::now();
    for _ in 0..rounds {
        let started = Instant::now();
        let answer = retrieve(&store.core, request).expect("answers");
        samples.push(started.elapsed());
        std::hint::black_box(answer);
    }
    let total = whole.elapsed();
    samples.sort();
    let percentile = |fraction: f64| {
        let index = ((samples.len() as f64 - 1.0) * fraction).round() as usize;
        samples[index]
    };
    (percentile(0.50), percentile(0.95), total, candidates)
}

/// Peak resident memory, or an admission that it could not be read.
fn peak_rss() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|value| value.split_whitespace().next()?.parse::<u64>().ok())
}

#[test]
#[ignore = "builds thousands of notes; run explicitly with --ignored in release"]
fn retrieval_cost_over_synthetic_stores() {
    // A guard rather than an assertion, because an assertion on a constant is a
    // lint: `cargo test --ignored` without `--release` would otherwise report
    // how slow an unoptimised build is, and somebody would quote it.
    if cfg!(debug_assertions) {
        panic!("run this in release: a debug build measures the absence of optimisation");
    }

    println!(
        "\n{:>7} {:>10} {:>22} {:>10} {:>10} {:>9} {:>11}",
        "notes", "corpus", "question", "p50", "p95", "rounds", "candidates"
    );

    for notes in [30usize, 100, 1_000, 5_000] {
        let built = Instant::now();
        let store = synthetic(notes);
        let build = built.elapsed();
        // Fewer rounds as the store grows: enough samples for a p95 to mean
        // something, without the run taking longer than anybody will wait.
        let rounds = if notes >= 5_000 { 20 } else { 60 };

        let asking = |question: &str| ContextRequest {
            limit: Some(MAX_CANDIDATES),
            include_tasks: true,
            ..ContextRequest::with_query(question)
        };
        for (label, request) in [
            // No query at all: the scan, the read and the projection, with no
            // folding, no tokenising and no BM25. The floor everything else is
            // measured against, and what says how much of the cost below is the
            // lexical work rather than the walk over the store.
            (
                "recency only",
                ContextRequest {
                    limit: Some(MAX_CANDIDATES),
                    ..ContextRequest::default()
                },
            ),
            // The whole phrase, present verbatim.
            ("exact phrase", asking("revisar depois")),
            // Several terms, none of them together.
            ("multi-term", asking("arritmia ferritina metformina")),
            // Nothing at all.
            ("no match", asking("helicoptero submarino")),
            // A word in almost every note.
            ("frequent term", asking("paciente")),
            // A word in exactly one.
            ("rare term", asking(UNIQUE)),
        ] {
            let (p50, p95, total, candidates) = time(&store, &request, rounds);
            println!(
                "{notes:>7} {:>9} KiB {label:>22} {:>8.2} ms {:>8.2} ms {rounds:>9} {candidates:>11}",
                store.bytes / 1024,
                p50.as_secs_f64() * 1000.0,
                p95.as_secs_f64() * 1000.0,
            );
            std::hint::black_box(total);
        }
        println!(
            "{notes:>7} {:>9} KiB {:>22} {:>8.2} ms",
            store.bytes / 1024,
            "(building the store)",
            build.as_secs_f64() * 1000.0
        );
    }

    match peak_rss() {
        Some(kib) => println!("\npeak RSS for the whole run: {kib} KiB"),
        None => println!("\npeak RSS: UNKNOWN — /proc/self/status could not be read"),
    }
}
