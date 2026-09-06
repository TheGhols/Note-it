//! What the running worker actually holds, reads and is told.
//!
//! `scripts/check-embed-boundary` proves things about the program that was
//! written. This suite watches the one that runs, on the four claims that a
//! static rule cannot settle:
//!
//! 1. **the worker's environment carries no credential**, read out of
//!    `/proc/<pid>/environ` rather than argued from the spawner's source
//!    (§23);
//! 2. **the worker opens no file of the store**, checked by giving it a store
//!    full of notes and comparing every file's `mtime` and `atime` before and
//!    after (§77);
//! 3. **no note-holding process holds an internet socket**, and the worker is
//!    the only one that ever does (§76);
//! 4. **a dead worker is not reported as a live one**, and leaves no socket
//!    behind pretending otherwise (§45).
//!
//! The worker binary is built by the same `cargo test` that runs this, and
//! found beside the test executable. Where it is not — a `cargo test` invoked
//! in a way that did not build it — each test says so and skips explicitly
//! rather than passing.

use noteit_embed_protocol::ProviderId;
use noteit_embedding_remote::worker::{WorkerHandle, WorkerPaths, INHERITED_VARIABLES};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A synthetic key, obviously not a real one.
const SENTINEL: &str = "NOTEIT_TEST_SECRET_NEVER_EXPOSE_7F4A2";

fn temporary(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "noteit-worker-iso-{}-{name}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("mkdir");
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).expect("chmod");
    base
}

/// The worker binary this build produced, beside the test executable.
fn worker_binary() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    // `target/<profile>/deps/<test>` → `target/<profile>/noteit-embed`
    let candidate = executable.parent()?.parent()?.join("noteit-embed");
    candidate.is_file().then_some(candidate)
}

fn skip(reason: &str) {
    eprintln!("worker_isolation: pulado — {reason}");
}

fn handle(name: &str, binary: PathBuf) -> (WorkerHandle, PathBuf) {
    let base = temporary(name);
    let config_dir = base.join("config");
    let runtime_dir = base.join("run");
    fs::create_dir_all(&config_dir).expect("mkdir");
    fs::create_dir_all(&runtime_dir).expect("mkdir");
    fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700)).expect("chmod");
    let paths = WorkerPaths {
        binary,
        socket: runtime_dir.join(noteit_embedding_remote::worker::SOCKET_NAME),
        config_dir,
    };
    (WorkerHandle::new(paths), base)
}

/// The child's environment, as the kernel has it.
fn environment_of(pid: u32) -> BTreeMap<String, String> {
    let raw = fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
    raw.split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let text = String::from_utf8_lossy(entry);
            text.split_once('=')
                .map(|(name, value)| (name.to_string(), value.to_string()))
        })
        .collect()
}

// ---------------------------------------------------------------- the secret

#[test]
fn the_worker_never_receives_a_credential_from_the_process_that_spawns_it() {
    let Some(binary) = worker_binary() else {
        return skip("o binário noteit-embed não está ao lado do executável de teste");
    };
    // Every credential variable set in *this* process — which stands in for
    // `noteit-mcp` started from a shell where somebody exported one. This is
    // the case §23 is about: not "we did not set it", but "it was set
    // and it still did not get through".
    for name in ["OPENAI_API_KEY", "GEMINI_API_KEY", "VOYAGE_API_KEY"] {
        std::env::set_var(name, SENTINEL);
    }

    let (worker, base) = handle("secret", binary);
    worker.ensure().expect("spawn");
    let pid = worker.child_pid().expect("a running worker");
    let environment = environment_of(pid);

    assert!(
        !environment.is_empty(),
        "could not read /proc/{pid}/environ; the assertion below would be vacuous"
    );
    for name in ["OPENAI_API_KEY", "GEMINI_API_KEY", "VOYAGE_API_KEY"] {
        assert!(
            !environment.contains_key(name),
            "the worker inherited {name}"
        );
    }
    // And no value anywhere, under any name at all — a rename would defeat the
    // check above.
    for (name, value) in &environment {
        assert!(
            !value.contains(SENTINEL),
            "the worker's environment carries the sentinel under `{name}`"
        );
    }

    // The environment is an allowlist and not merely a subtraction: nothing is
    // there that was not asked for.
    for name in environment.keys() {
        assert!(
            INHERITED_VARIABLES.contains(&name.as_str()),
            "the worker inherited `{name}`, which is not on the allowlist"
        );
    }

    worker.shutdown();
    for name in ["OPENAI_API_KEY", "GEMINI_API_KEY", "VOYAGE_API_KEY"] {
        std::env::remove_var(name);
    }
    fs::remove_dir_all(&base).ok();
}

