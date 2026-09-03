//! 4.1R1 / 4.1R1.1: what the running server actually holds open.
//!
//! The boundary script checks four things statically: that no network crate is
//! in the dependency graph, that Tokio is resolved without its `net` feature,
//! that this crate's source names no socket API at all, and that
//! `noteit-core` — which this crate calls into — names no *internet* API while
//! keeping the Unix socket its authority legitimately needs. All four describe
//! the program that was written. This suite watches the one that runs.
//!
//! ## What the 4.1R1.1 audit found, and what was done about it
//!
//! The first version took a snapshot of `/proc/<pid>/fd` before a call and
//! another after it, and said no internet socket existed "at any point of a
//! write". A socket opened and closed *inside* a handler is invisible to both
//! snapshots. It was reproduced — a `TcpListener` bound for 250ms inside a tool
//! handler — and all three tests passed.
//!
//! Observation is now continuous: a monitor thread samples the server's
//! descriptors for the whole duration of the operation, roughly every 13
//! microseconds on the machine this was written on.
//!
//! ## Exactly what this suite proves, and what it does not
//!
//! **Sound, and the primary assertion.** Whether the process holds *a socket at
//! all* is read straight off the `/proc/<pid>/fd` symlink, which cannot be
//! wrong about what kind of object a descriptor is. On the paths where the Core
//! needs no socket — every read, and a write taken directly with the lease free
//! — the assertion is that **no socket descriptor exists at any sample**. That
//! is complete for any socket living longer than one sample gap.
//!
//! **Best-effort, and deliberately not the load-bearing part.** Which *family*
//! a socket belongs to is looked up in the kernel's tables at first sight. Two
//! limits were measured rather than assumed, and both are why the family
//! result is a bonus detector and not the guarantee:
//!
//! - a socket created with `socket(AF_INET, …)` and never bound or connected
//!   does **not** appear in `/proc/net/tcp` — verified;
//! - a socket that closes between reading the descriptor and reading the table
//!   has already left it, so it reads back as unknown. The fail-closed retry
//!   loop produces exactly this, dozens of times, with legitimate Unix sockets.
//!
//! The classifier therefore has **no false positives** — if it says internet,
//! the inode was in an internet table — but it can miss. So "nothing was
//! identified as an internet socket" is asserted for what it is worth, and the
//! actual family guarantee rests on the static rule: the only socket API on the
//! whole MCP path is `std::os::unix::net::UnixStream`, in
//! `noteit-core/src/authority.rs`, and `scripts/check-mcp-boundary` refuses any
//! other in either crate. Any socket this process opens is that one, by
//! construction rather than by observation.
//!
//! ## Why the negative result is worth anything
//!
//! A watcher that sees nothing is indistinguishable from a watcher that is not
//! looking. So the write path is a **positive control**: when the store is held
//! by another instance, `noteit-core` opens a socket to hand the change over
//! and closes it inside the same MCP call — precisely the shape the old proof
//! could not see. The monitor is required to observe it. If it does not, the
//! test fails and the clean result beside it is not trusted.
//!
//! Linux only, deliberately. Note-it is a Wayland application with a
//! layer-shell dependency, its CI runs on Arch Linux, and procfs is how this
//! question is answered on the platform the project targets. On a system
//! without `/proc` the suite says so and fails rather than passing vacuously.

mod support;

use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{create_note, read_revision, AuthorityBehaviour, FakeAuthority, McpClient, Sandbox};

/// Whether this system can answer the question at all.
///
/// A check that quietly passes when it could not look is worse than no check,
/// so the absence of procfs is a failure with a sentence rather than a silent
/// success.
fn require_procfs() {
    assert!(
        std::path::Path::new("/proc/self/fd").is_dir(),
        "this suite reads /proc/<pid>/fd and this system has no procfs; \
         the no-socket property cannot be checked here and must not be assumed"
    );
}

// ------------------------------------------------------- classifying a socket

/// What family a socket descriptor could be shown to belong to.
///
/// A best-effort answer, and the type says so. See the module documentation:
/// the lookup has no false positives and can have false negatives, so
/// [`SocketFamily::Unknown`] means "the tables did not say", never "safe".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SocketFamily {
    /// Found in one of the kernel's TCP, UDP or raw tables. Forbidden here,
    /// and the one verdict that is certain when it is reached.
    Internet,
    /// Found in `/proc/net/unix`. What `noteit-core`'s authority uses.
    Unix,
    /// In none of the tables at the moment it was looked up — most often
    /// because it had already closed, which the fail-closed retry loop does
    /// dozens of times with perfectly legitimate Unix sockets.
    Unknown,
}

