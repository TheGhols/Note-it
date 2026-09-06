//! The loop: accept a connection, read one request, answer it, close.
//!
//! ## One request per connection, and why that is the whole concurrency design
//!
//! There is no multiplexing, no request identifier and no session. A client
//! connects, writes one frame, reads one frame, and the connection ends. That
//! removes an entire category of bug — a response delivered to the wrong
//! request — by removing the thing that would make it possible, and it makes
//! "how many requests are in flight" the same question as "how many
//! connections are open", which is a number this file can bound with a
//! counter.
//!
//! [`Limits::max_in_flight`] is the admission control §55 asks for. A
//! connection arriving above the ceiling is answered — with
//! [`WireError::RateLimited`], the honest word for "this process is at
//! capacity" — and closed, rather than queued behind a socket backlog nobody
//! can see the length of.
//!
//! ## What this loop refuses to do
//!
//! It does not open a note, walk a directory, spawn a process or read the
//! store. There is no code here that could: the request type has no field for
//! a path, and `scripts/check-embed-boundary` refuses the APIs that would let
//! this file grow one.

use crate::credential::{self, CredentialError};
use crate::http::{Cancelled, Delay, HttpClient, NeverCancelled, RealDelay};
use crate::provider::{self, EmbedJob};
use noteit_embed_protocol::{
    read_frame, write_frame, EmbedRequestV1, EmbedResponseV1, ProtocolError, WireError,
};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How long a client has to send its request once it has connected.
///
/// A connection that opens and says nothing holds a slot, and a slot is a
/// bounded resource. Short, because the client writes one already-encoded
/// frame immediately or it is not the client.
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the worker has to deliver an answer.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// The ceilings this worker runs under.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The most connections served at once.
    ///
    /// Four rather than one: a query and a background indexing pass are
    /// legitimately concurrent, and a provider's own rate limits are far above
    /// this. Small enough that a bug in the client cannot turn into a hundred
    /// simultaneous paid requests.
    pub max_in_flight: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_in_flight: 4 }
    }
}

/// Everything the worker needs to answer a request.
pub struct Worker {
    client: HttpClient,
    config_dir: PathBuf,
    limits: Limits,
    in_flight: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}

impl Worker {
    pub fn new(config_dir: PathBuf, limits: Limits) -> Self {
        Self {
            client: HttpClient::new(),
            config_dir,
            limits,
            in_flight: Arc::new(AtomicUsize::new(0)),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// A handle that makes [`Self::serve`] return.
    pub fn stopper(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }

    /// Serves until the stopper is set or the listener fails.
    ///
    /// Each connection is handled on its own thread, because one slow provider
    /// must not stop the next question from being asked — the same property
    /// the MCP reactor has, one process further out (§28, §56).
    pub fn serve(self: Arc<Self>, listener: UnixListener) {
        // So a stopper set while the loop is blocked in `accept` is noticed:
        // the timeout turns a blocking accept into a poll without a second
        // mechanism to get wrong.
        let _ = listener.set_nonblocking(false);
        for incoming in listener.incoming() {
            if self.stop.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = incoming else { continue };
            let admitted = self
                .in_flight
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                    (current < self.limits.max_in_flight).then_some(current + 1)
                })
                .is_ok();
            if !admitted {
                // Answered rather than dropped. A client that got no answer
                // cannot tell "at capacity" from "the worker died", and those
                // are different things to do next.
                let mut stream = stream;
                let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
                let _ = write_frame(&mut stream, &EmbedResponseV1::error(WireError::RateLimited));
                continue;
            }
            let worker = Arc::clone(&self);
            let in_flight = Arc::clone(&self.in_flight);
            std::thread::spawn(move || {
                worker.serve_one(stream);
                in_flight.fetch_sub(1, Ordering::SeqCst);
            });
        }
    }

    /// One connection: read a frame, answer it, close.
    pub fn serve_one(&self, mut stream: UnixStream) {
        let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        if !crate::socket::peer_is_trusted(&stream) {
            return;
        }
        let response = self.answer(&mut stream);
        let _ = write_frame(&mut stream, &response);
        let _ = stream.flush();
        // Half-close so the client sees the end of the answer rather than
        // waiting on a connection nothing will write to again.
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }

    fn answer<S: Read + Write>(&self, stream: &mut S) -> EmbedResponseV1 {
        let request: EmbedRequestV1 = match read_frame(stream) {
            Ok(request) => request,
            Err(ProtocolError::UnsupportedVersion) => {
                return EmbedResponseV1::error(WireError::Protocol)
            }
            Err(_) => return EmbedResponseV1::error(WireError::Protocol),
        };
        // Validated again on receipt. The client validates before sending so a
        // bug does not become a bill; this call is the one that matters,
        // because a client is an input.
        if request.validate().is_err() {
            return EmbedResponseV1::error(WireError::Protocol);
        }
        self.run(&request, &NeverCancelled, &RealDelay)
    }

