//! The throwaway world the CLI's process-level tests run in.
//!
//! Shared by every integration suite in this crate so there is exactly one
//! answer to "which store is this?" — a synthetic one, with its own XDG tree
//! and its own runtime directory, so the lease and the control socket under
//! test are never the ones the person using this machine depends on.
//!
//! Compiled into every suite that names it, so each one sees the whole harness
//! whether or not it uses all of it.
#![allow(dead_code)]

use noteit_core::control::{read_frame, write_frame, ControlRequest, ControlResponse};
use noteit_core::coordination::{WriteCoordinationPaths, WriterLease};
use noteit_core::model::NoteDocument;
use noteit_core::storage::StorageManager;
use noteit_core::write::{WriteOutcome, WriteOutcomeKind};
use noteit_core::{NoteItCore, StorePaths, Uuid};
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::{tempdir, TempDir};

pub fn noteit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_noteit"))
}

/// A throwaway store with a throwaway runtime directory beside it.
pub struct Sandbox {
    _tmp: TempDir,
    pub root: PathBuf,
}

impl Sandbox {
    pub fn new() -> Self {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        for name in ["data", "config", "state", "cache", "runtime"] {
            std::fs::create_dir_all(root.join(name)).expect("create the sandbox");
        }
        Self { _tmp: tmp, root }
    }

    /// The paths the `noteit` binary will resolve from this sandbox's XDG
    /// environment. Kept in one place so the test and the process under test
    /// can never disagree about which store they mean.
    pub fn store_paths(&self) -> StorePaths {
        StorePaths::from_custom_paths(
            self.root.join("data/note-it/notes"),
            self.root.join("config/note-it"),
            self.root.join("state/note-it"),
            self.root.join("runtime/note-it"),
        )
    }

    pub fn coordination(&self) -> WriteCoordinationPaths {
        WriteCoordinationPaths::for_store(&self.store_paths())
    }

    pub fn core(&self) -> NoteItCore {
        NoteItCore::from_storage(
            StorageManager::from_paths(self.store_paths()).expect("open the sandbox store"),
        )
    }

    pub fn notes_dir(&self) -> PathBuf {
        self.store_paths().data_dir.join("notes")
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(noteit_bin());
        command.args(args);
        // Headless on purpose: no display, no compositor, no session bus.
        command.env_remove("DISPLAY");
        command.env_remove("WAYLAND_DISPLAY");
        command.env_remove("DBUS_SESSION_BUS_ADDRESS");
        command.env("NO_COLOR", "1");
        command.env("XDG_DATA_HOME", self.root.join("data"));
        command.env("XDG_CONFIG_HOME", self.root.join("config"));
        command.env("XDG_STATE_HOME", self.root.join("state"));
        command.env("XDG_CACHE_HOME", self.root.join("cache"));
        command.env("XDG_RUNTIME_DIR", self.root.join("runtime"));
        command
    }

    pub fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = self.command(args).output().expect("run noteit");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    }

    pub fn run_with_stdin(&self, args: &[&str], stdin: &str) -> (i32, String, String) {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn noteit");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(stdin.as_bytes())
            .expect("write stdin");
        let output = child.wait_with_output().expect("wait");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    }

    /// Creates a note directly, without going through a command.
    pub fn seed(&self, content: &str) -> Uuid {
        let mut document = NoteDocument::new_empty();
        document.content = content.to_string();
        self.core()
            .storage()
            .save_note_atomic(&document)
            .expect("seed a note");
        document.metadata.id
    }

    pub fn body(&self, id: Uuid) -> String {
        self.core().read_note(&id).expect("read").content
    }

    /// The bytes of one note's file, for the comparisons that have to be exact.
    pub fn note_file(&self, id: Uuid) -> Vec<u8> {
        std::fs::read(self.notes_dir().join(format!("{id}.md"))).expect("read the note file")
    }
}

pub fn prefix(id: Uuid) -> String {
    id.as_simple().to_string()[..8].to_string()
}