/// Every socket inode currently in the kernel's internet tables, both families.
///
/// System-wide, which is what makes it usable: if one of our descriptors is an
/// internet socket, its inode is in one of these files.
fn internet_socket_inodes() -> BTreeSet<String> {
    let mut inodes = BTreeSet::new();
    for table in ["tcp", "tcp6", "udp", "udp6", "raw", "raw6"] {
        let Ok(text) = std::fs::read_to_string(format!("/proc/net/{table}")) else {
            continue;
        };
        for line in text.lines().skip(1) {
            // The inode is the tenth whitespace-separated column in all of them.
            if let Some(inode) = line.split_whitespace().nth(9) {
                inodes.insert(inode.to_string());
            }
        }
    }
    inodes
}

/// Every socket inode currently in `/proc/net/unix`.
fn unix_socket_inodes() -> BTreeSet<String> {
    let mut inodes = BTreeSet::new();
    let Ok(text) = std::fs::read_to_string("/proc/net/unix") else {
        return inodes;
    };
    for line in text.lines().skip(1) {
        // `Num RefCount Protocol Flags Type St Inode Path` — the inode is the
        // seventh column.
        if let Some(inode) = line.split_whitespace().nth(6) {
            inodes.insert(inode.to_string());
        }
    }
    inodes
}

/// Classifies one socket inode, consulting the tables while it is still open.
///
/// The order matters: the internet tables are read first, so a socket that is
/// in them is never reported as anything else. Looked up at first sight rather
/// than at the end, because a closed socket has already left every table.
fn classify(inode: &str) -> SocketFamily {
    if internet_socket_inodes().contains(inode) {
        return SocketFamily::Internet;
    }
    if unix_socket_inodes().contains(inode) {
        return SocketFamily::Unix;
    }
    SocketFamily::Unknown
}

// ------------------------------------------------------------- the monitor

/// What a watch saw.
#[derive(Debug, Default)]
struct Sighting {
    /// Every socket inode observed, with the family it was classified as at
    /// the moment it was first seen — that is, while it was still open.
    sockets: BTreeMap<String, SocketFamily>,
    /// Every non-socket descriptor target seen, for the message on a failure.
    others: BTreeSet<String>,
}

/// Samples one process's open descriptors for as long as it is alive.
struct DescriptorWatch {
    stop: Arc<AtomicBool>,
    samples: Arc<AtomicU64>,
    seen: Arc<Mutex<Sighting>>,
    thread: Option<std::thread::JoinHandle<()>>,
    started: Instant,
}

impl DescriptorWatch {
    /// Starts watching, and does not return until the monitor has taken at
    /// least one sample — so an operation that begins immediately afterwards is
    /// genuinely covered from its first instant.
    fn start(pid: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let samples = Arc::new(AtomicU64::new(0));
        let seen = Arc::new(Mutex::new(Sighting::default()));

        let stop_thread = Arc::clone(&stop);
        let samples_thread = Arc::clone(&samples);
        let seen_thread = Arc::clone(&seen);

        let thread = std::thread::spawn(move || {
            let directory = format!("/proc/{pid}/fd");
            while !stop_thread.load(Ordering::Relaxed) {
                if let Ok(entries) = std::fs::read_dir(&directory) {
                    for entry in entries.flatten() {
                        let Ok(target) = std::fs::read_link(entry.path()) else {
                            continue;
                        };
                        let target = target.display().to_string();
                        let Some(inode) = target
                            .strip_prefix("socket:[")
                            .and_then(|rest| rest.strip_suffix(']'))
                        else {
                            seen_thread.lock().expect("watch").others.insert(target);
                            continue;
                        };
                        // Classified once, on first sight, while it is open.
                        // Doing it here rather than at the end is the whole
                        // point: a closed socket has already left the tables.
                        let mut sighting = seen_thread.lock().expect("watch");
                        if !sighting.sockets.contains_key(inode) {
                            let family = classify(inode);
                            sighting.sockets.insert(inode.to_string(), family);
                        }
                    }
                }
                samples_thread.fetch_add(1, Ordering::Relaxed);
            }
        });

        let watch = Self {
            stop,
            samples,
            seen,
            thread: Some(thread),
            started: Instant::now(),
        };
        while watch.samples.load(Ordering::Relaxed) == 0 {
            std::hint::spin_loop();
        }
        watch
    }

