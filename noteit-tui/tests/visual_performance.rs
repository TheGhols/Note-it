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

/// How much of each scenario to actually run.
///
/// A debug build measures nothing useful — the numbers in `docs/tui.md` come
/// from `--release` — and running megabyte scenarios unoptimised saturates
/// every core, which starves the PTY tests sharing the same `cargo test` run
/// of the timing they need. So debug runs a tenth of each size: enough to
/// prove the same shapes, small enough to leave the machine to the tests that
/// need a terminal to answer in time.
fn scale(bytes: usize) -> usize {
    if cfg!(debug_assertions) {
        bytes / 10
    } else {
        bytes
    }
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
        ("1 KB", realistic(scale(1_024))),
        ("100 KB", realistic(scale(100 * 1_024))),
        ("1 MB", realistic(scale(1_024 * 1_024))),
        (
            "20.000 linhas",
            "linha de texto comum\n".repeat(scale(20_000)),
        ),
        ("linha de 100.000", "a".repeat(scale(100_000))),
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
    let small = realistic(scale(100 * 1_024));
    let large = realistic(scale(1_000 * 1_024));
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
    let short = "a".repeat(scale(50_000));
    let long = "a".repeat(scale(500_000));

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
    let hostile = "<x a=\"".repeat(scale(2_000).max(200));
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

// ---------------------------------------------------------------------------
// Gate 5.0D.4B.P1 — the five forbidden failure modes, excluded by name
// ---------------------------------------------------------------------------
//
// §26.13, as corrected by §27.21/m5, forbids five shapes. B.3 may not begin
// while any of them is present, so each gets a test that would fail if it were.

#[test]
fn p1_history_has_a_byte_budget_and_a_floor() {
    use noteit_tui::draft::{Draft, HISTORY_BYTE_BUDGET};

    // Mode 4: "history sem budget". 200 snapshots of a 1 MB note would be
    // 200 MB, which §26.8 forbids outright.
    let big = "x".repeat(scale(200_000));
    let mut draft = Draft::new(&big);

    for index in 0..50 {
        draft.insert_char(char::from(b'a' + (index % 26) as u8));
        draft.finish_edit_group();
    }

    assert!(
        draft.history_bytes() <= HISTORY_BYTE_BUDGET,
        "history holds {} bytes, over the {HISTORY_BYTE_BUDGET} budget",
        draft.history_bytes()
    );

    // §27.10's floor: the live text is never evicted and the edit is never
    // refused, however small the budget is against the note.
    assert!(draft.text().len() >= big.len());
}

#[test]
fn p1_a_note_larger_than_the_whole_budget_still_edits() {
    use noteit_tui::draft::{Draft, HISTORY_BYTE_BUDGET};

    // One snapshot cannot fit. §27.10: history goes empty, undo becomes a
    // no-op, the edit proceeds, and nothing is discarded from the source.
    let enormous = "y".repeat(HISTORY_BYTE_BUDGET + 1_000);
    let mut draft = Draft::new(&enormous);
    let before = draft.text();

    draft.insert_char('z');
    assert_eq!(draft.text().len(), before.len() + 1, "the edit happened");
    assert_eq!(draft.history_depth(), 0, "no snapshot could be kept");
    assert!(!draft.undo(), "undo is an announced no-op, not a crash");
    assert_eq!(
        draft.text().len(),
        before.len() + 1,
        "and it changed nothing"
    );
}

#[test]
fn p1_the_undo_step_limit_still_holds_alongside_the_byte_budget() {
    use noteit_tui::draft::{Draft, UNDO_LIMIT};

    let mut draft = Draft::new("");
    for _ in 0..(UNDO_LIMIT + 50) {
        draft.insert_char('a');
        draft.finish_edit_group();
    }
    assert!(draft.history_depth() <= UNDO_LIMIT);
}

#[test]
fn p1_projection_has_no_viewport_parameter_at_all() {
    // Modes 2 and 5: "source map por célula terminal" and "reparse ligado à
    // viewport". Neither is expressible — `project` takes a source and a
    // generation, and a grapheme cell carries a width for layout but is keyed
    // by source range, never by a screen position.
    use noteit_tui::source_map::Generation;
    use noteit_tui::visual::VisualDocument;

    let source = "日本語 e texto normal\n\noutro bloco\n";
    let document = VisualDocument::project(source, Generation::first());

    // Cells are per grapheme, not per column: the CJK run is three cells of
    // width two, not six entries.
    let cells: Vec<_> = document.graphemes_of(document.blocks()[0].id).collect();
    assert_eq!(cells.iter().filter(|cell| cell.width == 2).count(), 3);
    assert!(
        cells.len() < source.len(),
        "one entry per grapheme, never one per column"
    );
}

#[test]
fn p1_nesting_is_bounded() {
    // Mode 5 in §27.21/m5: "nesting sem limite".
    use noteit_tui::projection::NESTING_BUDGET;
    assert_eq!(NESTING_BUDGET, 32);

    let deep = "<a>".repeat(NESTING_BUDGET + 20) + &"</a>".repeat(NESTING_BUDGET + 20);
    let elapsed = measure(&deep);
    assert!(
        elapsed < Duration::from_secs(5),
        "deep nesting took {elapsed:?}"
    );
}
