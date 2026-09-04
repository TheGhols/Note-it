//! The throwaway world every MCP suite runs in, and a client to drive it.
//!
//! Two decisions here are deliberate and worth stating.
//!
//! **The store is always synthetic.** Its own `XDG_*` tree, its own runtime
//! directory, its own lease and its own control socket. Nothing in this file
//! can reach the store the person using this machine depends on, because every
//! process it starts is given an environment that resolves somewhere else.
//!
//! **The client is written by hand, not built out of `rmcp`'s client half.**
//! Using the same SDK on both ends would prove that `rmcp` agrees with itself.
//! What has to be proved is that a real host, reading real bytes off a real
//! pipe, sees a real MCP server — so this speaks the wire format directly:
//! one JSON-RPC message per line, on the child's standard input and standard
//! output. It is also why the shipped binary needs no client feature at all.

#![allow(dead_code)]

use noteit_core::control::{read_frame, write_frame, ControlRequest, ControlResponse};
use noteit_core::coordination::{WriteCoordinationPaths, WriterLease};
use noteit_core::model::NoteDocument;
use noteit_core::storage::StorageManager;
use noteit_core::write::{WriteOutcome, WriteOutcomeKind};
use noteit_core::{NoteItCore, StorePaths, Uuid};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tempfile::{tempdir, TempDir};

/// How long any single answer may take before the suite calls it a hang.
///
/// Generous enough that no healthy machine reaches it, and finite so that a
/// server which stops answering fails with a sentence instead of stalling the
/// run until the harness kills it. The concurrency suite depends on this: a
/// reactor blocked behind a Core call would otherwise deadlock the test rather
/// than fail it.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

/// A latch the test opens by hand.
///
/// Two of these replace every `sleep` the concurrency proof would otherwise
/// need: one says "the server has reached the blocking work", the other says
/// "you may finish now". What is between them is ordering the test controls,
/// not a duration it hopes for.
#[derive(Clone, Default)]
pub struct Gate {
    inner: Arc<(Mutex<bool>, Condvar)>,
}

impl Gate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lets everyone waiting through, now and later.
    pub fn open(&self) {
        let (lock, condvar) = &*self.inner;
        *lock.lock().expect("gate") = true;
        condvar.notify_all();
    }

    /// Blocks until [`Gate::open`], or gives up. `false` means it timed out.
    pub fn wait_for(&self, limit: Duration) -> bool {
        let (lock, condvar) = &*self.inner;
        let deadline = std::time::Instant::now() + limit;
        let mut open = lock.lock().expect("gate");
        while !*open {
            let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) else {
                return false;
            };
            let (guard, timeout) = condvar.wait_timeout(open, left).expect("gate");
            open = guard;
            if timeout.timed_out() && !*open {
                return false;
            }
        }
        true
    }
}

/// The MCP revision this harness asks for in `initialize`.
///
/// The handshake one, which is what every host in the field sends today. The
/// SDK owns what it answers; see `mcp_protocol.rs`, which asks for the newer
/// revision as well and checks that the answer is the SDK's and not ours.
pub const HANDSHAKE_PROTOCOL_VERSION: &str = "2025-11-25";