    /// Stops the monitor and answers with what it saw and how hard it looked.
    fn finish(mut self) -> Watched {
        let elapsed = self.started.elapsed();
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let samples = self.samples.load(Ordering::Relaxed);
        let seen = std::mem::take(&mut *self.seen.lock().expect("watch"));
        Watched {
            samples,
            elapsed,
            seen,
        }
    }
}

#[derive(Debug)]
struct Watched {
    samples: u64,
    elapsed: Duration,
    seen: Sighting,
}

impl Watched {
    fn family(&self, family: SocketFamily) -> Vec<&String> {
        self.seen
            .sockets
            .iter()
            .filter(|(_, kind)| **kind == family)
            .map(|(inode, _)| inode)
            .collect()
    }

    /// The gap between two consecutive looks, on average. Reported in failures
    /// so the number in this file's documentation can be checked rather than
    /// believed.
    fn sample_gap(&self) -> Duration {
        self.elapsed
            .checked_div(self.samples.max(1) as u32)
            .unwrap_or_default()
    }

    /// The claim this watch is allowed to support.
    fn assert_no_internet_socket(&self, what: &str) {
        let internet = self.family(SocketFamily::Internet);
        assert!(
            internet.is_empty(),
            "{what}: the server held internet socket(s) {internet:?} \
             ({} samples over {:?}, mean gap {:?}); all sockets seen: {:?}",
            self.samples,
            self.elapsed,
            self.sample_gap(),
            self.seen.sockets
        );
    }

    /// That a socket was seen at all — the instrument's own self-test.
    fn assert_saw_a_socket(&self, what: &str) {
        assert!(
            !self.seen.sockets.is_empty(),
            "{what}: the monitor saw no socket during an operation that provably \
             opens one ({} samples over {:?}, mean gap {:?}); the instrument \
             cannot be trusted to have seen an internet socket either",
            self.samples,
            self.elapsed,
            self.sample_gap()
        );
    }

    /// That the monitor really looked, and looked closely.
    ///
    /// The metric is the mean gap between two consecutive looks, not the number
    /// of samples: a short operation legitimately yields few samples, and a
    /// count threshold would either flake on fast machines or be meaninglessly
    /// low on slow ones. The gap is what decides how brief a socket can be and
    /// still be seen.
    ///
    /// The bound is deliberately loose against what was measured when this was
    /// written — 14µs sampling the descriptors alone, 76µs on the run where a
    /// socket appeared and had to be classified. A millisecond leaves more than
    /// a tenfold margin, so this guards against a monitor that has genuinely
    /// stopped working rather than against ordinary scheduling.
    fn assert_sampled_densely(&self) {
        const WORST_ACCEPTABLE_GAP: Duration = Duration::from_millis(1);
        assert!(
            self.samples >= 2,
            "the monitor took {} sample(s) in {:?}; there is no gap to speak of \
             and its negative result means nothing",
            self.samples,
            self.elapsed
        );
        assert!(
            self.sample_gap() <= WORST_ACCEPTABLE_GAP,
            "the monitor looked once every {:?} ({} samples over {:?}); that is \
             slower than the {WORST_ACCEPTABLE_GAP:?} this suite claims, so its \
             negative result cannot carry the claim",
            self.sample_gap(),
            self.samples,
            self.elapsed
        );
    }
}

/// Runs something while watching a process, and answers both results.
fn while_watching<T>(pid: u32, operation: impl FnOnce() -> T) -> (T, Watched) {
    let watch = DescriptorWatch::start(pid);
    let value = operation();
    (value, watch.finish())
}

// --------------------------------------------------------------------------

