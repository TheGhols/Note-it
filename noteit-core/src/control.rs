//! **PRIVATE INTERNAL CONTROL PROTOCOL.**
//!
//! This is how one Note-it process asks the process that holds the store to
//! make a change on its behalf. It is an implementation detail of that
//! handover and nothing else.
//!
//! It is **not** a public interface, not a machine-readable output contract,
//! not an API, and not the JSON surface Phase 4.0F is reserved for. Nothing
//! outside this repository may depend on any of it, and it may change shape
//! in any release without notice or a version bump anywhere visible. That JSON
//! happens to be the simplest thing that works on a private socket says
//! nothing about what Note-it will one day publish on purpose.
//!
//! ## Shape
//!
//! A local `SOCK_STREAM` Unix domain socket, in the runtime directory, `0600`
//! inside a `0700` directory (see [`crate::coordination`]). There is no TCP,
//! no HTTP, no localhost server and no port: the store is a local resource
//! and reaching it over a network is not a feature that was left out, it is a
//! thing this must never grow.
//!
//! ## Framing
//!
//! Every message is a four-byte big-endian length followed by exactly that
//! many bytes of UTF-8 JSON. Nothing ever reads to end-of-stream: a length is
//! read, checked against [`MAX_FRAME_BYTES`], and only then are the bytes
//! taken. A frame that claims to be larger is refused outright and the
//! connection is closed — never truncated and handled anyway, because a
//! truncated `append` is a corrupted note.
//!
//! ## No paths, ever
//!
//! A request carries a note *selector* — a UUID or a hexadecimal prefix of one
//! — text, tags, properties and task references. It cannot carry a filesystem
//! path, because there is no field to put one in and the selector is rejected
//! by [`crate::NoteItCore::resolve_note_id`] if it contains a separator or a
//! `..`. The authority decides which file that is; the client never gets to.
//!
//! ## Request identifiers
//!
//! Every mutation carries a fresh [`Uuid`]. It correlates the answer with the
//! question, and it lets the authority recognise the same request arriving
//! twice — over a reconnect, say — and answer with what it did the first time
//! instead of doing it again. Appending the same paragraph twice because a
//! socket closed at the wrong moment is precisely the failure this exists to
//! make impossible.

use crate::write::{WriteError, WriteOperation, WriteOutcome};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use uuid::Uuid;

/// The version of this private protocol.
///
/// Both ends state it and both ends check it. A mismatch is refused before any
/// field is looked at, let alone acted on: two versions that disagree about
/// what `append` means must never meet halfway.
pub const PROTOCOL_VERSION: u32 = 1;

/// The largest frame either end will accept, in bytes.
///
/// Generous — a note is text, and a megabyte of Markdown is a very long note —
/// and absolute. Anything larger is refused rather than trimmed to fit.
pub const MAX_FRAME_BYTES: u32 = 1024 * 1024;

/// A request to the process that holds the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlRequest {
    pub protocol_version: u32,
    pub request_id: Uuid,
    pub operation: WriteOperation,
}

impl ControlRequest {
    pub fn new(operation: WriteOperation) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: Uuid::new_v4(),
            operation,
        }
    }
}

/// What the authority did, or why it did nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlResponse {
    pub protocol_version: u32,
    pub request_id: Uuid,
    pub result: ControlResult,
}

impl ControlResponse {
    pub fn accepted(request_id: Uuid, outcome: WriteOutcome) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            result: ControlResult::Committed(Box::new(outcome)),
        }
    }

    pub fn refused(request_id: Uuid, error: WriteError) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            result: ControlResult::Refused(Box::new(error)),
        }
    }
}

/// The two things that can have happened, kept apart on purpose.
///
/// `Refused` means nothing was written and repeating the request is safe.
/// `Committed` means the file on disk changed, whatever else may have gone
/// wrong afterwards — a warning about the interface lives *inside* the
/// outcome, never as a failure beside it. Collapsing these two would let a
/// client repeat an append that already happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ControlResult {
    Committed(Box<WriteOutcome>),
    Refused(Box<WriteError>),
}