pub fn mcp_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_noteit-mcp"))
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

    /// A sandbox with nothing in it at all: not even the XDG directories.
    ///
    /// The world a read-only tool has to leave exactly as it found it.
    pub fn bare() -> Self {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        Self { _tmp: tmp, root }
    }

    pub fn store_paths(&self) -> StorePaths {
        StorePaths::from_custom_paths(
            self.root.join("data/note-it/notes"),
            self.root.join("config/note-it"),
            self.root.join("state/note-it"),
            self.root.join("runtime/note-it"),
        )
    }

    pub fn coordination(&self) -> WriteCoordinationPaths {
        WriteCoordinationPaths::for_store(&self.store_paths()).expect("coordination paths")
    }

    pub fn core(&self) -> NoteItCore {
        NoteItCore::from_storage(
            StorageManager::from_paths(self.store_paths()).expect("open the sandbox store"),
        )
    }

    /// The environment every child process in these suites is given.
    ///
    /// Headless on purpose, and pointed at this sandbox and nowhere else.
    pub fn apply_env(&self, command: &mut Command) {
        command.env_remove("DISPLAY");
        command.env_remove("WAYLAND_DISPLAY");
        command.env_remove("DBUS_SESSION_BUS_ADDRESS");
        command.env("HOME", &self.root);
        command.env("XDG_DATA_HOME", self.root.join("data"));
        command.env("XDG_CONFIG_HOME", self.root.join("config"));
        command.env("XDG_STATE_HOME", self.root.join("state"));
        command.env("XDG_CACHE_HOME", self.root.join("cache"));
        command.env("XDG_RUNTIME_DIR", self.root.join("runtime"));
    }

    /// Creates a note directly, without going through a tool.
    pub fn seed(&self, content: &str) -> Uuid {
        let mut document = NoteDocument::new_empty();
        document.content = content.to_string();
        self.core()
            .storage()
            .save_note_atomic(&document)
            .expect("seed a note");
        document.metadata.id
    }

    pub fn note_path(&self, id: &str) -> PathBuf {
        self.store_paths().notes_dir.join(format!("{id}.md"))
    }

    /// The exact bytes of a note file, for the comparisons a conflict has to
    /// survive.
    pub fn note_bytes(&self, id: &str) -> Vec<u8> {
        std::fs::read(self.note_path(id)).expect("read the note file")
    }

    pub fn body(&self, id: &str) -> String {
        let raw = std::fs::read_to_string(self.note_path(id)).expect("read the note file");
        NoteDocument::parse(&raw).expect("parse").content
    }
}

/// Everything under a directory: relative path, kind and content digest.
///
/// Used to prove that something changed nothing at all, which is a claim a
/// listing of file names alone cannot support.
pub fn fingerprint(root: &std::path::Path) -> Vec<String> {
    fn walk(root: &std::path::Path, at: &std::path::Path, into: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() => {
                    into.push(format!("dir  {relative}"));
                    walk(root, &path, into);
                }
                Ok(meta) if meta.is_file() => {
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    into.push(format!(
                        "file {relative} {} {}",
                        meta.len(),
                        noteit_core::hashing::sha256_hex(&bytes)
                    ));
                }
                _ => into.push(format!("other {relative}")),
            }
        }
    }
    let mut entries = Vec::new();
    walk(root, root, &mut entries);
    entries.sort();
    entries
}

/// One `noteit-mcp` process, spoken to over its own standard streams.
pub struct McpClient {
    child: Child,
    /// Taken away by `finish`, which closes it to let the server end the way a
    /// host ends it: by hanging up.
    stdin: Option<ChildStdin>,
    /// Lines the server has written, pumped off the pipe by a thread.
    ///
    /// Reading straight from the pipe would make "the server never answered"
    /// indistinguishable from "the test is still waiting", and the suite that
    /// proves the reactor keeps answering while a handler is busy has to be
    /// able to tell those apart. A thread and a channel make every read
    /// bounded by [`ANSWER_TIMEOUT`].
    lines: Receiver<Option<String>>,
    /// Answers read off the channel while looking for a different id, each
    /// beside the number of bytes its line occupied on the pipe.
    held: Vec<(usize, Value)>,
    next_id: i64,
}

impl McpClient {
    /// Starts the server against a sandbox and completes the handshake.
    pub fn start(sandbox: &Sandbox) -> Self {
        let mut client = Self::spawn(sandbox);
        client.initialize(HANDSHAKE_PROTOCOL_VERSION);
        client
    }

    /// Starts the server without saying anything to it.
    pub fn spawn(sandbox: &Sandbox) -> Self {
        let mut command = Command::new(mcp_bin());
        sandbox.apply_env(&mut command);
        Self::from_command(command)
    }

    /// The same, from a command whose environment the caller has decided.
    ///
    /// Used by the suite that spells one store's path several different ways.
    pub fn from_command(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn noteit-mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (sender, lines) = channel();
        std::thread::spawn(move || pump(stdout, sender));
        Self {
            child,
            stdin: Some(stdin),
            lines,
            held: Vec::new(),
            next_id: 1,
        }
    }

