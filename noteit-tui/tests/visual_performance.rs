//! Gate 5.0D.4B.P0 — the projection baseline, and the failure modes it forbids.
//!
//! `docs/tui.md` §26.13 makes performance a recurring gate rather than a
//! deferred one, and P0 is its first instalment: the scenarios are measured
//! now, while projection is still non-interactive and a full reparse is
//! allowed, so that P1 has something to compare against before any key can
//! trigger one.
//!
//! ## Why the assertions are shapes, not stopwatch readings
//!
//! A wall-clock threshold is a machine's opinion. What the spec actually
//! forbids is a *shape* — `O(n²)` prefix rescanning, a map per terminal cell,
//! unbounded nesting — and a shape survives being run on a slower machine.
//! So the bounds here are ratios: doubling the input may not quadruple the
//! work. The absolute numbers are printed for the record and are reproduced in
//! `docs/tui.md`; the ratios are what fail the build.
//!
//! Run with `--nocapture` to see the table.

use noteit_tui::projection::project;
use noteit_tui::source_map::Generation;
use std::time::{Duration, Instant};

/// Projects `source` a few times and returns the fastest run.
///
/// The fastest rather than the mean: the slow runs are the machine doing
/// something else, and this is measuring the projector.
fn measure(source: &str) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..3 {
        let start = Instant::now();
        let projection = project(source, Generation::first());
        let elapsed = start.elapsed();
        // Consume the result so nothing is optimised away.
        assert_eq!(projection.source_len(), source.len());
        best = best.min(elapsed);
    }
    best
}

fn report(name: &str, source: &str, elapsed: Duration) {
    let projection = project(source, Generation::first());
    println!(
        "P0 {name:<28} {:>9} bytes  {:>8.2} ms  {:>7} lexemes  {:>6} nodes",
        source.len(),
        elapsed.as_secs_f64() * 1000.0,
        projection.lexemes().len(),
        projection.nodes().len()
    );
}

/// A note shaped like a real one, repeated to the requested size.
fn realistic(target: usize) -> String {
    const BLOCK: &str = concat!(
        "# Título da seção\n\n",
        "Um parágrafo com **negrito**, *ênfase*, `código` e uma [referência](https://example.com/a_(b)).\n\n",
        "- [ ] uma tarefa pendente\n",
        "- [x] uma tarefa concluída\n\n",
        "> [!NOTE]\n> um alerta com acentuação: ação, ç, ã.\n\n",
        "<span data-note-it-color=\"#DC2626\" style=\"color:#DC2626\">colorido</span> e texto normal.\n\n",
        "```rust\nfn main() { println!(\"olá\"); }\n```\n\n",
    );
    let mut source = String::with_capacity(target + BLOCK.len());
    while source.len() < target {
        source.push_str(BLOCK);
    }
    source
}

#[test]
fn p0_projection_baseline_is_recorded() {
    println!();
    for (name, source) in [
        ("1 KB", realistic(1_024)),
        ("100 KB", realistic(100 * 1_024)),
        ("1 MB", realistic(1_024 * 1_024)),
        ("20.000 linhas", "linha de texto comum\n".repeat(20_000)),
        ("linha de 100.000", "a".repeat(100_000)),
        ("nesting adversarial", "<a>".repeat(31) + &"</a>".repeat(31)),
        ("unicode denso", "👨‍👩‍👧‍👦ç日".repeat(20_000)),
    ] {
        let elapsed = measure(&source);
        report(name, &source, elapsed);

        // A generous absolute ceiling, present only to turn a pathological
        // regression into a failed build rather than a hung one.
        assert!(
            elapsed < Duration::from_secs(20),
            "{name} took {elapsed:?}, which is not a projection but a hang"
        );
    }
    println!();
}

#[test]
fn p0_projection_cost_grows_linearly_in_document_size() {
    // §26.13 forbids "prefix scan repetido O(n²)". Ten times the input may not
    // cost a hundred times the work; the allowance below is wide enough for
    // cache effects and narrow enough that quadratic cannot hide in it.
    let small = realistic(100 * 1_024);
    let large = realistic(1_000 * 1_024);
    let ratio_of_sizes = large.len() as f64 / small.len() as f64;

    let small_time = measure(&small).as_secs_f64();
    let large_time = measure(&large).as_secs_f64();
    let growth = large_time / small_time.max(f64::MIN_POSITIVE);

    println!(
        "P0 growth: {:.1}x bytes -> {:.1}x time ({:.2} ms -> {:.2} ms)",
        ratio_of_sizes,
        growth,
        small_time * 1000.0,
        large_time * 1000.0
    );

    // Measured at 8.9x for 10x on the P0 baseline. Twice that leaves room for a
    // slower machine and still fails long before quadratic, which would be 100x.
    assert!(
        growth < ratio_of_sizes * 2.0,
        "projection grew {growth:.1}x for {ratio_of_sizes:.1}x the bytes, which is superlinear"
    );
}

#[test]
fn p0_one_enormous_line_is_not_quadratic() {
    // The other half of the same prohibition: a single line of 100.000
    // characters must not be rescanned from its start for every construct.
    let short = "a".repeat(50_000);
    let long = "a".repeat(500_000);

    let short_time = measure(&short).as_secs_f64();
    let long_time = measure(&long).as_secs_f64();
    let growth = long_time / short_time.max(f64::MIN_POSITIVE);

    println!("P0 long line: 10.0x bytes -> {growth:.1}x time");
    // Measured at 9.6x for 10x.
    assert!(
        growth < 25.0,
        "one long line grew {growth:.1}x for 10x the bytes, which is quadratic"
    );
}

#[test]
fn p0_adversarial_html_does_not_explode() {
    // Many unclosed candidates in one document is the worst case for the
    // close-matching scan: each one searches to EOF. This is the input that
    // would expose an accidental O(n²) in that search.
    let hostile = "<x a=\"".repeat(2_000);
    let elapsed = measure(&hostile);
    println!(
        "P0 {:<28} {:>9} bytes  {:>8.2} ms",
        "html adversarial",
        hostile.len(),
        elapsed.as_secs_f64() * 1000.0
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "adversarial HTML took {elapsed:?}"
    );

    // And a deeply nested but well-formed document stays within the budget.
    let nested = "<a>".repeat(200) + &"</a>".repeat(200);
    assert!(measure(&nested) < Duration::from_secs(10));
}

#[test]
fn p0_projection_never_consults_a_viewport() {
    // The structural half of the §26.13 prohibitions: there is no width,
    // height or scroll parameter anywhere in the projector's surface, so a
    // "map per terminal cell" is not expressible. This test is the compile-time
    // statement of that, kept honest by the signature it calls.
    let source = realistic(4_096);
    let first = project(&source, Generation::first());
    let second = project(&source, Generation::first());
    assert_eq!(first.lexemes().len(), second.lexemes().len());
    assert_eq!(first.nodes().len(), second.nodes().len());
}