/// Why a frame could not be exchanged.
#[derive(Debug)]
pub enum FrameError {
    /// The connection ended before a whole frame arrived.
    Closed,
    /// The frame claims a size this protocol does not accept.
    TooLarge(u32),
    /// The bytes are not the message they claim to be.
    Malformed(String),
    /// The socket itself failed.
    Io(io::Error),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("the connection closed mid-frame"),
            Self::TooLarge(size) => {
                write!(
                    formatter,
                    "a frame of {size} bytes exceeds {MAX_FRAME_BYTES}"
                )
            }
            Self::Malformed(detail) => formatter.write_str(detail),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<io::Error> for FrameError {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            Self::Closed
        } else {
            Self::Io(error)
        }
    }
}

/// Writes one length-prefixed JSON frame.
pub fn write_frame<W: Write, T: Serialize>(writer: &mut W, message: &T) -> Result<(), FrameError> {
    let payload = serde_json::to_vec(message)
        .map_err(|error| FrameError::Malformed(format!("could not serialize a frame: {error}")))?;
    let length = u32::try_from(payload.len()).map_err(|_| FrameError::TooLarge(u32::MAX))?;
    if length > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(length));
    }
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

/// Reads one length-prefixed JSON frame.
///
/// The length is checked before a single byte of payload is read, so a
/// declared size of four gigabytes costs four bytes and a refusal rather than
/// an allocation. Nothing here ever reads to the end of the stream.
pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(reader: &mut R) -> Result<T, FrameError> {
    let mut header = [0u8; 4];
    reader.read_exact(&mut header)?;
    let length = u32::from_be_bytes(header);
    if length > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(length));
    }
    if length == 0 {
        return Err(FrameError::Malformed(
            "an empty frame is not a message".into(),
        ));
    }

    let mut payload = vec![0u8; length as usize];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload)
        .map_err(|error| FrameError::Malformed(format!("a frame was not a valid message: {error}")))
}