    /// The `initialize` handshake, and the `notifications/initialized` that
    /// follows it. Answers with the server's `InitializeResult`.
    pub fn initialize(&mut self, protocol_version: &str) -> Value {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion": protocol_version,
                "capabilities": {},
                "clientInfo": { "name": "noteit-mcp-tests", "version": "0" },
            }),
        );
        let result = result.expect("initialize must succeed");
        self.notify("notifications/initialized", json!({}));
        result
    }

    /// Sends one request and reads the answer that belongs to it.
    ///
    /// `Ok` is the JSON-RPC `result`, `Err` its `error`. A tool that refused
    /// is an `Ok` carrying `isError`, which is the distinction MCP draws and
    /// the one these suites keep.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let id = self.send_request(method, params);
        self.await_response(id)
    }

    /// Sends one request and does **not** wait for its answer.
    ///
    /// The half of `request` the concurrency suite needs: it has to have a
    /// call in flight before it can ask whether anything else still gets
    /// through.
    pub fn send_request(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        id
    }

    /// Waits for the answer belonging to `id`, keeping any other answer that
    /// arrives first.
    pub fn await_response(&mut self, id: i64) -> Result<Value, Value> {
        self.await_response_on_the_wire(id).1
    }

    /// The same, and how many bytes the answer occupied on the pipe.
    ///
    /// The count is the JSON-RPC message without its newline, which is what a
    /// host reads before it parses anything. The suite that bounds a tool
    /// answer needs the number the operating system moved, not a number
    /// recomputed from a parsed value and hoped to agree.
    pub fn await_response_on_the_wire(&mut self, id: i64) -> (usize, Result<Value, Value>) {
        if let Some(index) = self
            .held
            .iter()
            .position(|(_, held)| held.get("id").and_then(Value::as_i64) == Some(id))
        {
            let (bytes, message) = self.held.remove(index);
            return (bytes, Self::split(message));
        }
        loop {
            let (bytes, message) = self.read_message();
            // Notifications and server-initiated requests are not this
            // answer; skipping them is what makes the correlation real.
            match message.get("id").and_then(Value::as_i64) {
                Some(answered) if answered == id => return (bytes, Self::split(message)),
                Some(_) => self.held.push((bytes, message)),
                None => continue,
            }
        }
    }

    /// The next answer to arrive, whatever it answers.
    ///
    /// This is the one the concurrency proof reads: *which* answer reaches the
    /// host first is the observation, and a call that waited for a particular
    /// id could not make it.
    pub fn next_response(&mut self) -> (i64, Result<Value, Value>) {
        if !self.held.is_empty() {
            let (_, message) = self.held.remove(0);
            let id = message.get("id").and_then(Value::as_i64).expect("id");
            return (id, Self::split(message));
        }
        loop {
            let (_, message) = self.read_message();
            if let Some(id) = message.get("id").and_then(Value::as_i64) {
                return (id, Self::split(message));
            }
        }
    }

    fn split(message: Value) -> Result<Value, Value> {
        if let Some(error) = message.get("error") {
            return Err(error.clone());
        }
        Ok(message.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// The operating system's identifier for the server process.
    ///
    /// Used by `mcp_no_network.rs`, which asks the kernel what this process
    /// actually holds open rather than taking the source code's word for it.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Everything `/proc/<pid>/fd` says this process has open, as
    /// `(descriptor number, what it points at)`.
    ///
    /// A descriptor whose target cannot be read is reported as `<unreadable>`
    /// rather than skipped: a file descriptor nobody could classify is exactly
    /// the one a check like this must not quietly ignore.
    pub fn open_descriptors(&self) -> Vec<(u32, String)> {
        let directory = format!("/proc/{}/fd", self.pid());
        let Ok(entries) = std::fs::read_dir(&directory) else {
            panic!("{directory} could not be read; this suite needs procfs");
        };
        let mut descriptors: Vec<(u32, String)> = entries
            .flatten()
            .filter_map(|entry| {
                let number: u32 = entry.file_name().to_string_lossy().parse().ok()?;
                let target = std::fs::read_link(entry.path())
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| "<unreadable>".to_string());
                Some((number, target))
            })
            .collect();
        descriptors.sort();
        descriptors
    }

    /// `tools/list`, as a host sees it.
    pub fn list_tools(&mut self) -> Vec<Value> {
        let result = self.request("tools/list", json!({})).expect("tools/list");
        result["tools"]
            .as_array()
            .expect("tools must be an array")
            .clone()
    }

    /// One tool call. Answers with the `CallToolResult`, refusal included.
    pub fn call(&mut self, name: &str, arguments: Value) -> ToolAnswer {
        self.call_on_the_wire(name, arguments).0
    }

    /// One tool call, and the bytes its answer occupied on the pipe.
    ///
    /// `bytes` is the whole JSON-RPC message: the `result` object plus the
    /// `{"jsonrpc":"2.0","id":N,"result":}` the host itself asked for. The
    /// budget in `noteit_mcp::budget` bounds the first of those, so a test
    /// comparing against it works from [`Self::result_bytes`] and reports this.
    pub fn call_on_the_wire(&mut self, name: &str, arguments: Value) -> (ToolAnswer, usize) {
        let id = self.send_request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let (bytes, result) = self.await_response_on_the_wire(id);
        let result = result.unwrap_or_else(|error| {
            panic!("tools/call {name} failed at the protocol level: {error}")
        });
        (ToolAnswer::from(result), bytes)
    }

    /// A tool call that is expected to be refused by the protocol itself —
    /// a missing required argument, say — rather than by the tool.
    pub fn call_expecting_protocol_error(&mut self, name: &str, arguments: Value) -> Value {
        match self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        ) {
            Ok(result) => panic!("{name} was accepted and should not have been: {result}"),
            Err(error) => error,
        }
    }

    fn send(&mut self, message: Value) {
        let line = serde_json::to_string(&message).expect("serialize");
        let stdin = self.stdin.as_mut().expect("the client is still open");
        stdin.write_all(line.as_bytes()).expect("write");
        stdin.write_all(b"\n").expect("write newline");
        stdin.flush().expect("flush");
    }

    /// Reads one line and insists it is a whole JSON-RPC message.
    ///
    /// A line that is not is the failure this is looking for: anything at all
    /// printed to standard output corrupts the stream, and the test that
    /// notices must be the one that names it.
    fn read_message(&mut self) -> (usize, Value) {
        let line = match self.lines.recv_timeout(ANSWER_TIMEOUT) {
            Ok(Some(line)) => line,
            Ok(None) => panic!("the server closed its output unexpectedly"),
            Err(RecvTimeoutError::Timeout) => panic!(
                "the server said nothing for {ANSWER_TIMEOUT:?}; \
                 a handler is holding the protocol"
            ),
            Err(RecvTimeoutError::Disconnected) => {
                panic!("the server closed its output unexpectedly")
            }
        };
        let message = serde_json::from_str(&line).unwrap_or_else(|error| {
            panic!("a line on stdout was not a JSON-RPC message ({error}): {line:?}")
        });
        (line.len(), message)
    }

    /// Closes standard input and collects what the process said and returned.
    pub fn finish(mut self) -> Finished {
        drop(self.stdin.take());
        let mut remaining = Vec::new();
        // Everything already taken off the pipe while looking for some other
        // answer counts as trailing output too.
        for (_, held) in self.held.drain(..) {
            remaining.push(held.to_string());
        }
        while let Ok(Some(line)) = self.lines.recv_timeout(ANSWER_TIMEOUT) {
            remaining.push(line);
        }
        let remaining_stdout = remaining.join("\n").into_bytes();
        let status = self.child.wait().expect("wait");
        let mut stderr = String::new();
        if let Some(mut handle) = self.child.stderr.take() {
            use std::io::Read;
            handle.read_to_string(&mut stderr).ok();
        }
        Finished {
            code: status.code(),
            trailing_stdout: String::from_utf8_lossy(&remaining_stdout).to_string(),
            stderr,
        }
    }
}

