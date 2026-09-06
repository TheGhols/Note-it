//! The one HTTP client in this product, and every knob on it turned
//! deliberately.
//!
//! ## What is configured, and why each one
//!
//! | setting | value | because |
//! | --- | --- | --- |
//! | TLS | rustls + `ring`, certificates validated | §65. There is no `danger_accept_invalid_certs` in this file and `scripts/check-embed-boundary` refuses one anywhere in the workspace |
//! | redirects | **zero** | §64. An embeddings API has no reason to redirect, and a 30x is how an `Authorization` header and a note's text reach a host nobody pinned |
//! | proxy | **none**, and the environment is not consulted | §63. `ureq` reads `ALL_PROXY`, `HTTPS_PROXY` and `HTTP_PROXY` when it builds a default agent. A proxy sees the note text and terminates the TLS this module exists to validate, so an ambient variable must not be able to choose one |
//! | connect timeout | 10 s | a host that is not answering is not going to |
//! | per-call timeout | 60 s | one HTTP exchange, redirects excluded because there are none |
//! | whole operation | 120 s | the ceiling across every retry, so a `Retry-After` cannot extend a request indefinitely |
//! | body ceiling | 8 MiB, explicit | §58. `ureq`'s own default is 10 MiB; this states its own so the number is in the file that depends on it |
//! | status handling | ours | `http_status_as_error(false)`, because a 429 is a policy decision here and not an error string |
//! | HTTP version | 1.1 only | `ureq` implements no HTTP/2 or /3, so `h2`, `h3` and `quinn` are absent from the graph rather than merely unused |
//! | compression | off | the `gzip` feature is not enabled: a decompressed size is not the number the ceiling above is applied to |
//! | cookies | off | an embeddings API has no session |
//!
//! ## What this module refuses to say
//!
//! A vendor's error body, a request ID, a header, an HTTP status line and a
//! TLS failure are all text nobody here controls. None of them is returned,
//! logged or formatted: every path out of this module ends in a
//! [`WireError`], which is a closed set of Note-it's own words
//! (§25, §51, §62).

use crate::credential::Credential;
use crate::endpoint;
use noteit_embed_protocol::{ProviderId, WireError};
use std::time::{Duration, Instant};

/// How long to wait for a connection.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long one HTTP exchange may take.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(60);
/// How long the whole operation may take, retries included.
pub const OPERATION_DEADLINE: Duration = Duration::from_secs(120);
/// The most response body this process will read, in bytes.
pub const MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;

/// How many times a retryable status is retried before giving up.
///
/// Three attempts in total, never an unbounded loop (§27). A provider
/// that is rate limiting is telling the truth about its own capacity, and the
/// right answer to being told so four times is to degrade rather than to keep
/// asking.
pub const MAX_ATTEMPTS: u32 = 3;

/// The base of the exponential backoff.
pub const BACKOFF_BASE: Duration = Duration::from_millis(500);

/// The most this process will wait between two attempts, whatever `Retry-After`
/// says.
///
/// A vendor's header is a hint and not an instruction: a `Retry-After: 3600`
/// on a note-taking application's query is not a thing to obey, it is a thing
/// to stop for. Capped rather than ignored, because respecting it up to a
/// bound is what keeps this a good client.
pub const MAX_BACKOFF: Duration = Duration::from_secs(8);

/// One HTTP exchange, already reduced to what the caller may know about it.
pub struct HttpAnswer {
    pub status: u16,
    pub body: Vec<u8>,
}

/// A request this module will make.
pub struct HttpRequest<'a> {
    pub provider: ProviderId,
    /// Appended to the provider's pinned base. Built by the adapter from
    /// constants and a model name that `is_model_token` has already accepted.
    pub path: String,
    pub body: Vec<u8>,
    /// The name and value of the one authentication header this provider uses.
    pub auth_header: &'static str,
    pub credential: &'a Credential,
    /// Whether the value is `Bearer <key>` rather than the bare key.
    pub bearer: bool,
}

/// Whether an operation has been asked to stop.
///
/// A trait object rather than a channel, so the caller decides what
/// cancellation means and this module only has to ask (§57).
pub trait Cancelled {
    fn cancelled(&self) -> bool;
}

/// Never cancelled. The shape of a request nobody is waiting on.
pub struct NeverCancelled;
impl Cancelled for NeverCancelled {
    fn cancelled(&self) -> bool {
        false
    }
}

/// How long to sleep between attempts. Injected so the suite does not.
///
/// §27 asks for rate-limit behaviour tested without a suite that sleeps for
/// real seconds. This is how: the policy computes a duration and hands it to a
/// clock, and the test's clock records it instead of waiting.
pub trait Delay {
    fn sleep(&self, duration: Duration);
}

