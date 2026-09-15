//! What parsing a note's links costs, in release, on documents nobody wrote by
//! hand.
//!
//! Ignored by default, like the retrieval harness 5.0D.4B established. Run it
//! deliberately:
//!
//! ```text
//! cargo test -p noteit-core --release --test link_parser_performance -- --ignored --nocapture
//! ```
//!
//! **Release, and only release.** A debug build measures the absence of
//! optimisation, and quoting that as the cost of the parser would be quoting
//! the wrong thing.
//!
//! Every document is generated from a fixed seed, so the same run measures the
//! same work, and no note on this machine is read or can be read.
//!
//! ## What 6.A.2 measured
//!
//! The roadmap asks for the cost of a 64 KiB note. There is no pre-approved
//! numeric budget for it in ADR-065 or the roadmap, so this is recorded as the
//! baseline it is, and no ceiling is invented here to dress it up as a gate.
//! Measured on the development machine, release, 200 iterations after 20
//! warm-up passes, reported as the median of per-run times:
//!
//! ```text
//!      bytes      refs       median          MiB/s
//!      65541       217      0.934 ms             67
//! ```
//!
//! The scaling rows are the claim that matters more than any single number:
//! ADR-065 promises linear cost, and doubling a document should roughly double
//! its parse time rather than quadruple it.
//!
//! ```text
//!      bytes      refs       median     vs. 2x
//!       8227        27      0.205 ms          —
//!      16399        54      0.423 ms      1.04x
//!      32778       109      0.466 ms      0.55x
//!      65577       219      0.924 ms      0.99x
//!     131072       436      2.023 ms      1.10x
//!     262235       871      3.792 ms      0.94x
//! ```

use noteit_core::link::parse_references;
use std::time::{Duration, Instant};

/// xorshift64*: the same seed is the same document on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, ceiling: usize) -> usize {
        (self.next() % ceiling as u64) as usize
    }
}

const WORDS: [&str; 10] = [
    "paciente",
    "conduta",
    "revisar",
    "anotação",
    "pressão",
    "exame",
    "resumo",
    "consulta",
    "sintoma",
    "história",
];

/// A note of about `bytes` bytes that looks like one: prose, headings, lists,
/// a code fence, an HTML wrapper, a Markdown link, and roughly one wikilink
/// every few lines — so the measurement covers the structural scan and not
/// only the happy path.
fn document(bytes: usize, seed: u64) -> String {
    let mut rng = Rng(seed);
    let mut out = String::with_capacity(bytes + 256);
    let mut line = 0usize;
    while out.len() < bytes {
        match line % 12 {
            0 => out.push_str(&format!("## {}\n", WORDS[rng.below(WORDS.len())])),
            3 => out.push_str(&format!(
                "- item com [[{} {}]] e texto\n",
                WORDS[rng.below(WORDS.len())],
                rng.below(1000)
            )),
            5 => out.push_str("```\ncódigo [[não é link]]\n```\n"),
            7 => out.push_str(&format!(
                "<u>[[{}#seção|visível]]</u> e `[[literal]]`\n",
                WORDS[rng.below(WORDS.len())]
            )),
            9 => out.push_str("[veja](https://exemplo.com/a) e ![[outra^abc123]]\n"),
            _ => {
                for _ in 0..12 {
                    out.push_str(WORDS[rng.below(WORDS.len())]);
                    out.push(' ');
                }
                out.push('\n');
            }
        }
        line += 1;
    }
    out
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn measure(source: &str, iterations: usize) -> Duration {
    for _ in 0..20 {
        std::hint::black_box(parse_references(std::hint::black_box(source)));
    }
    let samples = (0..iterations)
        .map(|_| {
            let started = Instant::now();
            std::hint::black_box(parse_references(std::hint::black_box(source)));
            started.elapsed()
        })
        .collect();
    median(samples)
}

#[test]
#[ignore = "release-only measurement; run it deliberately"]
fn parsing_a_64_kib_note_is_measured_and_recorded() {
    let source = document(64 * 1024, 0x6A1D_5EED_C0FF_EE01);
    let references = parse_references(&source);
    let elapsed = measure(&source, 200);

    println!(
        "  {:>9} {:>9} {:>12} {:>14}",
        "bytes", "refs", "median", "MiB/s"
    );
    let throughput = source.len() as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0);
    println!(
        "  {:>9} {:>9} {:>10.3} ms {:>12.0}",
        source.len(),
        references.len(),
        elapsed.as_secs_f64() * 1000.0,
        throughput
    );

    assert!(
        !references.is_empty(),
        "a document with no references measures the wrong thing"
    );
}

#[test]
#[ignore = "release-only measurement; run it deliberately"]
fn the_cost_of_parsing_grows_with_the_document_and_not_with_its_square() {
    println!(
        "  {:>9} {:>9} {:>12} {:>10}",
        "bytes", "refs", "median", "vs. 2x"
    );
    let mut previous: Option<(usize, Duration)> = None;
    let mut worst = 0.0f64;

    for power in 4..=9 {
        let bytes = (8 * 1024) << (power - 4);
        let source = document(bytes, 0x0D15_EA5E_D00D_2024);
        let references = parse_references(&source).len();
        let elapsed = measure(&source, 40);

        let growth = previous.map(|(size, before)| {
            let scale = source.len() as f64 / size as f64;
            (elapsed.as_secs_f64() / before.as_secs_f64()) / scale
        });
        println!(
            "  {:>9} {:>9} {:>10.3} ms {:>10}",
            source.len(),
            references,
            elapsed.as_secs_f64() * 1000.0,
            growth.map_or("—".to_string(), |g| format!("{g:.2}x")),
        );
        if let Some(growth) = growth {
            worst = worst.max(growth);
        }
        previous = Some((source.len(), elapsed));
    }

    // Linear work keeps this near 1.0; quadratic work doubles it at every row.
    // The bound is loose because a shared runner is noisy, and still nowhere
    // near what a quadratic parser would produce over a 64x range.
    assert!(
        worst < 3.0,
        "cost per byte grew by {worst:.2}x as the document grew: that is not linear"
    );
}