/// Serving the whole tool surface with the lease free, the process holds no
/// socket at any sample — the strongest claim in this file, and the one that
/// needs no family classification to be sound.
///
/// Every read, a creation and two writes taken directly. None of them needs to
/// hand anything to another instance, so none of them needs a socket, and the
/// monitor is asked to confirm that across the whole operation rather than at
/// its edges.
#[test]
fn the_server_serves_its_whole_surface_holding_no_socket_at_all() {
    require_procfs();
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);
    let pid = client.pid();

    let (id, watched) = while_watching(pid, || {
        client.list_tools();
        client.call("noteit_list", json!({}));
        client.call("noteit_search", json!({ "query": "qualquer" }));
        client.call("noteit_tasks_list", json!({}));
        client.call("noteit_trash_list", json!({}));
        let id = create_note(&mut client, "BASE");
        let revision = read_revision(&mut client, &id);
        client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": "MAIS", "expected_revision": revision }),
        );
        client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": "NÃO", "expected_revision": "0".repeat(64) }),
        );
        id
    });

    watched.assert_sampled_densely();
    watched.assert_no_internet_socket("serving the read and write surface");

    // Writing directly, with the lease free, the server needs no socket at all.
    assert!(
        watched.seen.sockets.is_empty(),
        "a direct write opened socket(s): {:?}",
        watched.seen.sockets
    );

    // And what it does hold is only the three streams a host gave it.
    let descriptors = client.open_descriptors();
    let numbers: Vec<u32> = descriptors.iter().map(|(number, _)| *number).collect();
    assert_eq!(
        numbers,
        vec![0, 1, 2],
        "the server holds descriptors beyond its standard streams: {descriptors:?}"
    );
    assert_eq!(sandbox.body(&id), "BASE\nMAIS");
}

/// The write that goes out through another instance: watched throughout, and
/// used to prove the watcher can see a transient socket in the first place.
///
/// The socket it sees usually classifies as unknown rather than as Unix, and
/// that is expected — it closes inside the same call, before the tables can be
/// read. What matters here is that it was *seen at all*, which is what makes
/// the "no internet socket" result beside it a measurement.
///
/// When the store is held, `noteit-core` opens a Unix socket, hands the change
/// over and closes it — all inside one MCP call. That is precisely the shape
/// the snapshot-based proof could not see, so requiring the monitor to observe
/// it turns "we saw no internet socket" from an absence of evidence into a
/// measurement by an instrument that is demonstrably working.
#[test]
fn a_write_through_the_authority_is_watched_and_shows_no_internet_socket() {
    require_procfs();
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);
    let pid = client.pid();
    let revision = read_revision(&mut client, &id);

    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::CommitForReal);

    let (answer, watched) = while_watching(pid, || {
        client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": "PELA AUTORIDADE", "expected_revision": revision }),
        )
    });

    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(authority.handled(), 1, "the write did not reach the holder");
    watched.assert_sampled_densely();

    // The positive control. Without it, a monitor that had silently stopped
    // working would report the same clean result as one that is watching. The
    // assertion is that a socket was seen *at all* — not that it classified as
    // Unix, because the classification is best-effort by design and a socket
    // that closes quickly reads back as unknown.
    watched.assert_saw_a_socket("a write handed to the authority");

    // And the claim itself, over the same continuously-watched window.
    watched.assert_no_internet_socket("a write handed to the authority");

    // Nothing survives the call: the Core closes the connection.
    let after: Vec<u32> = client
        .open_descriptors()
        .iter()
        .map(|(number, _)| *number)
        .collect();
    assert_eq!(
        after,
        vec![0, 1, 2],
        "a descriptor survived the write through the authority"
    );
    assert_eq!(sandbox.body(&id), "BASE\nPELA AUTORIDADE");
}

/// The fail-closed path, watched throughout: it opens sockets, and none of
/// them is an internet one.
///
/// Worth being exact about, because the obvious sentence — "the fail-closed
/// path opens nothing" — is false and was measured to be false. When the lease
/// is held and nobody is listening, `noteit-core` retries its Unix connection
/// for a bounded window, and every failed attempt is a real socket that lives
/// for microseconds. Dozens of them appear here. They are legitimate, they are
/// the Core's, and the static rule is what says they are AF_UNIX.
#[test]
fn a_fail_closed_refusal_opens_no_internet_socket() {
    require_procfs();
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);
    let pid = client.pid();
    let revision = read_revision(&mut client, &id);

    // The lease is held and nothing is listening: the shape of an instance that
    // died holding the store.
    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare");
    let lease = noteit_core::coordination::WriterLease::try_acquire_prepared(&coordination)
        .expect("prepare")
        .expect("take the lease");

    let (answer, watched) = while_watching(pid, || {
        client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": "NUNCA", "expected_revision": revision }),
        )
    });

    assert_eq!(
        answer.code(),
        Some("authority_unavailable"),
        "{}",
        answer.raw
    );
    watched.assert_sampled_densely();
    watched.assert_no_internet_socket("a fail-closed refusal");
    // The retry loop really did open sockets, so the clean verdict above is a
    // measurement and not an empty set.
    watched.assert_saw_a_socket("a fail-closed refusal");
    drop(lease);
}
