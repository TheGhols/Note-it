//! A controllable HTTP server on loopback, and nothing that reaches the
//! internet.
//!
//! §47 and §49 are the reason this file exists: the suite must be
//! able to produce a 401, a 429 with a `Retry-After`, a truncated JSON body, a
//! vector full of `NaN` and a connection that dies mid-response — and it must
//! do all of that **without** `api.openai.com`, without a key and without a
//! network. `NOTEIT_REMOTE_SMOKE` is the only path that ever touches a real
//! vendor, it is `#[ignore]`, and it is not in this file.
//!
//! Plain HTTP on `127.0.0.1`, not TLS. The thing under test here is the
//! adapter's parsing, its status handling and its retry policy; that the
//! client validates certificates is a property of `crate::http`'s
//! configuration, asserted by reading it and by `scripts/check-embed-boundary`
//! refusing `danger_accept_invalid_certs` anywhere in the workspace. A
//! self-signed certificate in a test would be the one place in this repository
//! where certificate validation was turned off, which is precisely the
//! construction the gate exists to forbid.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the mock does when it is asked.
#[derive(Clone)]
pub enum Reply {
    /// A status and a body, verbatim.
    Status(u16, String),
    /// A status, a body, and one extra header.
    WithHeader(u16, String, &'static str, String),
    /// Answer after waiting. For the reactor and timeout tests.
    Slow(Duration, u16, String),
    /// Announce a length and send less than that, then hang up.
    Truncated(u16, String),
    /// Accept the connection and close it without writing anything.
    Hangup,
    /// A body far larger than the client's ceiling.
    Enormous(usize),
    /// A 30x pointing somewhere else.
    Redirect(u16, String),
}

/// A loopback HTTP server that answers from a script.
pub struct MockProvider {
    base: String,
    hits: Arc<AtomicUsize>,
    bodies: Arc<Mutex<Vec<String>>>,
    stop: Arc<TcpListener>,
}

impl MockProvider {
    /// Serves `script` in order; the last entry repeats once the script runs
    /// out, so a test that only cares about the first answer says one thing.
    pub fn start(script: Vec<Reply>) -> Self {
        assert!(!script.is_empty(), "a mock needs at least one reply");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("addr").port();
        let hits = Arc::new(AtomicUsize::new(0));
        let bodies = Arc::new(Mutex::new(Vec::new()));

        let listener = Arc::new(listener);
        let serving = Arc::clone(&listener);
        let counting = Arc::clone(&hits);
        let recording = Arc::clone(&bodies);
        std::thread::spawn(move || {
            for incoming in serving.incoming() {
                let Ok(stream) = incoming else { break };
                let index = counting.fetch_add(1, Ordering::SeqCst);
                let reply = script
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| script.last().expect("non-empty").clone());
                let recording = Arc::clone(&recording);
                std::thread::spawn(move || handle(stream, reply, recording));
            }
        });

        Self {
            base: format!("http://127.0.0.1:{port}"),
            hits,
            bodies,
            stop: listener,
        }
    }

    /// One 200 with this body, for as many requests as arrive.
    pub fn always(status: u16, body: &str) -> Self {
        Self::start(vec![Reply::Status(status, body.to_string())])
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// How many requests have arrived. The counter §81 and §82
    /// are measured with.
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    /// The request bodies, in arrival order, so a test can assert what was
    /// sent — and, for the privacy tests, what was *not*.
    pub fn bodies(&self) -> Vec<String> {
        self.bodies.lock().expect("bodies").clone()
    }
}

impl Drop for MockProvider {
    fn drop(&mut self) {
        // Waking the accept loop so the thread ends with the test.
        let _ = TcpStream::connect(self.stop.local_addr().expect("addr"));
    }
}

fn handle(mut stream: TcpStream, reply: Reply, bodies: Arc<Mutex<Vec<String>>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let body = read_request(&mut stream);
    if let Ok(mut recorded) = bodies.lock() {
        recorded.push(body);
    }

    match reply {
        Reply::Status(status, body) => write_response(&mut stream, status, &body, &[]),
        Reply::WithHeader(status, body, name, value) => {
            write_response(&mut stream, status, &body, &[(name, value.as_str())])
        }
        Reply::Slow(wait, status, body) => {
            std::thread::sleep(wait);
            write_response(&mut stream, status, &body, &[]);
        }
        Reply::Truncated(status, body) => {
            // A `content-length` that promises more than is sent, then a close.
            let head = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
                body.len() + 64
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body.as_bytes());
            let _ = stream.flush();
        }
        Reply::Hangup => {}
        Reply::Enormous(bytes) => {
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {bytes}\r\n\r\n"
            );
            let _ = stream.write_all(head.as_bytes());
            let chunk = vec![b'a'; 64 * 1024];
            let mut sent = 0;
            while sent < bytes {
                let take = chunk.len().min(bytes - sent);
                if stream.write_all(&chunk[..take]).is_err() {
                    break;
                }
                sent += take;
            }
        }
        Reply::Redirect(status, location) => {
            write_response(&mut stream, status, "", &[("location", location.as_str())])
        }
    }
    let _ = stream.flush();
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return String::new();
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; length];
    if reader.read_exact(&mut body).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&body).into_owned()
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str, extra: &[(&str, &str)]) {
    let mut head = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (name, value) in extra {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

// ------------------------------------------------------------ bodies to serve

/// A well-formed OpenAI or Voyage answer of `count` vectors of `dimension`.
pub fn indexed_body(count: usize, dimension: usize) -> String {
    let rows: Vec<String> = (0..count)
        .map(|index| {
            let values: Vec<String> = (0..dimension)
                .map(|position| format!("{:.4}", 0.1 + position as f32 * 0.01))
                .collect();
            format!(
                r#"{{"object":"embedding","index":{index},"embedding":[{}]}}"#,
                values.join(",")
            )
        })
        .collect();
    format!(
        r#"{{"object":"list","data":[{}],"model":"m","usage":{{"total_tokens":1}}}}"#,
        rows.join(",")
    )
}

/// A well-formed Gemini answer of `count` vectors of `dimension`.
pub fn positional_body(count: usize, dimension: usize) -> String {
    let rows: Vec<String> = (0..count)
        .map(|_| {
            let values: Vec<String> = (0..dimension)
                .map(|position| format!("{:.4}", 0.1 + position as f32 * 0.01))
                .collect();
            format!(r#"{{"values":[{}]}}"#, values.join(","))
        })
        .collect();
    format!(r#"{{"embeddings":[{}]}}"#, rows.join(","))
}

/// A clock that records what it was asked to wait instead of waiting.
///
/// §27: rate-limit behaviour has to be tested without a suite that
/// sleeps for real seconds.
#[derive(Default)]
pub struct RecordingDelay {
    waited: Mutex<Vec<Duration>>,
}

impl RecordingDelay {
    pub fn waits(&self) -> Vec<Duration> {
        self.waited.lock().expect("waits").clone()
    }

    pub fn total(&self) -> Duration {
        self.waits().into_iter().sum()
    }
}

impl noteit_embed::http::Delay for RecordingDelay {
    fn sleep(&self, duration: Duration) {
        self.waited.lock().expect("waits").push(duration);
    }
}

/// Cancellation a test drives.
#[derive(Default)]
pub struct Switch(std::sync::atomic::AtomicBool);

impl Switch {
    pub fn flip(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl noteit_embed::http::Cancelled for Switch {
    fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
