//! A Note-it that is running is a Note-it that owns the store.
//!
//! Phase 4.0E made the desktop instance the store's writer, and left one gap
//! open: when the lease could not be taken, the application started anyway and
//! merely declined to be the authority. That instance still had windows, still
//! autosaved, and still wrote notes — a second writer, which is the exact
//! failure the lease exists to prevent, reached by the code that was supposed
//! to prevent it.
//!
//! It now refuses to start. This test proves it against the real binary and a
//! real held lease, because the interesting part is what the *process* does:
//! a unit test could only assert about a function, and the claim being made
//! here is that nothing gets as far as opening a note.
//!
//! Everything runs against a throwaway store on a private bus. A display is
//! needed to start GTK at all, so where there is none — CI — the test says it
//! skipped rather than passing quietly.

use noteit_core::coordination::{WriteCoordinationPaths, WriterLease};
use noteit_core::storage::StorePaths;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::{tempdir, TempDir};

/// How long the binary is given to refuse. Comfortably past its own bounded
/// wait for a busy store, so a timeout here means it did not refuse at all.
const REFUSAL_TIMEOUT: Duration = Duration::from_secs(20);

struct Sandbox {
    _tmp: TempDir,
    root: PathBuf,
    _bus: PrivateBus,
    bus_address: String,
}

/// A `dbus-daemon` of this test's own.
///
/// Without it the binary would find the real session bus, discover that the
/// user's own Note-it owns the well-known name, forward its command line and
/// exit — never reaching the startup path under test. That is the Phase 3.7
/// incident, and it is the reason the harness exists.
struct PrivateBus {
    pid: Option<u32>,
    socket_dir: PathBuf,
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
        let _ = std::fs::remove_dir_all(&self.socket_dir);
    }
}

/// `$XDG_RUNTIME_DIR/note-it`, resolved the way the application resolves it.
fn real_runtime_root() -> PathBuf {
    StorePaths::resolve().runtime_dir
}

fn note_it_binary() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for profile in ["debug", "release"] {
        let candidate = root.join("target").join(profile).join("note-it");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn display_available() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some()
}

fn start_private_bus() -> Option<(PrivateBus, String)> {
    let socket_dir =
        std::env::temp_dir().join(format!("note-it-failclosed-{}", std::process::id()));
    std::fs::create_dir_all(&socket_dir).ok()?;

    let output = Command::new("dbus-daemon")
        .arg("--session")
        .arg(format!("--address=unix:dir={}", socket_dir.display()))
        .arg("--fork")
        .arg("--print-address")
        .arg("--print-pid")
        .output()
        .ok()?;
    if !output.status.success() {
        let _ = std::fs::remove_dir_all(&socket_dir);
        return None;
    }

    let mut address = None;
    let mut pid = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.starts_with("unix:") {
            address = Some(line.to_string());
        } else if let Ok(parsed) = line.trim().parse::<u32>() {
            pid = Some(parsed);
        }
    }

    let address = address?;
    Some((PrivateBus { pid, socket_dir }, address))
}

impl Sandbox {
    fn new() -> Option<Self> {
        let tmp = tempdir().ok()?;
        let root = tmp.path().to_path_buf();
        for name in ["data", "config", "state", "cache", "runtime"] {
            std::fs::create_dir_all(root.join(name)).ok()?;
        }
        let (bus, bus_address) = start_private_bus()?;
        Some(Self {
            _tmp: tmp,
            root,
            _bus: bus,
            bus_address,
        })
    }

    /// The paths the spawned binary will resolve, runtime directory included.
    ///
    /// `XDG_RUNTIME_DIR` is deliberately *not* overridden for the child:
    /// `WAYLAND_DISPLAY` resolves inside it, so a private one would leave the
    /// process unable to open a display and it would fail for a reason that has
    /// nothing to do with what is under test. Coordination stays separate the
    /// way it always does — by store key — so the lease this test holds is the
    /// synthetic store's, never the real one's. It is removed on the way out.
    fn store_paths(&self) -> StorePaths {
        StorePaths::from_custom_paths(
            self.root.join("data/note-it/notes"),
            self.root.join("config/note-it"),
            self.root.join("state/note-it"),
            real_runtime_root(),
        )
    }

    /// Removes this store's coordination directory from the real runtime tree.
    ///
    /// A test may not leave anything behind, and the directory belongs to a
    /// store that only ever existed for this test.
    fn clean_runtime(&self) {
        // Either kind: a test that deliberately put a plain file where the
        // directory belongs leaves a file, and `remove_dir_all` will not take
        // one. Both are debris and both go.
        let path = self.coordination().store_dir().to_path_buf();
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_file(&path);
    }