/// The real one.
pub struct RealDelay;
impl Delay for RealDelay {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// The agent, built once with every setting stated.
pub struct HttpClient {
    agent: ureq::Agent,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            // Zero. Not "a small number": an embeddings endpoint that answers
            // 302 is an embeddings endpoint that has been taken over, and
            // following it would send the `Authorization` header and the
            // note's text to whatever the `Location` said.
            .max_redirects(0)
            // The environment is not asked. `ureq`'s default agent reads
            // ALL_PROXY / HTTPS_PROXY / HTTP_PROXY; a proxy chosen by an
            // ambient variable would see the note text in the clear and
            // terminate the TLS validated above.
            .proxy(None)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_per_call(Some(CALL_TIMEOUT))
            .timeout_global(Some(OPERATION_DEADLINE))
            // A status is data here. `true` would turn a 429 into an error
            // whose text this module would then have to refuse to publish.
            .http_status_as_error(false)
            .user_agent("noteit-embed")
            .build();
        Self {
            agent: config.into(),
        }
    }

    /// Makes one request, retrying only what is worth retrying.
    ///
    /// Retried: 429, 500, 502, 503, 504, and a connection that failed before
    /// an answer. Not retried: 400, 401, 403, 404, 408 and every other 4xx,
    /// because sending the same bad request again is asking a question already
    /// answered — and because a wrong key retried three times is a wrong key
    /// reported to the vendor three times.
    pub fn send(
        &self,
        request: &HttpRequest<'_>,
        cancel: &dyn Cancelled,
        delay: &dyn Delay,
    ) -> Result<HttpAnswer, WireError> {
        let started = Instant::now();
        let url = format!("{}{}", endpoint::base_for(request.provider), request.path);
        let mut attempt = 0u32;
        loop {
            if cancel.cancelled() {
                return Err(WireError::Cancelled);
            }
            if started.elapsed() >= OPERATION_DEADLINE {
                return Err(WireError::Timeout);
            }
            attempt += 1;

            let header_value = if request.bearer {
                format!("Bearer {}", request.credential.expose())
            } else {
                request.credential.expose().to_string()
            };
            let outcome = self
                .agent
                .post(&url)
                .header("content-type", "application/json")
                .header(request.auth_header, header_value.as_str())
                .header("accept", "application/json")
                .send(&request.body[..]);

            match outcome {
                Ok(mut response) => {
                    let status = response.status().as_u16();
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|value| value.to_str().ok())
                        .and_then(parse_retry_after);
                    // Read under an explicit ceiling. A body larger than this
                    // is refused rather than truncated-and-parsed: half a JSON
                    // document is not a smaller answer, it is a different one.
                    let body = response
                        .body_mut()
                        .with_config()
                        .limit(MAX_BODY_BYTES)
                        .read_to_vec()
                        .map_err(|_| WireError::InvalidResponse)?;

                    if !is_retryable(status) || attempt >= MAX_ATTEMPTS {
                        return Ok(HttpAnswer { status, body });
                    }
                    let wait = backoff(attempt, retry_after);
                    if started.elapsed() + wait >= OPERATION_DEADLINE {
                        return Ok(HttpAnswer { status, body });
                    }
                    delay.sleep(wait);
                }
                Err(error) => {
                    let kind = classify_transport(&error);
                    if kind != WireError::Unavailable || attempt >= MAX_ATTEMPTS {
                        return Err(kind);
                    }
                    let wait = backoff(attempt, None);
                    if started.elapsed() + wait >= OPERATION_DEADLINE {
                        return Err(kind);
                    }
                    delay.sleep(wait);
                }
            }
        }
    }
}

