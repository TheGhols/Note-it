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
//! microseconds on the machine this was written on and idle. That number is
//! reported, never asserted — see `assert_watched_continuously`.
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
//! ## What 4.3C.R1 changed about how that control is run
//!
//! The control was right and the way it was run was a race. The socket it has
//! to catch lives for tens of microseconds inside a call of six to eighteen
//! milliseconds, and whether a polling thread lands inside it is luck that a
//! busy machine spends: under load it missed **four runs out of six**. The
//! suite's answer to "did the monitor look hard enough" — a mean sample gap
//! under a millisecond — was satisfied in every one of those failures, at
//! 43–122µs. Sampling density does not imply catching a transient, and the
//! millisecond was an incidental property of the machine this was written on
//! rather than a contract anything depends on.
//!
//! Two things were changed, and neither loosens a claim:
//!
//! - **The control is a rendezvous, not a race.** The fake authority holds the
//!   request inside the operation, so the Core's connection stays open until
//!   the monitor has been *observed to see it*. The socket is still one that
//!   opens and closes strictly inside a single MCP call — the shape that
//!   defeats a before/after snapshot — but whether it is seen is no longer a
//!   coin toss. A failure now means the instrument is blind, which is the only
//!   thing that assertion was ever meant to detect.
//! - **The density check asserts the worst gap, not the mean, and adds
//!   end-liveness.** A mean cannot bound a worst case and so could not detect
//!   the monitor stalling — the exact failure it was documented to guard
//!   against. The monitor now times its own passes, reports the longest
//!   silence, and must still be sampling when the operation returns.
//!
//! The other positive control — the fail-closed retry loop, which opens dozens
//! of sockets across its window rather than one — was measured under the same
//! load and did not fail in fifteen runs. It stays as it was.
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
use support::{
    create_note, read_revision, AuthorityBehaviour, FakeAuthority, Gate, McpClient, Sandbox,
};

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
    /// The longest interval between the starts of two consecutive passes,
    /// in nanoseconds, measured by the monitor about itself.
    ///
    /// This and not the mean, because the claim this suite makes is "a socket
    /// living longer than one sample gap cannot be missed" — a statement about
    /// the **worst** gap. A mean cannot bound it: a monitor that samples
    /// furiously and then stalls has an excellent mean and a blind spot.
    worst_gap: Arc<AtomicU64>,
    /// Set the first time any socket descriptor is seen, so a test can wait
    /// for the observation instead of hoping for it.
    saw_socket: Arc<AtomicBool>,
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
        let worst_gap = Arc::new(AtomicU64::new(0));
        let saw_socket = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(Mutex::new(Sighting::default()));

        let stop_thread = Arc::clone(&stop);
        let samples_thread = Arc::clone(&samples);
        let worst_thread = Arc::clone(&worst_gap);
        let saw_thread = Arc::clone(&saw_socket);
        let seen_thread = Arc::clone(&seen);

        let thread = std::thread::spawn(move || {
            let directory = format!("/proc/{pid}/fd");
            let mut previous = Instant::now();
            while !stop_thread.load(Ordering::Relaxed) {
                // Timed here, at the top of the pass, so the number is the
                // monitor's own cadence and not the test thread's view of it.
                let now = Instant::now();
                let gap = now.duration_since(previous).as_nanos() as u64;
                previous = now;
                worst_thread.fetch_max(gap, Ordering::Relaxed);
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
                        saw_thread.store(true, Ordering::Release);
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
            worst_gap,
            saw_socket,
            seen,
            thread: Some(thread),
            started: Instant::now(),
        };
        while watch.samples.load(Ordering::Relaxed) == 0 {
            std::hint::spin_loop();
        }
        watch
    }

    /// Whether the monitor has seen any socket yet.
    fn saw_a_socket(&self) -> bool {
        self.saw_socket.load(Ordering::Acquire)
    }

    /// Blocks until the monitor has seen a socket, or gives up.
    ///
    /// `false` means it timed out. This is what turns the instrument's
    /// self-test from a race into a rendezvous: the caller arranges for a
    /// socket to be **held open** inside the operation, waits here for the
    /// monitor to actually see it, and only then lets the operation finish.
    fn wait_for_a_socket(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while !self.saw_socket.load(Ordering::Acquire) {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::yield_now();
        }
        true
    }

    /// Stops the monitor and answers with what it saw and how hard it looked.
    ///
    /// Before stopping, it waits for one more pass to begin. That is the
    /// end-liveness check: a monitor that died halfway through the operation
    /// would otherwise hand back a clean, confident and worthless result, and
    /// no statistic over the samples it did take could tell the difference.
    fn finish(mut self) -> Watched {
        let mark = self.samples.load(Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut ended_alive = false;
        while Instant::now() < deadline {
            if self.samples.load(Ordering::Relaxed) > mark {
                ended_alive = true;
                break;
            }
            std::thread::yield_now();
        }
        let elapsed = self.started.elapsed();
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let samples = self.samples.load(Ordering::Relaxed);
        let worst_gap = Duration::from_nanos(self.worst_gap.load(Ordering::Relaxed));
        let seen = std::mem::take(&mut *self.seen.lock().expect("watch"));
        Watched {
            samples,
            elapsed,
            worst_gap,
            ended_alive,
            seen,
        }
    }
}

#[derive(Debug)]
struct Watched {
    samples: u64,
    elapsed: Duration,
    worst_gap: Duration,
    ended_alive: bool,
    seen: Sighting,
}

impl Watched {
    /// Whether any socket was observed while it was open.
    fn saw_a_socket(&self) -> bool {
        !self.seen.sockets.is_empty()
    }

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
             ({} samples over {:?}, mean gap {:?}, worst gap {:?}); all sockets \
             seen: {:?}",
            self.samples,
            self.elapsed,
            self.sample_gap(),
            self.worst_gap,
            self.seen.sockets
        );
    }

    /// That a socket was seen at all — the instrument's own self-test.
    fn assert_saw_a_socket(&self, what: &str) {
        assert!(
            !self.seen.sockets.is_empty(),
            "{what}: the monitor saw no socket during an operation that provably \
             opens one ({} samples over {:?}, mean gap {:?}, worst gap {:?}); \
             the instrument cannot be trusted to have seen an internet socket \
             either",
            self.samples,
            self.elapsed,
            self.sample_gap(),
            self.worst_gap
        );
    }

    /// That the monitor looked for the whole window, and never stalled inside
    /// it.
    ///
    /// ## What this replaced, and why
    ///
    /// This used to assert that the **mean** gap was under one millisecond.
    /// That bound was an incidental property of the machine this file was
    /// written on, and it was measured to be the wrong test on two counts.
    ///
    /// It bounds the wrong statistic. The claim in the module documentation —
    /// "complete for any socket living longer than one sample gap" — is about
    /// the **worst** gap. A mean cannot bound a worst case: a monitor that
    /// samples ten thousand times in the first millisecond and is then
    /// descheduled for the rest of the window reports an excellent mean and
    /// has a blind spot across almost the entire operation. The old assertion
    /// could not detect the one failure its own documentation named.
    ///
    /// And the bound was not sufficient for what it was taken to imply. Under
    /// load, runs were captured where the mean gap was **43–122µs** — eight to
    /// twenty times *better* than the millisecond — and the monitor still
    /// missed a socket that provably existed, because that socket lived for
    /// microseconds inside a window of ten. Sampling density and the certainty
    /// of catching a transient are not the same property, and no threshold on
    /// the first establishes the second. The instrument's sensitivity is
    /// established by rendezvous instead, in
    /// [`DescriptorWatch::wait_for_a_socket`], where a socket is held open
    /// until the monitor is seen to see it.
    ///
    /// ## What is asserted now
    ///
    /// Two things, both about the monitor rather than about the scheduler:
    /// that it was still running when the operation returned, and that it
    /// never went quiet for long enough to hide something inside the window.
    /// The gap ceiling is generous on purpose — it exists to catch a monitor
    /// that stopped, which is the failure that would make a clean result
    /// meaningless, and not to relitigate how much CPU a loaded machine gives
    /// a polling thread. The mean is reported and no longer asserted.
    fn assert_watched_continuously(&self, what: &str) {
        // Long enough that ordinary scheduling on a loaded machine cannot
        // reach it, short enough that a monitor which has actually stopped is
        // caught within one operation. It is a stall detector and says so.
        const LONGEST_SILENCE: Duration = Duration::from_millis(500);
        assert!(
            self.samples >= 2,
            "{what}: the monitor took {} sample(s) in {:?}; there is no gap to \
             speak of and its negative result means nothing",
            self.samples,
            self.elapsed
        );
        assert!(
            self.ended_alive,
            "{what}: the monitor never took another sample after the operation \
             returned ({} samples over {:?}); it stopped at some unknown point \
             and everything it did not see is unaccounted for",
            self.samples, self.elapsed
        );
        assert!(
            self.worst_gap <= LONGEST_SILENCE,
            "{what}: the monitor went {:?} without looking ({} samples over \
             {:?}, mean gap {:?}); it stalled rather than sampled, so anything \
             inside that silence is unobserved and the clean verdict cannot \
             stand",
            self.worst_gap,
            self.samples,
            self.elapsed,
            self.sample_gap()
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

    watched.assert_watched_continuously("serving the read and write surface");
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
/// When the store is held, `noteit-core` opens a Unix socket, hands the change
/// over and closes it — all inside one MCP call. That is precisely the shape
/// the snapshot-based proof could not see, so requiring the monitor to observe
/// it turns "we saw no internet socket" from an absence of evidence into a
/// measurement by an instrument that is demonstrably working.
///
/// ## Why the authority holds the line open
///
/// This was a race until 4.3C.R1, and it lost it. The version before this one
/// let the authority answer immediately and then asserted that the monitor had
/// happened to sample while the socket was alive — a socket that lives tens of
/// microseconds inside a call that takes six to eighteen milliseconds. On an
/// idle machine it worked; under load it failed **four runs out of six**, with
/// mean sample gaps of 43–122µs, far inside the millisecond the suite claimed
/// was enough. A positive control that fails when the machine is busy is not
/// evidence that the instrument works, and — worse in the other direction — a
/// control that passes by luck cannot fail when the instrument is genuinely
/// half-blind.
///
/// So the timing is no longer hoped for. `CommitWhenReleased` holds the
/// authority inside the operation: `arrived` opens with the server provably
/// blocked in the Core call and its connection open, and the write cannot
/// finish until this test says so. The test waits for the monitor to *actually
/// see* a socket and only then releases. The observation became a rendezvous,
/// the socket is still one that opens and closes strictly inside a single MCP
/// call, and nothing here is a duration anybody guessed.
#[test]
fn a_write_through_the_authority_is_watched_and_shows_no_internet_socket() {
    require_procfs();
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);
    let pid = client.pid();
    let revision = read_revision(&mut client, &id);

    let arrived = Gate::new();
    let release = Gate::new();
    let authority = FakeAuthority::start(
        &sandbox,
        AuthorityBehaviour::CommitWhenReleased {
            arrived: arrived.clone(),
            release: release.clone(),
        },
    );

    let watch = DescriptorWatch::start(pid);

    // In flight, and deliberately not waited on.
    let append = client.send_request(
        "tools/call",
        json!({
            "name": "noteit_append",
            "arguments": {
                "note_id": &id,
                "text": "PELA AUTORIDADE",
                "expected_revision": revision,
            },
        }),
    );

    assert!(
        arrived.wait_for(Duration::from_secs(30)),
        "the write never reached the authority, so this test never got to ask \
         its question"
    );

    // The positive control, and the whole reason this test is shaped like
    // this. The server is inside the Core call with its connection open; the
    // monitor is required to see a socket while it is open. Without this, a
    // monitor that had silently stopped working would report the same clean
    // result as one that is watching.
    let saw_it = watch.wait_for_a_socket(Duration::from_secs(10));

    // Only now may the write finish, and the socket close with it.
    release.open();
    let result = client
        .await_response(append)
        .expect("the held write must still complete");
    let watched = watch.finish();

    assert!(
        saw_it,
        "the monitor did not see the Core's connection in ten seconds, with the \
         socket held open the whole time ({} samples, worst gap {:?}); the \
         instrument cannot be trusted to have seen an internet socket either",
        watched.samples, watched.worst_gap
    );
    // And, said the other way, from what the watch actually collected.
    watched.assert_saw_a_socket("a write handed to the authority");

    assert_eq!(
        result["structuredContent"]["status"], "ok",
        "the write that waited must still commit: {result}"
    );
    assert_eq!(authority.handled(), 1, "the write did not reach the holder");
    watched.assert_watched_continuously("a write handed to the authority");

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
///
/// ## Why the refusal is repeated until it is seen
///
/// These sockets cannot be held open the way the authority's connection can:
/// they are failed `connect` calls, and nothing on either side can slow one
/// down. So the sighting is a sampling race like the one 4.3C.R1 removed from
/// the write path — and it loses the same way. Under 48-way load on eight
/// cores it was caught missing every socket of a whole refusal: 36 429 samples
/// over 3.0s, mean gap 83µs, longest silence 72.7ms, and not one of the
/// Core's microsecond sockets landed in a sample.
///
/// The answer is not a looser assertion, and not a longer window hoped to be
/// enough. The refusal is simply **repeated until the instrument has seen
/// one**, up to a bounded number of attempts. Each attempt is an independent,
/// complete fail-closed refusal over the real path, so the loop exercises the
/// property harder rather than less; on an unloaded machine the first attempt
/// ends it, and only a starved monitor ever pays for the rest. Failing after
/// every attempt means the monitor cannot see this path's sockets at all,
/// which is the finding the assertion was always for.
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

    // Bounded, and generous only in the case nobody normally reaches: the
    // first refusal ends the loop unless the monitor is being starved.
    const ATTEMPTS: usize = 5;
    let watch = DescriptorWatch::start(pid);
    let mut attempts = 0;
    let mut answer = None;
    while attempts < ATTEMPTS {
        attempts += 1;
        // A refusal changes nothing, so the same revision is still current.
        answer = Some(client.call(
            "noteit_append",
            json!({ "note_id": &id, "text": "NUNCA", "expected_revision": &revision }),
        ));
        if watch.saw_a_socket() {
            break;
        }
    }
    let watched = watch.finish();
    let answer = answer.expect("at least one refusal");

    assert_eq!(
        answer.code(),
        Some("authority_unavailable"),
        "{}",
        answer.raw
    );
    watched.assert_watched_continuously("a fail-closed refusal");
    watched.assert_no_internet_socket("a fail-closed refusal");
    // The retry loop really did open sockets, so the clean verdict above is a
    // measurement and not an empty set.
    assert!(
        watched.saw_a_socket(),
        "the monitor saw no socket across {attempts} complete fail-closed \
         refusals, each of which provably opens dozens ({} samples over {:?}, \
         mean gap {:?}, worst gap {:?}); the instrument cannot see this path at \
         all, and its clean verdict beside this is worthless",
        watched.samples,
        watched.elapsed,
        watched.sample_gap(),
        watched.worst_gap
    );
    drop(lease);
}