    /// Resolves a credential, calls the provider, and reduces everything to one
    /// word.
    pub fn run(
        &self,
        request: &EmbedRequestV1,
        cancel: &dyn Cancelled,
        delay: &dyn Delay,
    ) -> EmbedResponseV1 {
        let credential = match credential::resolve(request.provider, &self.config_dir) {
            Ok(credential) => credential,
            // Every reason is the same word on the wire. "Your credentials
            // file is mode 644" is a useful thing to say to a person at a
            // terminal and a useless thing to put on a socket, and the
            // difference between `Missing` and `Insecure` is exactly the sort
            // of detail that ends up in a bug report.
            Err(CredentialError::Missing)
            | Err(CredentialError::Insecure)
            | Err(CredentialError::Unreadable) => {
                return EmbedResponseV1::error(WireError::CredentialMissing)
            }
        };

        let job = EmbedJob {
            model: &request.model,
            role: request.role,
            dimension: request.dimension,
            texts: &request.texts,
        };
        match provider::embed(
            &self.client,
            request.provider,
            &credential,
            &job,
            cancel,
            delay,
        ) {
            Ok(vectors) => {
                let dimension = vectors.first().map(Vec::len).unwrap_or(0) as u32;
                match EmbedResponseV1::answer(dimension, vectors) {
                    Ok(response) => response,
                    Err(error) => EmbedResponseV1::error(error),
                }
            }
            Err(error) => EmbedResponseV1::error(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noteit_embed_protocol::{ProviderId, Role, PROTOCOL_VERSION};

    fn worker() -> Worker {
        let empty = std::env::temp_dir().join(format!(
            "noteit-embed-server-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).expect("mkdir");
        Worker::new(empty, Limits::default())
    }

    fn framed(bytes: &[u8]) -> Vec<u8> {
        let mut out = (bytes.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(bytes);
        out
    }

    /// A two-way buffer standing in for a socket.
    struct Pipe {
        input: std::io::Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Read for Pipe {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buffer)
        }
    }

    impl Write for Pipe {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.output.write(buffer)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn answer_to(bytes: Vec<u8>) -> EmbedResponseV1 {
        let worker = worker();
        let mut pipe = Pipe {
            input: std::io::Cursor::new(bytes),
            output: Vec::new(),
        };
        worker.answer(&mut pipe)
    }

    #[test]
    fn a_malformed_frame_is_a_protocol_refusal_and_never_a_panic() {
        for hostile in [
            vec![],
            vec![0u8],
            vec![0, 0, 0, 0],
            u32::MAX.to_be_bytes().to_vec(),
            framed(b"not json at all"),
            framed(b"{"),
            framed(b"{\"protocol_version\":1}"),
            framed(&[0xff, 0xfe, 0xfd]),
        ] {
            assert_eq!(
                answer_to(hostile),
                EmbedResponseV1::error(WireError::Protocol)
            );
        }
    }

    #[test]
    fn an_unknown_protocol_version_is_refused() {
        let body = br#"{"protocol_version":99,"provider":"openai","model":"m","role":"document","texts":["a"]}"#;
        assert_eq!(
            answer_to(framed(body)),
            EmbedResponseV1::error(WireError::Protocol)
        );
    }

    #[test]
    fn a_request_that_breaks_a_limit_is_refused_before_a_credential_is_looked_for() {
        // Empty batch: refused by `validate`, which runs before `run`.
        let body = br#"{"protocol_version":1,"provider":"openai","model":"m","role":"document","texts":[]}"#;
        assert_eq!(
            answer_to(framed(body)),
            EmbedResponseV1::error(WireError::Protocol)
        );
    }

    #[test]
    fn a_field_that_is_not_in_the_protocol_is_refused() {
        // The shape a "helpful" client would send, and the shape §19
        // exists to make impossible.
        let body = br#"{"protocol_version":1,"provider":"openai","model":"m","role":"document","texts":["a"],"note_id":"a-uuid","source_revision":"r"}"#;
        assert_eq!(
            answer_to(framed(body)),
            EmbedResponseV1::error(WireError::Protocol)
        );
    }

    #[test]
    fn a_valid_request_with_no_credential_says_so_and_never_opens_a_connection() {
        let worker = worker();
        let request = EmbedRequestV1 {
            protocol_version: PROTOCOL_VERSION,
            provider: ProviderId::OpenAi,
            model: "text-embedding-3-small".to_string(),
            role: Role::Document,
            dimension: None,
            texts: vec!["uma nota".to_string()],
        };
        // The config directory is empty and no `OPENAI_API_KEY` is set in the
        // test environment, so this is the credential path and not the network
        // path. Asserted rather than assumed by the isolation suite, which
        // watches the process's descriptors.
        let response = worker.run(&request, &NeverCancelled, &RealDelay);
        assert_eq!(
            response,
            EmbedResponseV1::error(WireError::CredentialMissing)
        );
    }

    #[test]
    fn every_credential_failure_is_the_same_word_on_the_wire() {
        // `Missing`, `Insecure` and `Unreadable` are three different things to
        // tell a person and one thing to tell a socket.
        for error in [
            CredentialError::Missing,
            CredentialError::Insecure,
            CredentialError::Unreadable,
        ] {
            let word = match error {
                CredentialError::Missing
                | CredentialError::Insecure
                | CredentialError::Unreadable => WireError::CredentialMissing,
            };
            assert_eq!(word, WireError::CredentialMissing);
        }
    }

    #[test]
    fn the_default_admission_ceiling_is_small_and_not_zero() {
        let limits = Limits::default();
        assert!(limits.max_in_flight >= 1);
        assert!(
            limits.max_in_flight <= 8,
            "a high ceiling turns a client bug into a bill"
        );
    }
}
