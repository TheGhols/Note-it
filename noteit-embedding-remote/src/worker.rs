//! Starting `noteit-embed`, keeping exactly one of it, and stopping it.
//!
//! ## Who launches the worker, and how the secret reaches it
//!
//! §23 asks for this in writing, so: **the process that holds the note
//! store launches the worker, and it does not hand it a credential.**
//! [`spawn`] builds the child's environment from nothing —
//! `Command::env_clear()` — and then puts back an explicit allowlist that
//! contains `PATH`, `HOME`, the XDG directories, the locale and the TLS
//! certificate overrides, and **no** `OPENAI_API_KEY`, `GEMINI_API_KEY` or
//! `VOYAGE_API_KEY`.
//!
//! That is not a tidy-up. It is what makes "`noteit-mcp` does not receive
//! `OPENAI_API_KEY`" a fact rather than a hope: the MCP process never reads a
//! credential into its address space, so it has nothing to forward, and even if
//! its *own* environment carries one — because a person exported it in the
//! shell that started the host — the clear-and-allowlist stops it reaching the
//! worker by inheritance. The worker resolves its own key from a mode-`0600`
//! file it reads itself.
//!
//! `noteit-embedding-remote/tests/worker_isolation.rs` reads
//! `/proc/<worker>/environ` and proves the sentence above rather than asserting
//! it.
//!
//! ## On demand, and only on demand
//!
//! [`WorkerHandle::ensure`] is the only thing that starts a process, it is
//! called only when a remote embedding is actually needed, and the factory
//! default never reaches it: `lexical_only` has no provider, and the local
//! provider is in-process. §43 and §44, made structural by there being
//! no other constructor.

use crate::client::ClientError;
use noteit_embed_protocol::ProviderId;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The environment variables a worker is allowed to inherit.
///
/// An allowlist and not a denylist, because a denylist is a list of the
/// credential names somebody remembered. Everything here is something the
/// worker genuinely needs: where to find its own files, how to resolve a host,
/// and which certificate bundle to trust.
pub const INHERITED_VARIABLES: [&str; 9] = [
    "PATH",
    "HOME",
    "USER",
    "LANG",
    "LC_ALL",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "XDG_RUNTIME_DIR",
    // Respected by rustls-based stacks for a non-default certificate bundle.
    // Not a credential, and a machine that needs it needs it.
    "SSL_CERT_FILE",
];

/// How long to wait for the worker to say it is listening.
pub const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a worker is not available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerError {
    /// The binary could not be started.
    Spawn,
    /// It started and did not become ready in time.
    NotReady,
    /// It was there and is not any more.
    Gone,
}

/// Where the worker binary is, and where its socket goes.
#[derive(Debug, Clone)]
pub struct WorkerPaths {
    pub binary: PathBuf,
    pub socket: PathBuf,
    pub config_dir: PathBuf,
}

impl WorkerPaths {
    /// The ordinary product layout.
    ///
    /// The binary is looked for beside the running executable first, because
    /// that is where an installed Note-it puts it and it is the answer that
    /// does not depend on `PATH`. Falling back to the bare name lets a
    /// development build and a distribution package both work without a
    /// configuration key that could point at any executable on the machine —
    /// which is the reason there is no such key.
    pub fn resolve(config_dir: PathBuf, runtime_dir: &Path) -> Self {
        let binary = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("noteit-embed")))
            .filter(|candidate| candidate.is_file())
            .unwrap_or_else(|| PathBuf::from("noteit-embed"));
        Self {
            binary,
            // Named by the process so two Note-it instances on one machine do
            // not fight over one path, and inside the runtime directory, which
            // is per-user, mode 0700 and cleared with the session.
            socket: runtime_dir.join(format!("embed-{}.sock", std::process::id())),
            config_dir,
        }
    }
}

/// A worker, if one is running.
struct Running {
    child: Child,
    socket: PathBuf,
}