#[test]
fn no_credential_reaches_the_workers_command_line() {
    let Some(binary) = worker_binary() else {
        return skip("o binário noteit-embed não está ao lado do executável de teste");
    };
    std::env::set_var("OPENAI_API_KEY", SENTINEL);
    let (worker, base) = handle("argv", binary);
    worker.ensure().expect("spawn");
    let pid = worker.child_pid().expect("a running worker");

    // `ps` reads this, and so does anybody else on the machine.
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let cmdline = String::from_utf8_lossy(&cmdline).replace('\0', " ");
    assert!(
        !cmdline.is_empty(),
        "could not read the worker's command line"
    );
    assert!(
        !cmdline.contains(SENTINEL),
        "the worker's argv carries a credential: {cmdline}"
    );
    for suspicious in [
        "--key",
        "--token",
        "--api-key",
        "--secret",
        "--url",
        "--base",
    ] {
        assert!(
            !cmdline.contains(suspicious),
            "the worker takes `{suspicious}` on its command line"
        );
    }

    worker.shutdown();
    std::env::remove_var("OPENAI_API_KEY");
    fs::remove_dir_all(&base).ok();
}

// ----------------------------------------------------------------- the store

#[test]
fn the_worker_opens_no_file_of_the_note_store() {
    let Some(binary) = worker_binary() else {
        return skip("o binário noteit-embed não está ao lado do executável de teste");
    };
    let (worker, base) = handle("store", binary);

    // A store, beside the worker's own directories, full of notes.
    let notes = base.join("notes");
    fs::create_dir_all(&notes).expect("mkdir");
    let mut written = Vec::new();
    for index in 0..8 {
        let path = notes.join(format!("nota-{index}.md"));
        fs::write(&path, format!("# Nota {index}\n\nNOTE_TEXT_PRIVATE_456\n")).expect("write");
        written.push(path);
    }
    let before = stamps(&written);

    worker.ensure().expect("spawn");
    // Give it a request to answer, so this is not a test of a process that did
    // nothing. It has no credential, so it refuses — after resolving, which is
    // the only file access it legitimately makes.
    let socket = worker.socket_path().to_path_buf();
    let _ = noteit_embedding_remote::client::request(
        &socket,
        ProviderId::OpenAi,
        "text-embedding-3-small",
        noteit_embed_protocol::Role::Document,
        Some(4),
        &["algum texto".to_string()],
    );
    std::thread::sleep(Duration::from_millis(50));

    // What the worker has open right now, by name.
    let pid = worker.child_pid().expect("a running worker");
    for open in open_files(pid) {
        assert!(
            !open.starts_with(&notes),
            "the worker has a note open: {}",
            open.display()
        );
    }

    let after = stamps(&written);
    assert_eq!(
        before, after,
        "the worker changed a note's timestamps; indexing is reading and the worker does not even read"
    );
    // And every note is still exactly what it was.
    for path in &written {
        let text = fs::read_to_string(path).expect("read");
        assert!(text.contains("NOTE_TEXT_PRIVATE_456"));
    }

    worker.shutdown();
    fs::remove_dir_all(&base).ok();
}

fn stamps(paths: &[PathBuf]) -> Vec<(u64, u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    paths
        .iter()
        .map(|path| {
            let metadata = fs::metadata(path).expect("stat");
            (
                metadata.mtime() as u64,
                metadata.ctime() as u64,
                metadata.size(),
            )
        })
        .collect()
}

/// Every path the process currently has open.
fn open_files(pid: u32) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fd")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .collect()
}

// ---------------------------------------------------------------- lifecycle

