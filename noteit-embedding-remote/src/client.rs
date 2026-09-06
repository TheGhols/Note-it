//! The AF_UNIX half of the boundary: write one frame, read one frame, close.
//!
//! No HTTP crate, no TLS crate and no internet socket appears in this file or
//! in this crate's dependency graph — `scripts/check-embed-boundary` refuses
//! them. What is here is `std::os::unix::net::UnixStream`, which is the family
//! ADR-047 permits and the same mechanism the store's write authority already
//! uses.

use noteit_embed_protocol::{
    read_frame, write_frame, EmbedRequestV1, EmbedResponseV1, ProtocolError, ProviderId, Role,
    WireError, MAX_TEXTS, PROTOCOL_VERSION,
};
use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// How long to wait for the worker to answer one batch.
///
/// Longer than the worker's own operation deadline (120 s) so that a provider
/// running slowly is reported by the worker as a timeout — with its own word
/// for it — rather than cut off here, where the only thing that could be said
/// is that the socket went quiet.
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(150);

/// How long to wait to hand the request over.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// What went wrong between here and the worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientError {
    /// The worker is not there, or the socket broke.
    Unavailable,
    /// The worker answered something this build does not understand.
    Protocol,
    /// The worker answered with one of its own words.
    Reported(WireError),
    /// The caller stopped waiting.
    Cancelled,
}

/// Sends one batch and waits for its answer.
///
/// One connection per request, matching the worker's own model. A connection
/// that carried several would need identifiers to match answers to questions,
/// and the bug that costs — an answer delivered to the wrong request — attaches
/// one paragraph's meaning to another paragraph's identity.
pub fn request(
    socket: &Path,
    provider: ProviderId,
    model: &str,
    role: Role,
    dimension: Option<u32>,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, ClientError> {
    let message = EmbedRequestV1 {
        protocol_version: PROTOCOL_VERSION,
        provider,
        model: model.to_string(),
        role,
        dimension,
        texts: texts.to_vec(),
    };
    // Validated before the socket is even opened. A bug here should be a
    // refusal and not a paid request, which is why the same check runs on both
    // sides of this wire.
    message.validate().map_err(|_| ClientError::Protocol)?;

    let mut stream = UnixStream::connect(socket).map_err(|_| ClientError::Unavailable)?;
    stream
        .set_write_timeout(Some(SEND_TIMEOUT))
        .map_err(|_| ClientError::Unavailable)?;
    stream
        .set_read_timeout(Some(RESPONSE_TIMEOUT))
        .map_err(|_| ClientError::Unavailable)?;

    write_frame(&mut stream, &message).map_err(map_protocol)?;
    // Half-close so the worker sees the end of the question without needing a
    // length it already has.
    let _ = stream.shutdown(std::net::Shutdown::Write);

    let response: EmbedResponseV1 = read_frame(&mut stream).map_err(map_protocol)?;
    // The worker validated on the way out; this validates on the way in. A
    // worker is an input too, and this is the only place a count, a dimension
    // and a finite float can be checked before a vector becomes an index entry.
    response
        .validate(texts.len())
        .map_err(|_| ClientError::Protocol)?;

    match response {
        EmbedResponseV1::Ok { vectors, .. } => Ok(vectors),
        EmbedResponseV1::Err { error, .. } => Err(ClientError::Reported(error)),
    }
}

fn map_protocol(error: ProtocolError) -> ClientError {
    match error {
        ProtocolError::Io | ProtocolError::Truncated => ClientError::Unavailable,
        _ => ClientError::Protocol,
    }
}

/// Splits a job into batches this protocol will carry.
///
/// [`MAX_TEXTS`] per frame. The Core hands over whatever a note produced and
/// this is where it becomes a number of round trips — the Core does not learn
/// a batch size, here or at the vendor (§4).
pub fn batches(count: usize) -> Vec<std::ops::Range<usize>> {
    if count == 0 {
        return Vec::new();
    }
    let mut runs = Vec::with_capacity(count.div_ceil(MAX_TEXTS));
    let mut start = 0;
    while start < count {
        let end = (start + MAX_TEXTS).min(count);
        runs.push(start..end);
        start = end;
    }
    runs
}

/// Whether a socket looks like something worth connecting to.
///
/// Used by the diagnostic, which must answer "is the worker there" without
/// starting one and without sending a request that could cost money
/// (§53).
pub fn socket_is_live(socket: &Path) -> bool {
    match UnixStream::connect(socket) {
        Ok(stream) => {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            true
        }
        Err(error) => {
            !matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) && false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batching_never_exceeds_the_frame_ceiling_and_covers_everything() {
        for count in [
            0usize,
            1,
            MAX_TEXTS - 1,
            MAX_TEXTS,
            MAX_TEXTS + 1,
            3 * MAX_TEXTS + 7,
        ] {
            let runs = batches(count);
            let mut seen = Vec::new();
            for run in &runs {
                assert!(run.len() <= MAX_TEXTS);
                assert!(!run.is_empty());
                seen.extend(run.clone());
            }
            assert_eq!(seen, (0..count).collect::<Vec<_>>(), "count {count}");
        }
    }

    #[test]
    fn a_full_batch_is_one_round_trip() {
        assert_eq!(batches(MAX_TEXTS).len(), 1);
        assert_eq!(batches(MAX_TEXTS + 1).len(), 2);
    }

    #[test]
    fn a_missing_socket_is_unavailable_rather_than_a_panic() {
        let nowhere = std::env::temp_dir().join("noteit-embed-does-not-exist.sock");
        let _ = std::fs::remove_file(&nowhere);
        let error = request(
            &nowhere,
            ProviderId::OpenAi,
            "text-embedding-3-small",
            Role::Query,
            Some(2),
            &["olá".to_string()],
        )
        .unwrap_err();
        assert_eq!(error, ClientError::Unavailable);
        assert!(!socket_is_live(&nowhere));
    }

    #[test]
    fn a_request_that_breaks_a_limit_never_opens_a_socket() {
        let nowhere = std::env::temp_dir().join("noteit-embed-never-opened.sock");
        let error = request(
            &nowhere,
            ProviderId::OpenAi,
            // Refused by `is_model_token`, so `validate` fails before connect.
            "../../etc/passwd",
            Role::Query,
            Some(2),
            &["olá".to_string()],
        )
        .unwrap_err();
        assert_eq!(error, ClientError::Protocol);
    }

    #[test]
    fn an_empty_batch_is_refused_before_the_socket() {
        let nowhere = std::env::temp_dir().join("noteit-embed-never-opened-2.sock");
        assert_eq!(
            request(
                &nowhere,
                ProviderId::Voyage,
                "voyage-4",
                Role::Document,
                None,
                &[]
            )
            .unwrap_err(),
            ClientError::Protocol
        );
    }
}