/// Which statuses are worth asking again about.
pub fn is_retryable(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// How long to wait before attempt `attempt + 1`.
///
/// `Retry-After` wins when the vendor sent a usable one, capped at
/// [`MAX_BACKOFF`]; otherwise exponential from [`BACKOFF_BASE`], also capped.
/// The jitter is deterministic in the sense that matters — it is derived from
/// the process id and the attempt rather than from a random source — so two
/// Note-it processes that hit the same limit do not retry in lockstep, and a
/// test can still predict the range.
pub fn backoff(attempt: u32, retry_after: Option<Duration>) -> Duration {
    let base = match retry_after {
        Some(hint) => hint.min(MAX_BACKOFF),
        None => {
            let exponent = attempt.saturating_sub(1).min(4);
            BACKOFF_BASE
                .saturating_mul(1u32 << exponent)
                .min(MAX_BACKOFF)
        }
    };
    let spread = (std::process::id() as u64).wrapping_mul(attempt as u64 + 1) % 250;
    base.saturating_add(Duration::from_millis(spread))
        .min(MAX_BACKOFF + Duration::from_millis(250))
}

/// `Retry-After`, in the one form worth trusting.
///
/// The header may be a number of seconds or an HTTP date. Only the number is
/// honoured: a date requires agreeing with the vendor about the current time,
/// and a clock that is wrong turns "wait three seconds" into "wait until
/// Tuesday". An unparseable header falls back to the exponential schedule,
/// which is a schedule and not a refusal.
pub fn parse_retry_after(raw: &str) -> Option<Duration> {
    let seconds: u64 = raw.trim().parse().ok()?;
    Some(Duration::from_secs(seconds.min(3600)))
}

/// A transport failure, reduced to one of Note-it's words.
///
/// The library's own message is never read and never returned. What is used is
/// its *kind*, which is a small enum the library defines, and the mapping is
/// here so there is exactly one place where a vendor's failure becomes a word
/// of ours.
fn classify_transport(error: &ureq::Error) -> WireError {
    match error {
        ureq::Error::Timeout(_) => WireError::Timeout,
        // A redirect that was not followed, because the policy is zero. Not a
        // transport problem: it means the pinned endpoint answered with a
        // pointer somewhere else, which is exactly the case §64 exists for.
        ureq::Error::TooManyRedirects => WireError::InvalidResponse,
        _ => WireError::Unavailable,
    }
}

/// The public meaning of an HTTP status from an embeddings API.
///
/// One table, used by all three adapters, so "401 means authentication" is not
/// re-decided per vendor (§26).
pub fn status_to_error(status: u16) -> Option<WireError> {
    match status {
        200..=299 => None,
        400 => Some(WireError::InvalidResponse),
        401 | 403 => Some(WireError::Authentication),
        404 => Some(WireError::ModelUnavailable),
        408 => Some(WireError::Timeout),
        429 => Some(WireError::RateLimited),
        // Reached only after the retry policy above gave up, so this is the
        // vendor still being unavailable rather than a first refusal.
        500 | 502 | 503 | 504 => Some(WireError::RateLimited),
        _ => Some(WireError::Unavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_statuses_worth_asking_again_about_are_retried() {
        for status in [429, 500, 502, 503, 504] {
            assert!(is_retryable(status), "{status} should be retried");
        }
        for status in [200, 201, 400, 401, 403, 404, 408, 409, 418, 422, 501] {
            assert!(!is_retryable(status), "{status} should not be retried");
        }
    }

    #[test]
    fn a_bad_key_is_never_retried() {
        // Worth its own test: retrying a 401 tells the vendor three times that
        // this key is wrong, which is how an account gets locked.
        assert!(!is_retryable(401));
        assert!(!is_retryable(403));
        assert_eq!(status_to_error(401), Some(WireError::Authentication));
        assert_eq!(status_to_error(403), Some(WireError::Authentication));
    }

    #[test]
    fn every_status_maps_to_one_of_our_words() {
        assert_eq!(status_to_error(200), None);
        assert_eq!(status_to_error(299), None);
        assert_eq!(status_to_error(400), Some(WireError::InvalidResponse));
        assert_eq!(status_to_error(404), Some(WireError::ModelUnavailable));
        assert_eq!(status_to_error(408), Some(WireError::Timeout));
        assert_eq!(status_to_error(429), Some(WireError::RateLimited));
        assert_eq!(status_to_error(500), Some(WireError::RateLimited));
        assert_eq!(status_to_error(503), Some(WireError::RateLimited));
        assert_eq!(status_to_error(418), Some(WireError::Unavailable));
    }

    #[test]
    fn the_backoff_grows_and_stops_growing() {
        let ceiling = MAX_BACKOFF + Duration::from_millis(250);
        let mut previous = Duration::ZERO;
        for attempt in 1..=6 {
            let wait = backoff(attempt, None);
            assert!(wait <= ceiling, "attempt {attempt} waits {wait:?}");
            if attempt <= 4 {
                assert!(wait >= previous, "attempt {attempt} went backwards");
            }
            previous = wait;
        }
    }

    #[test]
    fn retry_after_is_respected_up_to_the_ceiling() {
        let short = backoff(1, Some(Duration::from_secs(2)));
        assert!(short >= Duration::from_secs(2));
        assert!(short <= Duration::from_secs(2) + Duration::from_millis(250));

        // An hour is a real thing for a vendor to say and not a thing to do.
        let absurd = backoff(1, Some(Duration::from_secs(3600)));
        assert!(absurd <= MAX_BACKOFF + Duration::from_millis(250));
    }

    #[test]
    fn retry_after_is_read_as_seconds_and_never_as_a_date() {
        assert_eq!(parse_retry_after("3"), Some(Duration::from_secs(3)));
        assert_eq!(parse_retry_after("  7 "), Some(Duration::from_secs(7)));
        assert_eq!(parse_retry_after("0"), Some(Duration::ZERO));
        // Capped on the way in as well as on the way out.
        assert_eq!(parse_retry_after("99999"), Some(Duration::from_secs(3600)));
        // A date is not parsed. Agreeing with a vendor about the current time
        // is a dependency this does not need.
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after("soon"), None);
        assert_eq!(parse_retry_after("-1"), None);
    }

    #[test]
    fn the_timeouts_are_ordered_the_way_a_deadline_has_to_be() {
        assert!(CONNECT_TIMEOUT < CALL_TIMEOUT);
        assert!(CALL_TIMEOUT < OPERATION_DEADLINE);
        // Three attempts, each at most one call, plus the waits between them,
        // must be able to fit inside the deadline for the retry policy to be
        // reachable at all rather than cut off by the global timeout.
        let worst_waits = MAX_BACKOFF * (MAX_ATTEMPTS - 1);
        assert!(worst_waits < OPERATION_DEADLINE);
    }
}