    fn coordination(&self) -> WriteCoordinationPaths {
        WriteCoordinationPaths::for_store(&self.store_paths())
    }

    fn spawn_note_it(&self, binary: &Path, args: &[&str]) -> Child {
        let mut command = Command::new(binary);
        command
            .args(args)
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("DBUS_SESSION_BUS_ADDRESS", &self.bus_address)
            // Left as the session's own: see `store_paths`.
            .env_remove("DBUS_STARTER_ADDRESS")
            .env_remove("DBUS_STARTER_BUS_TYPE")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.spawn().expect("spawn note-it")
    }

    fn notes_dir(&self) -> PathBuf {
        self.root.join("data/note-it/notes")
    }

    fn note_count(&self) -> usize {
        std::fs::read_dir(self.notes_dir())
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("md"))
                    .count()
            })
            .unwrap_or(0)
    }

    fn state_file(&self) -> PathBuf {
        self.root.join("state/note-it/state.json")
    }
}

/// Waits for the process to end, killing it if it will not.
fn wait_for_exit(child: &mut Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn skip(reason: &str) {
    eprintln!("SKIP: {reason}");
}

#[test]
fn a_desktop_instance_refuses_to_start_while_another_writer_holds_the_store() {
    let Some(binary) = note_it_binary() else {
        return skip("no note-it binary built; run cargo build");
    };
    if !display_available() {
        return skip("no display; GTK cannot start here (expected in CI)");
    }
    let Some(sandbox) = Sandbox::new() else {
        return skip("no private D-Bus session could be started");
    };

    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare coordination");
    // Held for the whole test, and for far longer than the binary's own bounded
    // wait. This stands in for a `noteit` command that never finishes, or for a
    // second desktop instance.
    let held = WriterLease::try_acquire_prepared(&coordination)
        .expect("prepare")
        .expect("the test must hold the store");

    let mut child = sandbox.spawn_note_it(&binary, &[]);
    let status = wait_for_exit(&mut child, REFUSAL_TIMEOUT);
    let output = child.wait_with_output().ok();
    let stderr = output
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
        .unwrap_or_default();

    let status = status.unwrap_or_else(|| {
        panic!("note-it kept running while another writer held the store\nstderr:\n{stderr}")
    });

    assert!(
        !status.success(),
        "note-it started successfully without owning the store (stderr:\n{stderr})"
    );
    assert!(
        stderr.contains("não pôde iniciar"),
        "the refusal was not explained to the person who ran it:\n{stderr}"
    );

    // The whole point: nothing that could write ever came into being.
    assert_eq!(sandbox.note_count(), 0, "a refused instance created a note");
    assert!(
        !sandbox.state_file().exists(),
        "a refused instance wrote window state"
    );

    drop(held);
    sandbox.clean_runtime();
}

#[test]
fn the_same_instance_starts_normally_once_the_store_is_free() {
    // The refusal has to be about the store being held, not about the harness:
    // released, the very same command works.
    let Some(binary) = note_it_binary() else {
        return skip("no note-it binary built; run cargo build");
    };
    if !display_available() {
        return skip("no display; GTK cannot start here (expected in CI)");
    }
    let Some(sandbox) = Sandbox::new() else {
        return skip("no private D-Bus session could be started");
    };

    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare coordination");
    let held = WriterLease::try_acquire_prepared(&coordination)
        .expect("prepare")
        .expect("hold the store");

    let mut refused = sandbox.spawn_note_it(&binary, &[]);
    assert!(
        wait_for_exit(&mut refused, REFUSAL_TIMEOUT).is_some_and(|s| !s.success()),
        "the held store did not produce a refusal"
    );
    let _ = refused.wait_with_output();

    drop(held);

    let mut running = sandbox.spawn_note_it(&binary, &["--background"]);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut claimed = false;
    while Instant::now() < deadline {
        // The lease being unavailable to us is the proof that the instance took
        // it, which is the same fact the refusal above was about.
        if matches!(WriterLease::try_acquire_prepared(&coordination), Ok(None)) {
            claimed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = running.kill();
    let _ = running.wait();
    sandbox.clean_runtime();

    assert!(
        claimed,
        "the instance did not take the store once it was free"
    );
}

#[test]
fn an_unusable_coordination_directory_refuses_the_same_way() {
    // 4.0E.1 §5.8. A runtime path that cannot be trusted is not a reason to
    // carry on unprotected: it is the same answer as a held store, because the
    // consequence is the same — this process cannot know it is alone.
    let Some(binary) = note_it_binary() else {
        return skip("no note-it binary built; run cargo build");
    };
    if !display_available() {
        return skip("no display; GTK cannot start here (expected in CI)");
    }
    let Some(sandbox) = Sandbox::new() else {
        return skip("no private D-Bus session could be started");
    };

    // A plain file where the per-store coordination directory belongs. Nothing
    // can be created inside it, for any user.
    let coordination = sandbox.coordination();
    std::fs::create_dir_all(coordination.runtime_root()).expect("runtime root");
    std::fs::write(coordination.store_dir(), b"not a directory").expect("occupy the path");

    let mut child = sandbox.spawn_note_it(&binary, &[]);
    let status = wait_for_exit(&mut child, REFUSAL_TIMEOUT);
    let output = child.wait_with_output().ok();
    let stderr = output
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
        .unwrap_or_default();

    let status = status
        .unwrap_or_else(|| panic!("note-it kept running with unusable coordination\n{stderr}"));
    assert!(
        !status.success(),
        "note-it started with an unusable coordination directory (stderr:\n{stderr})"
    );
    assert_eq!(sandbox.note_count(), 0, "a refused instance created a note");
    assert!(
        !sandbox.state_file().exists(),
        "a refused instance wrote window state"
    );
    sandbox.clean_runtime();
}

#[test]
fn a_control_socket_that_cannot_be_opened_refuses_and_releases_the_lease() {
    // 4.0E.1 §4. A lease without a socket is not an authority: the command line
    // would have no way to reach the process that holds the store, so `noteit`
    // would find the store held and unreachable and refuse every write. Taking
    // the lease and carrying on would be the worst of both — this instance
    // writing, and every other one locked out.
    let Some(binary) = note_it_binary() else {
        return skip("no note-it binary built; run cargo build");
    };
    if !display_available() {
        return skip("no display; GTK cannot start here (expected in CI)");
    }
    let Some(sandbox) = Sandbox::new() else {
        return skip("no private D-Bus session could be started");
    };

    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare coordination");
    // A directory where the socket belongs. `remove_file` will not remove it and
    // `bind` cannot replace it — for every user, because it is path resolution
    // rather than a permission bit.
    std::fs::create_dir_all(coordination.socket_path()).expect("occupy the socket path");

    let mut child = sandbox.spawn_note_it(&binary, &[]);
    let status = wait_for_exit(&mut child, REFUSAL_TIMEOUT);
    let output = child.wait_with_output().ok();
    let stderr = output
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
        .unwrap_or_default();

    let status =
        status.unwrap_or_else(|| panic!("note-it kept running without a control socket\n{stderr}"));
    assert!(
        !status.success(),
        "note-it started without a control socket (stderr:\n{stderr})"
    );
    assert!(
        stderr.contains("canal de controle"),
        "the refusal did not say the control channel was the problem:\n{stderr}"
    );
    assert_eq!(sandbox.note_count(), 0, "a refused instance created a note");

    // And the lease is free again: a refusal must not leave the store locked
    // out for everyone else.
    let lease = WriterLease::try_acquire_prepared(&coordination).expect("prepare");
    assert!(lease.is_some(), "a refused startup kept the writer lease");
    drop(lease);

    let _ = std::fs::remove_dir_all(coordination.socket_path());
    sandbox.clean_runtime();
}

#[test]
fn the_desktop_and_the_command_line_agree_on_which_store_they_mean() {
    // 4.0E.1 §19. The store key is the digest of the notes directory path as
    // each process resolved it. Both adapters resolve it from the same
    // `StorePaths`, so both land on the same coordination directory — which is
    // what makes the lease an exclusion between them rather than two locks that
    // never meet.
    let paths = StorePaths::resolve();
    let from_desktop = WriteCoordinationPaths::for_store(&paths);
    let from_cli = WriteCoordinationPaths::for_store(&StorePaths::resolve());

    assert_eq!(from_desktop.store_key(), from_cli.store_key());
    assert_eq!(from_desktop.lock_path(), from_cli.lock_path());
    assert_eq!(from_desktop.socket_path(), from_cli.socket_path());

    // And a different store is a different key, so an isolated instance and the
    // real one never contend.
    let elsewhere = StorePaths::from_custom_paths(
        PathBuf::from("/tmp/note-it-somewhere-else/notes"),
        paths.config_dir.clone(),
        paths.state_dir.clone(),
        paths.runtime_dir.clone(),
    );
    assert_ne!(
        from_desktop.store_key(),
        WriteCoordinationPaths::for_store(&elsewhere).store_key()
    );
}