/// Moves the server's lines off the pipe as they arrive.
///
/// `None` is end of stream. Nothing here parses or judges: a line that is not
/// JSON is still delivered, because the suite that notices rubbish on standard
/// output has to be the one that names it.
fn pump(stdout: std::process::ChildStdout, sender: Sender<Option<String>>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => {
                let _ = sender.send(None);
                return;
            }
            Ok(_) => {
                if sender.send(Some(line.trim_end().to_string())).is_err() {
                    return;
                }
            }
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Started by this test and by nothing else, so this test ends it.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Finished {
    pub code: Option<i32>,
    /// Anything left on standard output after the last answer. Must be empty.
    pub trailing_stdout: String,
    pub stderr: String,
}

/// One `CallToolResult`, with the two things a caller actually branches on.
pub struct ToolAnswer {
    pub raw: Value,
}

impl From<Value> for ToolAnswer {
    fn from(raw: Value) -> Self {
        Self { raw }
    }
}

impl ToolAnswer {
    /// The bytes this `CallToolResult` serialises to.
    ///
    /// What `noteit_mcp::budget::MAX_READ_RESPONSE_BYTES` bounds, measured the
    /// way the wire measures it: the value came off the pipe, and putting it
    /// back into JSON gives the same length the server wrote, whatever order
    /// the keys ended up in.
    pub fn result_bytes(&self) -> usize {
        serde_json::to_string(&self.raw).expect("serialise").len()
    }

    pub fn is_error(&self) -> bool {
        self.raw
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// The structured result. Every tool in this server produces one, so its
    /// absence is a failure rather than a case to handle.
    pub fn structured(&self) -> &Value {
        self.raw
            .get("structuredContent")
            .unwrap_or_else(|| panic!("a tool answered without structured content: {}", self.raw))
    }

    pub fn status(&self) -> &str {
        self.structured()["status"]
            .as_str()
            .unwrap_or_else(|| panic!("no status: {}", self.raw))
    }

    pub fn code(&self) -> Option<&str> {
        self.structured().get("code").and_then(Value::as_str)
    }

    pub fn commit_state(&self) -> Option<&str> {
        self.structured()
            .get("commit_state")
            .and_then(Value::as_str)
    }

    pub fn str_field(&self, name: &str) -> Option<&str> {
        self.structured().get(name).and_then(Value::as_str)
    }

    pub fn note_id(&self) -> String {
        self.str_field("note_id")
            .unwrap_or_else(|| panic!("no note_id: {}", self.raw))
            .to_string()
    }

    pub fn revision(&self) -> String {
        self.str_field("revision")
            .unwrap_or_else(|| panic!("no revision: {}", self.raw))
            .to_string()
    }
}

/// Reads a note and returns `(note_id, revision)`.
pub fn read_revision(client: &mut McpClient, note_id: &str) -> String {
    let answer = client.call("noteit_read", json!({ "note_id": note_id }));
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    answer.structured()["note"]["revision"]
        .as_str()
        .expect("a read must publish the revision it describes")
        .to_string()
}

pub fn create_note(client: &mut McpClient, content: &str) -> String {
    let answer = client.call("noteit_create", json!({ "content": content }));
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    answer.note_id()
}

// ---------------------------------------------------------- fake authority

/// A stand-in for the desktop instance: holds the lease and answers on the
/// socket exactly as the real authority does.
///
/// It exists so the MCP server's side of the conversation can be tested
/// without a compositor, a WebView or a display — the very things this binary
/// is supposed not to need.
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
    /// Answer properly, with the outcome the real Core would have produced for
    /// that operation, applied to this store.
    CommitForReal,
    /// Read the request and hang up without a word — the shape of a crash
    /// *before* the change was made. Nothing is written.
    HangUpAfterRequest,
    /// Commit the operation for real and *then* hang up without answering.
    ///
    /// The other half of `indeterminate`, and the half that matters: the write
    /// happened and the caller cannot know it. A client that treats the missing
    /// answer as a failure and repeats the request lands the same paragraph in
    /// the note twice.
    CommitThenHangUp,
    /// Answer with a protocol version this build does not speak.
    WrongVersion(u32),
    /// Refuse every request the way a peer speaking an older private protocol
    /// does: read the frame, see a version it does not know, and say so.
    RefuseOnVersion,
    /// Commit properly, but only once the test says so.
    ///
    /// `arrived` opens the moment the authority has the request in its hands,
    /// which is the moment the server is provably inside the blocking Core
    /// call; `release` is the test's answer to "you may finish now". Between
    /// them the tool call cannot complete, and that window is where the
    /// concurrency proof asks whether the protocol still answers.
    CommitWhenReleased { arrived: Gate, release: Gate },
    /// Answer the way a desktop instance with an open editor does: the base is
    /// not the file, it is the committed note with the editor's unsaved text
    /// folded in, and the precondition is checked against *that*.
    ///
    /// This is the same `apply_over_live_body` the real authority calls. What
    /// it makes testable without a compositor is the one case a file cannot
    /// answer: somebody typed a paragraph into the window, the client's
    /// revision predates it, and the client's write must be refused rather
    /// than allowed to erase what is on screen.
    LiveEditor { unsaved_text: String },
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
        let listener = std::os::unix::net::UnixListener::bind(&socket_path).expect("bind");
        noteit_core::coordination::narrow_socket_file(&socket_path).expect("narrow");

        let handled = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicUsize::new(0));
        let handled_thread = Arc::clone(&handled);
        let stop_thread = Arc::clone(&stop);
        let paths = sandbox.store_paths();

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
                    AuthorityBehaviour::HangUpAfterRequest => drop(stream),
                    AuthorityBehaviour::WrongVersion(version) => {
                        let mut response = ControlResponse::accepted(
                            request.request_id,
                            WriteOutcome::new(Uuid::new_v4(), WriteOutcomeKind::NoteCreated, true),
                        );
                        response.protocol_version = *version;
                        let _ = write_frame(&mut stream, &response);
                    }
                    AuthorityBehaviour::RefuseOnVersion => {
                        let error = noteit_core::control::check_protocol_version(
                            request.protocol_version.wrapping_add(1),
                        )
                        .expect_err("a mismatched version must refuse");
                        let response = ControlResponse::refused(request.request_id, error);
                        let _ = write_frame(&mut stream, &response);
                    }
                    AuthorityBehaviour::LiveEditor { unsaved_text } => {
                        let storage =
                            StorageManager::from_paths(paths.clone()).expect("open the store");
                        let core = NoteItCore::from_storage(storage);
                        let response = match &request.operation {
                            noteit_core::write::WriteOperation::MutateNote {
                                selector,
                                mutation,
                                expected_revision,
                            } => {
                                let note_id =
                                    core.resolve_note_id(selector).expect("resolve the note");
                                let committed =
                                    core.read_note(&note_id).expect("read the committed note");
                                match noteit_core::write::apply_over_live_body(
                                    &committed,
                                    unsaved_text,
                                    mutation,
                                    expected_revision,
                                ) {
                                    Ok(live) => match live.candidate {
                                        None => ControlResponse::accepted(
                                            request.request_id,
                                            WriteOutcome::new(
                                                note_id,
                                                mutation.outcome_kind(),
                                                false,
                                            )
                                            .with_revision(live.base_revision),
                                        ),
                                        Some(candidate) => {
                                            noteit_core::write::commit_addressed(
                                                &core, &note_id, &candidate,
                                            )
                                            .expect("commit");
                                            ControlResponse::accepted(
                                                request.request_id,
                                                WriteOutcome::new(
                                                    note_id,
                                                    mutation.outcome_kind(),
                                                    live.mutation_changed,
                                                )
                                                .with_revision(
                                                    noteit_core::write::revision_of(&candidate)
                                                        .expect("revision"),
                                                ),
                                            )
                                        }
                                    },
                                    Err(error) => {
                                        ControlResponse::refused(request.request_id, error)
                                    }
                                }
                            }
                            other => {
                                let outcome = noteit_core::write::execute(&core, other);
                                match outcome {
                                    Ok(outcome) => {
                                        ControlResponse::accepted(request.request_id, outcome)
                                    }
                                    Err(error) => {
                                        ControlResponse::refused(request.request_id, error)
                                    }
                                }
                            }
                        };
                        let _ = write_frame(&mut stream, &response);
                    }
                    AuthorityBehaviour::CommitWhenReleased { arrived, release } => {
                        arrived.open();
                        // Held here, inside the operation the tool is waiting
                        // on, until the test has asked its question.
                        release.wait_for(Duration::from_secs(30));
                        let storage =
                            StorageManager::from_paths(paths.clone()).expect("open the store");
                        let core = NoteItCore::from_storage(storage);
                        let response = match noteit_core::write::execute(&core, &request.operation)
                        {
                            Ok(outcome) => ControlResponse::accepted(request.request_id, outcome),
                            Err(error) => ControlResponse::refused(request.request_id, error),
                        };
                        let _ = write_frame(&mut stream, &response);
                    }
                    AuthorityBehaviour::CommitThenHangUp => {
                        let storage =
                            StorageManager::from_paths(paths.clone()).expect("open the store");
                        let core = NoteItCore::from_storage(storage);
                        let _ = noteit_core::write::execute(&core, &request.operation);
                        // The answer is never written: the socket closes with
                        // the change already on disk.
                        drop(stream);
                    }
                    AuthorityBehaviour::CommitForReal => {
                        // The real thing: this process holds the lease, so it
                        // is the one entitled to run the operation, and it
                        // runs it through the same Core the desktop does.
                        let storage =
                            StorageManager::from_paths(paths.clone()).expect("open the store");
                        let core = NoteItCore::from_storage(storage);
                        let response = match noteit_core::write::execute(&core, &request.operation)
                        {
                            Ok(outcome) => ControlResponse::accepted(request.request_id, outcome),
                            Err(error) => ControlResponse::refused(request.request_id, error),
                        };
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
        let _ = std::os::unix::net::UnixStream::connect(&self.socket_path);
        let _ = std::fs::remove_file(&self.socket_path);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