/// A stand-in for the desktop instance: holds the lease and answers on the
/// socket exactly as the real authority does.
///
/// It exists so the CLI's side of the conversation can be tested without a
/// compositor, a WebView or a display — the very thing the CLI is supposed not
/// to need.
pub struct FakeAuthority {
    _lease: WriterLease,
    handled: Arc<AtomicUsize>,
    stop: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
    socket_path: PathBuf,
}

/// How a fake authority answers one request.
#[derive(Clone)]
pub enum AuthorityBehaviour {
    /// Answer properly.
    Commit,
    /// Answer properly, with exactly this outcome.
    CommitOutcome(WriteOutcome),
    /// Read the request and hang up without a word — the shape of a crash
    /// after the change may already have been committed.
    HangUpAfterRequest,
    /// Answer with a protocol version this build does not speak.
    WrongVersion,
    /// Answer an entirely different request. Whatever happened to this one is
    /// not in the envelope that came back.
    MismatchedResponseId,
}

impl FakeAuthority {
    pub fn start(sandbox: &Sandbox, behaviour: AuthorityBehaviour) -> Self {
        let coordination = sandbox.coordination();
        coordination.prepare().expect("prepare");
        let lease = WriterLease::try_acquire_prepared(&coordination)
            .expect("prepare")
            .expect("the fake authority must hold the store");

        let socket_path = coordination.socket_path();
        let _ = std::fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).expect("bind");
        noteit_core::coordination::narrow_socket_file(&socket_path).expect("narrow");

        let handled = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicUsize::new(0));
        let handled_thread = Arc::clone(&handled);
        let stop_thread = Arc::clone(&stop);

        let thread = std::thread::spawn(move || {
            for connection in listener.incoming() {
                if stop_thread.load(Ordering::SeqCst) == 1 {
                    return;
                }
                let Ok(mut stream) = connection else { continue };
                let request: ControlRequest = match read_frame(&mut stream) {
                    Ok(request) => request,
                    Err(_) => continue,
                };
                handled_thread.fetch_add(1, Ordering::SeqCst);

                match &behaviour {
                    AuthorityBehaviour::HangUpAfterRequest => {
                        drop(stream);
                    }
                    AuthorityBehaviour::WrongVersion => {
                        let mut response = ControlResponse::accepted(
                            request.request_id,
                            WriteOutcome::new(Uuid::new_v4(), WriteOutcomeKind::NoteCreated, true),
                        );
                        response.protocol_version = 999;
                        let _ = write_frame(&mut stream, &response);
                    }
                    AuthorityBehaviour::MismatchedResponseId => {
                        let response = ControlResponse::accepted(
                            Uuid::new_v4(),
                            WriteOutcome::new(
                                Uuid::new_v4(),
                                WriteOutcomeKind::ContentAppended,
                                true,
                            ),
                        );
                        let _ = write_frame(&mut stream, &response);
                    }
                    AuthorityBehaviour::CommitOutcome(outcome) => {
                        let response =
                            ControlResponse::accepted(request.request_id, outcome.clone());
                        let _ = write_frame(&mut stream, &response);
                    }
                    AuthorityBehaviour::Commit => {
                        let response = ControlResponse::accepted(
                            request.request_id,
                            WriteOutcome::new(
                                Uuid::new_v4(),
                                WriteOutcomeKind::ContentAppended,
                                true,
                            ),
                        );
                        let _ = write_frame(&mut stream, &response);
                    }
                }
            }
        });

        Self {
            _lease: lease,
            handled,
            stop,
            thread: Some(thread),
            socket_path,
        }
    }

    pub fn handled(&self) -> usize {
        self.handled.load(Ordering::SeqCst)
    }
}

impl Drop for FakeAuthority {
    fn drop(&mut self) {
        self.stop.store(1, Ordering::SeqCst);
        // Wakes the accept loop so the thread can see the stop flag.
        let _ = std::os::unix::net::UnixStream::connect(&self.socket_path);
        let _ = std::fs::remove_file(&self.socket_path);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
