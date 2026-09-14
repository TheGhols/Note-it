//! One conversation: a hidden subprocess, its lines, and how it ends.
//!
//! ## No terminal, ever
//!
//! The child is started with piped standard input, output and error and a
//! closed stdin. No pseudoterminal is allocated and no terminal emulator is
//! launched, so nothing appears on screen — the person sees a chat window and
//! never learns that a command-line program is involved.
//!
//! ## Failure is an event, not a crash
//!
//! §29 of the phase: if the AI client crashes, goes offline, ends, answers
//! invalid JSON, takes too long or is not installed, Note-it keeps working and
//! no note is blocked. Everything that can go wrong arrives here as an
//! [`AgentEvent::Failed`] on the same channel as the answer, and the session
//! is over. There is no path from a client's behaviour to a panic in the
//! interface.

use crate::adapter::{Adapter, Request};
use crate::event::{AgentEvent, Failure, ProviderId};
use std::io::{BufRead, BufReader};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A running conversation.
pub struct Session {
    provider: ProviderId,
    events: Receiver<AgentEvent>,
    child: Arc<Mutex<Option<std::process::Child>>>,
    reader: Option<std::thread::JoinHandle<()>>,
    finished: bool,
}

impl Session {
    /// Starts `adapter`'s client on `request`.
    ///
    /// Returns a session even when the client is missing: the failure arrives
    /// as an event, so an interface has one place to draw every outcome
    /// instead of two.
    pub fn start(adapter: Box<dyn Adapter>, request: &Request) -> Self {
        let provider = adapter.id();
        let (sender, events) = mpsc::channel();

        let mut command = adapter.command(request);
        // Its own process group, so that ending the conversation ends
        // everything the client started and not merely the client. Without
        // this, an AI client that spawns a helper leaves the helper holding
        // the pipe being read here, and the read never ends: the chat window
        // would close and the thread behind it would wait for ever.
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let spawned = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let failure = if error.kind() == std::io::ErrorKind::NotFound {
                    Failure::ClientNotFound {
                        program: provider.program().to_owned(),
                    }
                } else {
                    Failure::StartFailed {
                        reason: error.to_string(),
                    }
                };
                let _ = sender.send(AgentEvent::Failed { failure });
                return Self {
                    provider,
                    events,
                    child: Arc::new(Mutex::new(None)),
                    reader: None,
                    finished: false,
                };
            }
        };

        let child = Arc::new(Mutex::new(Some(spawned)));
        let _ = sender.send(AgentEvent::Started { provider });

        let stdout = child
            .lock()
            .expect("the child is not poisoned here")
            .as_mut()
            .and_then(|child| child.stdout.take());

        let reading = Arc::clone(&child);
        let reader = std::thread::spawn(move || {
            let mut terminal_seen = false;
            if let Some(stdout) = stdout {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if adapter.is_unreadable(&line) {
                        let _ = sender.send(AgentEvent::Failed {
                            failure: Failure::Protocol { line },
                        });
                        terminal_seen = true;
                        break;
                    }
                    if let Some(event) = adapter.parse(&line) {
                        terminal_seen |= event.is_terminal();
                        if sender.send(event).is_err() {
                            // Nobody is listening any more.
                            return;
                        }
                        if terminal_seen {
                            break;
                        }
                    }
                }
            }

            // Whatever happened to the stream, the process is reaped here and
            // not left behind: §32 requires that the subprocess is ended.
            let status = reading
                .lock()
                .ok()
                .and_then(|mut child| child.as_mut().map(|child| child.wait()));

            if !terminal_seen {
                let code = status.and_then(|status| status.ok()).and_then(|s| s.code());
                let _ = sender.send(AgentEvent::Failed {
                    failure: Failure::Exited { code },
                });
            }
        });

        Self {
            provider,
            events,
            child,
            reader: Some(reader),
            finished: false,
        }
    }

    pub fn provider(&self) -> ProviderId {
        self.provider
    }

    /// The next event, or `None` once the conversation is over.
    pub fn next_event(&mut self) -> Option<AgentEvent> {
        if self.finished {
            return None;
        }
        match self.events.recv() {
            Ok(event) => {
                self.finished = event.is_terminal();
                Some(event)
            }
            Err(_) => {
                self.finished = true;
                None
            }
        }
    }

    /// The next event, giving up after `timeout`.
    ///
    /// A client that never answers is a failure a person can be told about,
    /// not an interface that hangs.
    pub fn next_event_within(&mut self, timeout: Duration) -> Option<AgentEvent> {
        if self.finished {
            return None;
        }
        match self.events.recv_timeout(timeout) {
            Ok(event) => {
                self.finished = event.is_terminal();
                Some(event)
            }
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                self.finished = true;
                None
            }
        }
    }

    /// Everything, until the conversation ends.
    pub fn collect(&mut self) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        while let Some(event) = self.next_event() {
            events.push(event);
        }
        events
    }

    /// Stops the client now.
    ///
    /// Ends the process group this session created, which is the client and
    /// whatever the client started. The group is identified by the pid of the
    /// process this crate spawned itself — there is no name, no pattern, and
    /// no way for this to reach anything else somebody is running.
    pub fn cancel(&mut self) {
        if let Ok(mut held) = self.child.lock() {
            if let Some(child) = held.as_mut() {
                end_group(child.id());
                let _ = child.kill();
            }
        }
        self.finished = true;
    }

    /// Whether the conversation has ended.
    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

/// Ends the process group led by `pid`.
///
/// `pid` is always one this crate spawned with `process_group(0)`, so it is
/// the group leader and the group contains exactly its descendants.
fn end_group(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    // SAFETY: `kill` with a negative pid signals that process group. The pid
    // is one this crate created and has not yet reaped, so it names that group
    // and nothing else.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
}

impl Drop for Session {
    /// A dropped session leaves no process behind.
    ///
    /// Closing a chat window must not leak an AI client that goes on consuming
    /// somebody's quota in the background.
    fn drop(&mut self) {
        self.cancel();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Ok(mut held) = self.child.lock() {
            if let Some(mut child) = held.take() {
                let _ = child.wait();
            }
        }
    }
}
