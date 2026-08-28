use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

static PROCESS_START: OnceLock<Instant> = OnceLock::new();
static NEXT_LAYER_TOGGLE: AtomicU64 = AtomicU64::new(1);

pub fn enabled() -> bool {
    std::env::var_os("NOTE_IT_LAYER_DIAGNOSTICS").is_some()
}

pub fn log(args: fmt::Arguments<'_>) {
    if !enabled() {
        return;
    }

    let process_start = PROCESS_START.get_or_init(Instant::now);
    eprintln!(
        "NOTE_IT_LAYER_DIAG process_us={} {args}",
        process_start.elapsed().as_micros()
    );
}

#[derive(Debug, Clone, Copy)]
pub struct LayerToggleTrace {
    id: u64,
    started: Instant,
}

impl LayerToggleTrace {
    pub fn begin(source: &str) -> Self {
        let trace = Self {
            id: NEXT_LAYER_TOGGLE.fetch_add(1, Ordering::Relaxed),
            started: Instant::now(),
        };
        trace.phase("T0", format_args!("source={source} command=toggle-layer"));
        trace
    }

    pub fn phase(&self, phase: &str, details: fmt::Arguments<'_>) {
        log(format_args!(
            "toggle={} phase={phase} elapsed_us={} {details}",
            self.id,
            self.started.elapsed().as_micros()
        ));
    }
}