/// Checks the version a peer stated.
pub fn check_protocol_version(stated: u32) -> Result<(), WriteError> {
    if stated == PROTOCOL_VERSION {
        return Ok(());
    }
    Err(WriteError::InvalidInput {
        detail: format!(
            "this Note-it speaks control protocol {PROTOCOL_VERSION} and the \
             other side speaks {stated}; nothing was written"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::{NoteDraft, NoteMutation, WriteOutcomeKind};

    fn sample_request() -> ControlRequest {
        ControlRequest::new(WriteOperation::MutateNote {
            selector: "8c4f1a2b".to_string(),
            mutation: NoteMutation::Append {
                payload: "acréscimo".to_string(),
            },
        })
    }

    #[test]
    fn a_request_and_its_answer_survive_the_wire() {
        let request = sample_request();
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &request).expect("write");

        let decoded: ControlRequest = read_frame(&mut buffer.as_slice()).expect("read");
        assert_eq!(decoded, request);
    }

    #[test]
    fn two_frames_in_one_stream_are_read_one_at_a_time() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &sample_request()).expect("write");
        write_frame(&mut buffer, &sample_request()).expect("write");

        let mut cursor = buffer.as_slice();
        let _first: ControlRequest = read_frame(&mut cursor).expect("first");
        let _second: ControlRequest = read_frame(&mut cursor).expect("second");
        assert!(cursor.is_empty(), "framing left bytes behind");
    }

    #[test]
    fn a_frame_cut_in_half_is_refused_rather_than_guessed_at() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &sample_request()).expect("write");
        buffer.truncate(buffer.len() - 5);

        let error =
            read_frame::<_, ControlRequest>(&mut buffer.as_slice()).expect_err("incomplete frame");
        assert!(matches!(error, FrameError::Closed), "{error:?}");
    }

    #[test]
    fn an_absurd_length_costs_four_bytes_and_a_refusal() {
        let header = u32::MAX.to_be_bytes();
        let error = read_frame::<_, ControlRequest>(&mut header.as_slice())
            .expect_err("an oversized frame must be refused");
        assert!(matches!(error, FrameError::TooLarge(_)), "{error:?}");
    }

    #[test]
    fn a_frame_at_the_limit_is_allowed_and_one_past_it_is_not() {
        let mut oversized = Vec::new();
        oversized.extend_from_slice(&(MAX_FRAME_BYTES + 1).to_be_bytes());
        let error = read_frame::<_, ControlRequest>(&mut oversized.as_slice())
            .expect_err("one byte past the limit");
        assert!(matches!(error, FrameError::TooLarge(_)), "{error:?}");
    }

    #[test]
    fn a_frame_that_is_not_a_message_is_refused_without_a_panic() {
        let payload = b"{ not json";
        let mut buffer = (payload.len() as u32).to_be_bytes().to_vec();
        buffer.extend_from_slice(payload);

        let error =
            read_frame::<_, ControlRequest>(&mut buffer.as_slice()).expect_err("invalid json");
        assert!(matches!(error, FrameError::Malformed(_)), "{error:?}");
    }

    #[test]
    fn a_request_missing_its_operation_is_refused() {
        let payload = serde_json::json!({
            "protocol_version": 1,
            "request_id": Uuid::new_v4(),
        })
        .to_string();
        let mut buffer = (payload.len() as u32).to_be_bytes().to_vec();
        buffer.extend_from_slice(payload.as_bytes());

        let error = read_frame::<_, ControlRequest>(&mut buffer.as_slice())
            .expect_err("a request without an operation");
        assert!(matches!(error, FrameError::Malformed(_)), "{error:?}");
    }

    #[test]
    fn an_unknown_operation_is_refused() {
        let payload = serde_json::json!({
            "protocol_version": 1,
            "request_id": Uuid::new_v4(),
            "operation": { "operation": "detonate_store" },
        })
        .to_string();
        let mut buffer = (payload.len() as u32).to_be_bytes().to_vec();
        buffer.extend_from_slice(payload.as_bytes());

        let error = read_frame::<_, ControlRequest>(&mut buffer.as_slice())
            .expect_err("an operation nothing implements");
        assert!(matches!(error, FrameError::Malformed(_)), "{error:?}");
    }

    #[test]
    fn a_zero_length_frame_is_not_a_message() {
        let header = 0u32.to_be_bytes();
        let error =
            read_frame::<_, ControlRequest>(&mut header.as_slice()).expect_err("empty frame");
        assert!(matches!(error, FrameError::Malformed(_)), "{error:?}");
    }

    #[test]
    fn a_version_this_build_does_not_speak_is_refused_before_anything_is_written() {
        assert!(check_protocol_version(PROTOCOL_VERSION).is_ok());
        let error = check_protocol_version(PROTOCOL_VERSION + 1).expect_err("mismatch");
        assert!(
            matches!(error, WriteError::InvalidInput { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn committed_and_refused_are_never_the_same_answer() {
        let id = Uuid::new_v4();
        let note = Uuid::new_v4();
        let committed = ControlResponse::accepted(
            id,
            WriteOutcome::new(note, WriteOutcomeKind::ContentAppended, true),
        );
        let refused = ControlResponse::refused(
            id,
            WriteError::StaleTaskRef {
                task_ref: "a71bc920".into(),
            },
        );
        assert_ne!(committed.result, refused.result);

        let mut buffer = Vec::new();
        write_frame(&mut buffer, &committed).expect("write");
        let decoded: ControlResponse = read_frame(&mut buffer.as_slice()).expect("read");
        assert_eq!(decoded, committed);
    }

    #[test]
    fn a_creation_request_carries_no_path_anywhere() {
        let request = ControlRequest::new(WriteOperation::CreateNote {
            draft: NoteDraft {
                content: "# Choque".into(),
                tags: vec!["Medicina".into()],
                properties: Vec::new(),
            },
        });
        let json = serde_json::to_string(&request).expect("serialize");
        assert!(!json.contains('/'), "a path reached the wire: {json}");
        assert!(!json.contains("notes"), "{json}");
    }
}