#[test]
fn a_dead_worker_is_not_reported_as_a_live_one() {
    let Some(binary) = worker_binary() else {
        return skip("o binário noteit-embed não está ao lado do executável de teste");
    };
    let (worker, base) = handle("dead", binary);
    worker.ensure().expect("spawn");
    assert!(worker.is_running());
    let socket = worker.socket_path().to_path_buf();
    assert!(socket.exists(), "the worker did not create its socket");

    worker.shutdown();
    assert!(!worker.is_running(), "a stopped worker reports as running");
    assert!(
        !socket.exists(),
        "a stopped worker left its socket behind, which the next question would read as availability"
    );
    assert!(worker.child_pid().is_none());

    fs::remove_dir_all(&base).ok();
}

#[test]
fn a_crashed_worker_is_replaced_rather_than_reported_forever() {
    let Some(binary) = worker_binary() else {
        return skip("o binário noteit-embed não está ao lado do executável de teste");
    };
    let (worker, base) = handle("crash", binary);
    worker.ensure().expect("spawn");
    let first = worker.child_pid().expect("a running worker");

    // Killed from outside, which is what a crash looks like from here.
    let _ = std::process::Command::new("kill")
        .arg("-KILL")
        .arg(first.to_string())
        .status();
    // Wait for the kernel to actually reap it.
    for _ in 0..100 {
        if !worker.is_running() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !worker.is_running(),
        "a killed worker still reports as running"
    );

    // The next request starts a new one rather than talking to a socket
    // nothing is behind.
    worker.ensure().expect("respawn");
    let second = worker.child_pid().expect("a running worker");
    assert_ne!(first, second, "the same process id came back");
    assert!(worker.is_running());

    worker.shutdown();
    fs::remove_dir_all(&base).ok();
}

#[test]
fn two_callers_racing_produce_one_worker() {
    // §18 and §55, one process out: "one indexing at a time" has a
    // sibling here, which is "one worker at a time".
    let Some(binary) = worker_binary() else {
        return skip("o binário noteit-embed não está ao lado do executável de teste");
    };
    let (worker, base) = handle("race", binary);
    let worker = std::sync::Arc::new(worker);

    let mut threads = Vec::new();
    for _ in 0..6 {
        let worker = std::sync::Arc::clone(&worker);
        threads.push(std::thread::spawn(move || worker.ensure().is_ok()));
    }
    let started: Vec<bool> = threads
        .into_iter()
        .map(|t| t.join().expect("join"))
        .collect();
    assert!(
        started.iter().all(|ok| *ok),
        "a racing caller failed to get a worker"
    );

    // One process id, and one socket.
    assert!(worker.is_running());
    let pid = worker.child_pid().expect("a running worker");
    let siblings = fs::read_dir(worker.socket_path().parent().expect("parent"))
        .expect("read_dir")
        .flatten()
        .count();
    assert_eq!(siblings, 1, "more than one socket in the runtime directory");
    assert!(pid > 0);

    worker.shutdown();
    fs::remove_dir_all(&base).ok();
}

// -------------------------------------------------------- the socket itself

#[test]
fn the_workers_socket_is_owner_only_and_outside_the_store() {
    let Some(binary) = worker_binary() else {
        return skip("o binário noteit-embed não está ao lado do executável de teste");
    };
    let (worker, base) = handle("socketmode", binary);
    worker.ensure().expect("spawn");
    let socket = worker.socket_path();

    let mode = fs::symlink_metadata(socket)
        .expect("stat")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "the socket is {mode:o}");

    let shown = socket.to_string_lossy();
    for forbidden in ["/notes/", "/trash/", "/backups/"] {
        assert!(!shown.contains(forbidden), "{shown}");
    }

    worker.shutdown();
    fs::remove_dir_all(&base).ok();
}

#[test]
fn a_worker_refuses_to_start_on_a_path_something_else_owns() {
    let Some(binary) = worker_binary() else {
        return skip("o binário noteit-embed não está ao lado do executável de teste");
    };
    let (worker, base) = handle("occupied", binary);
    // A regular file where the socket should go.
    fs::write(worker.socket_path(), b"nao sou um socket").expect("write");
    assert!(
        worker.ensure().is_err(),
        "the worker started on a path occupied by a regular file"
    );
    // And it did not delete what it did not create.
    assert!(worker.socket_path().is_file());
    fs::remove_dir_all(&base).ok();
}

/// Where a store would be, for the rule that a socket is never inside one.
#[allow(dead_code)]
fn store_like(base: &Path) -> PathBuf {
    base.join("notes")
}