/// The one worker this process may have.
///
/// A mutex and not an atomic, because "start one if there is not one" has to be
/// one decision: two questions arriving at once on an unstarted worker must
/// produce one process, for the same reason two questions about an unindexed
/// store must produce one index (§18).
pub struct WorkerHandle {
    paths: WorkerPaths,
    running: Mutex<Option<Running>>,
}

impl WorkerHandle {
    pub fn new(paths: WorkerPaths) -> Self {
        Self {
            paths,
            running: Mutex::new(None),
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.paths.socket
    }

    /// Whether a worker is running *right now*, without starting one.
    ///
    /// What a diagnostic asks. §53: answering "is the worker
    /// available" must not be the thing that starts it.
    pub fn is_running(&self) -> bool {
        let mut guard = match self.running.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        match guard.as_mut() {
            None => false,
            Some(running) => match running.child.try_wait() {
                // Exited: reap it and forget it, so a dead worker is never
                // reported as an available one and its socket does not stay
                // behind confusing the next question (§45).
                Ok(Some(_)) => {
                    let _ = std::fs::remove_file(&running.socket);
                    *guard = None;
                    false
                }
                Ok(None) => true,
                Err(_) => false,
            },
        }
    }

    /// Makes sure a worker is running, starting one if not.
    pub fn ensure(&self) -> Result<PathBuf, WorkerError> {
        let mut guard = match self.running.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(running) = guard.as_mut() {
            match running.child.try_wait() {
                Ok(None) => return Ok(running.socket.clone()),
                _ => {
                    // Crashed. The socket it left behind is stale, and a stale
                    // socket that looks like a live one is exactly what §46
                    // asks to be impossible.
                    let _ = std::fs::remove_file(&running.socket);
                    *guard = None;
                }
            }
        }
        let running = spawn(&self.paths)?;
        let socket = running.socket.clone();
        *guard = Some(running);
        Ok(socket)
    }

    /// Stops the worker and leaves nothing behind.
    pub fn shutdown(&self) {
        let mut guard = match self.running.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(mut running) = guard.take() {
            let _ = running.child.kill();
            let _ = running.child.wait();
            let _ = std::fs::remove_file(&running.socket);
        }
    }

    /// The child's process id, for a test that wants to read its environment.
    pub fn child_pid(&self) -> Option<u32> {
        let guard = match self.running.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.as_ref().map(|running| running.child.id())
    }
}

/// Never leaves a process or a socket behind, however the holder ends.
impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn spawn(paths: &WorkerPaths) -> Result<Running, WorkerError> {
    if let Some(parent) = paths.socket.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut command = Command::new(&paths.binary);
    command
        .arg("--socket")
        .arg(&paths.socket)
        .arg("--config-dir")
        .arg(&paths.config_dir)
        // Built from nothing, then filled in. The credential names are not on
        // the list, so a key in *this* process's environment cannot reach the
        // child by inheritance.
        .env_clear()
        .stdin(Stdio::null())
        // The worker's standard output is not a channel and is not read.
        .stdout(Stdio::null())
        // Its standard error is: one line saying it is listening. Piped rather
        // than inherited because `noteit-mcp`'s own standard error belongs to
        // the host, and a worker writing into it would be a second voice on a
        // stream somebody else is reading.
        .stderr(Stdio::piped());
    for name in INHERITED_VARIABLES {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    // Nothing on the command line but two paths. A key in `argv` is visible in
    // `ps`, in `/proc/<pid>/cmdline` and in a shell history (§24).
    let mut child = command.spawn().map_err(|_| WorkerError::Spawn)?;

    let ready = wait_for_ready(&mut child);
    if !ready {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(&paths.socket);
        return Err(WorkerError::NotReady);
    }
    Ok(Running {
        child,
        socket: paths.socket.clone(),
    })
}

/// Waits for the worker's one line of standard error.
///
/// A line and not a sleep: a fixed wait is either too short on a loaded machine
/// or wasted on an idle one, and the worker already knows the moment it is
/// listening.
fn wait_for_ready(child: &mut Child) -> bool {
    let Some(stderr) = child.stderr.take() else {
        return false;
    };
    let deadline = Instant::now() + READY_TIMEOUT;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        let seen = reader.read_line(&mut line).unwrap_or(0) > 0 && line.contains("pronto");
        let _ = sender.send(seen);
        // Keep draining so the worker never blocks on a full pipe. Its stderr
        // is for a person and is not otherwise read.
        let mut sink = String::new();
        while reader.read_line(&mut sink).unwrap_or(0) > 0 {
            sink.clear();
        }
    });
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .unwrap_or(false)
}

/// The credential variables that must never be forwarded.
///
/// Duplicated from `noteit-embed` deliberately: this crate does not depend on
/// that one — the whole point is that the process holding the store links no
/// part of the HTTP worker — so the list is stated here and
/// `scripts/check-embed-boundary` checks that the two agree.
pub const NEVER_FORWARDED: [&str; 3] = ["OPENAI_API_KEY", "GEMINI_API_KEY", "VOYAGE_API_KEY"];

/// Whether a variable name is one this process refuses to pass on.
pub fn is_credential_variable(name: &str) -> bool {
    NEVER_FORWARDED.contains(&name)
        || name.ends_with("_API_KEY")
        || name.ends_with("_TOKEN")
        || name.ends_with("_SECRET")
}

/// Turns a worker or client failure into what the provider layer reports.
pub fn to_client_error(error: WorkerError) -> ClientError {
    match error {
        WorkerError::Spawn | WorkerError::NotReady | WorkerError::Gone => ClientError::Unavailable,
    }
}

/// Every provider this build can reach, for a diagnostic that lists them.
pub fn known_providers() -> [ProviderId; 3] {
    ProviderId::ALL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_credential_variable_is_on_the_inherited_list() {
        // The assertion that makes §23 structural. If somebody adds
        // `OPENAI_API_KEY` to the allowlist "so smoke tests work", this fails.
        for name in INHERITED_VARIABLES {
            assert!(
                !is_credential_variable(name),
                "`{name}` is inherited by the worker and looks like a credential"
            );
        }
        for name in NEVER_FORWARDED {
            assert!(
                !INHERITED_VARIABLES.contains(&name),
                "`{name}` is on the inherit list"
            );
        }
    }

    #[test]
    fn the_inherited_list_is_what_a_worker_needs_and_no_more() {
        // Written out so that adding one is a diff a reviewer sees.
        assert_eq!(
            INHERITED_VARIABLES,
            [
                "PATH",
                "HOME",
                "USER",
                "LANG",
                "LC_ALL",
                "XDG_CONFIG_HOME",
                "XDG_CACHE_HOME",
                "XDG_RUNTIME_DIR",
                "SSL_CERT_FILE",
            ]
        );
    }

    #[test]
    fn the_credential_shapes_are_recognised() {
        for name in [
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "VOYAGE_API_KEY",
            "ANYTHING_API_KEY",
            "SOME_TOKEN",
            "A_SECRET",
        ] {
            assert!(is_credential_variable(name), "{name}");
        }
        for name in ["PATH", "HOME", "XDG_CACHE_HOME", "LANG"] {
            assert!(!is_credential_variable(name), "{name}");
        }
    }

    #[test]
    fn a_socket_path_is_per_process_and_in_the_runtime_directory() {
        let paths = WorkerPaths::resolve(
            PathBuf::from("/tmp/config"),
            Path::new("/run/user/1000/note-it"),
        );
        assert!(paths.socket.starts_with("/run/user/1000/note-it"));
        assert!(paths
            .socket
            .to_string_lossy()
            .contains(&std::process::id().to_string()));
        // Never inside the store.
        for forbidden in ["notes", "trash", "backups"] {
            assert!(
                !paths.socket.to_string_lossy().contains(forbidden),
                "the socket path names {forbidden}"
            );
        }
    }
}
