//! What crosses the process boundary between Note-it and `noteit-embed`, and
//! nothing else.
//!
//! The remote embedding worker exists so that exactly one process in this
//! product has an HTTP client and exactly one sees a provider credential
//! (`docs/semantic-retrieval.md` §9). A boundary is only worth the process it
//! costs if what crosses it is small, versioned and bounded, so this crate is
//! three things and no more: the two messages, their limits, and the framing
//! that reads them without trusting the sender about how much memory to
//! allocate.
//!
//! ## What a request carries, and why it carries so little
//!
//! A note identifier, a path, a revision, a front matter or a `NoteDocument`
//! would all be *useful* to a worker that wanted to be helpful, and every one
//! of them is refused: the worker embeds text, and text is the only thing it
//! is given. There is no field a store root could go in, so "the worker cannot
//! read your notes" needs no runtime check on this side of the wire —
//! §19 of the specification, made structural.
//!
//! The destination is not here either. A request names a *provider* — one of
//! three enum variants — and never a URL, a host, a port or a header. Which
//! endpoint that provider means is a constant compiled into the worker, so a
//! caller who wanted to point this at `169.254.169.254` has nowhere to write
//! it down.
//!
//! ## Fail closed, on every axis
//!
//! Both messages carry an explicit `protocol_version` and both are
//! `deny_unknown_fields`. A frame is refused before it is allocated for, a
//! text is refused before it is embedded, and a vector is refused before it is
//! returned. Every limit below is checked on **both** sides: the client checks
//! before sending because a bug here should not become a bill, and the worker
//! checks on receipt because the client is still an input.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// The only version this build speaks.
///
/// Named in every message rather than negotiated once per connection, because
/// a connection carries exactly one request: there is no session state for a
/// negotiation to live in, and a version in the message is a version that
/// cannot be forgotten halfway through.
pub const PROTOCOL_VERSION: u32 = 1;

// ------------------------------------------------------------------ limits

/// The largest frame either side will read, in bytes.
///
/// The length prefix is read first and compared against this **before** a
/// buffer is reserved, so a sender that claims four gigabytes gets a refusal
/// rather than an allocation. That ordering is the whole point of the
/// constant; the value itself is only generous enough for the largest legal
/// response, which is [`MAX_TEXTS`] × [`MAX_DIMENSION`] floats plus the JSON
/// around them.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// The most texts one request may carry.
///
/// Smaller than every provider's own batch ceiling — OpenAI documents none,
/// Voyage documents 1 000 — because this limit is not about what the vendor
/// accepts. It bounds the frame, it bounds the memory a single hostile client
/// can make the worker hold, and it bounds how much work one cancellation
/// throws away. A provider whose batch limit is larger is partitioned by the
/// worker's own adapter, which is where vendor-specific numbers belong (§29).
pub const MAX_TEXTS: usize = 64;

/// The most bytes one text may carry.
///
/// The chunker caps a chunk at 800 *characters* and the engine caps a query,
/// so this is a backstop against a caller that is not the chunker rather than
/// a routine cut. Bytes and not characters, because the frame is bytes.
pub const MAX_TEXT_BYTES: usize = 32 * 1024;

/// The most bytes all the texts of one request may carry together.
///
/// [`MAX_TEXTS`] × [`MAX_TEXT_BYTES`] would be two megabytes of text, which is
/// far more than sixty-four paragraphs can be. The tighter total is what
/// actually bounds a request, and it is checked as well as the per-text limit
/// because a request may be one enormous text or many small ones.
pub const MAX_TOTAL_TEXT_BYTES: usize = 512 * 1024;

/// The largest vector dimension this protocol will carry.
///
/// Above every dimension any of the three providers offers — OpenAI's largest
/// is 3 072, Gemini's is 3 072, Voyage's is 2 048 — and finite, which is the
/// property that matters: a response that claims a dimension is refused before
/// anything is sized from it.
pub const MAX_DIMENSION: usize = 4096;

/// The longest a model identifier may be.
pub const MAX_MODEL_CHARS: usize = 64;

// ---------------------------------------------------------------- provider

/// Which vendor a request is for.
///
/// **An enum and never a URL, and that is the SSRF control.** Gemini puts the
/// model name in the request *path* and every provider puts the host in the
/// endpoint, so a protocol that carried either would be a general-purpose HTTP
/// client with extra steps. Three variants means the set of reachable hosts is
/// the set of constants in the worker, decided at compile time, and a caller
/// who wants a fourth has to ship a new worker.
///
/// There is no `Anthropic`. The official documentation says, verbatim,
/// *"Anthropic does not offer its own embedding model"*, and a variant for a
/// product that does not exist would be an API nobody wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    #[serde(rename = "openai")]
    OpenAi,
    Gemini,
    Voyage,
}

impl ProviderId {
    /// The stable identifier this provider is known by everywhere.
    ///
    /// A constant, never a translated label: it becomes the `provider` field
    /// of an `EmbeddingSpaceId`, and therefore part of what decides whether
    /// two vectors may be compared. A name that changed with the interface
    /// language would silently split one space into two.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
            Self::Voyage => "voyage",
        }
    }

    /// Every provider this build knows, in a stable order.
    pub const ALL: [Self; 3] = [Self::OpenAi, Self::Gemini, Self::Voyage];

    /// Parses one of [`Self::ALL`], and nothing else.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == text)
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Whether a text is a note's or a question.
///
/// On the wire because the vendors need it and the caller must not have to
/// know that they do: Voyage takes `input_type`, `gemini-embedding-001` takes
/// `taskType`, OpenAI takes neither. Which of those a role becomes is the
/// worker's adapter's business (§30).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Document,
    Query,
}

// ----------------------------------------------------------------- request

/// One batch of texts to embed.
///
/// `deny_unknown_fields` is deliberate and is the fail-closed half of §70: a
/// field this build does not know is a message from a build that does not
/// share this contract, and guessing what it meant is how two versions come to
/// disagree in silence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbedRequestV1 {
    pub protocol_version: u32,
    pub provider: ProviderId,
    /// The model, as the provider names it. Validated as a narrow token
    /// before it can reach a URL — see [`EmbedRequestV1::validate`].
    pub model: String,
    pub role: Role,
    /// The dimension asked for, when the provider supports asking.
    ///
    /// `None` means "the model's own", which is the only honest way to say it:
    /// a client that guessed a default would put a number into the
    /// `EmbeddingSpaceId` that the vendor never promised.
    #[serde(default)]
    pub dimension: Option<u32>,
    pub texts: Vec<String>,
}

/// Why a message is not one.
///
/// Every variant is a *fact about the frame*, never a sentence from a library,
/// and none of them carries the offending bytes. Echoing what was refused is
/// the leak `scripts/check-mcp-boundary` already forbids one layer up, and a
/// protocol error is exactly the place somebody would be tempted to be
/// helpful about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    /// The length prefix could not be read, or the stream ended inside a frame.
    Truncated,
    /// The length prefix is larger than [`MAX_FRAME_BYTES`], or zero.
    FrameTooLarge,
    /// The frame is not the JSON this version defines — bad syntax, a missing
    /// field, an unknown field, an unknown enum variant, or invalid UTF-8.
    Malformed,
    /// The message says a version this build does not speak.
    UnsupportedVersion,
    /// A limit in this module was exceeded: too many texts, a text too long,
    /// too much text in total, a dimension out of range, or a model name
    /// outside the token alphabet.
    LimitExceeded,
    /// The socket failed underneath the framing.
    Io,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Truncated => "o quadro terminou antes do fim",
            Self::FrameTooLarge => "o quadro declara um tamanho fora do limite",
            Self::Malformed => "o quadro não tem a forma que esta versão define",
            Self::UnsupportedVersion => "a mensagem usa uma versão de protocolo desconhecida",
            Self::LimitExceeded => "a mensagem excede um limite do protocolo",
            Self::Io => "o canal falhou",
        })
    }
}

impl std::error::Error for ProtocolError {}

impl EmbedRequestV1 {
    /// Everything that must be true of a request before anything acts on it.
    ///
    /// Called by the client before sending and by the worker on receipt. Twice
    /// on purpose: the client's call turns a bug into a refusal instead of a
    /// bill, and the worker's call is the one that matters, because a client
    /// is an input like any other.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion);
        }
        if !is_model_token(&self.model) {
            return Err(ProtocolError::LimitExceeded);
        }
        if self.texts.is_empty() || self.texts.len() > MAX_TEXTS {
            return Err(ProtocolError::LimitExceeded);
        }
        let mut total = 0usize;
        for text in &self.texts {
            if text.len() > MAX_TEXT_BYTES {
                return Err(ProtocolError::LimitExceeded);
            }
            total = total.saturating_add(text.len());
        }
        if total > MAX_TOTAL_TEXT_BYTES {
            return Err(ProtocolError::LimitExceeded);
        }
        if let Some(dimension) = self.dimension {
            if dimension == 0 || dimension as usize > MAX_DIMENSION {
                return Err(ProtocolError::LimitExceeded);
            }
        }
        Ok(())
    }
}

/// Whether a string is a model identifier this protocol will carry.
///
/// **This is a security control and not a tidiness rule.** Gemini's endpoint is
/// `…/v1beta/models/{model}:embedContent`, so the model name is part of a URL
/// path. A name containing `/`, `.` `.`, `?`, `#`, `@` or a percent escape
/// could steer a request off the pinned endpoint while every "the host is a
/// constant" claim above stayed technically true. The alphabet below has no
/// character with meaning in a URL, so the pinned host and path survive
/// concatenation.
pub fn is_model_token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_MODEL_CHARS
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        // A leading dot, or two in a row, is the shape of a path traversal even
        // though no separator is in the alphabet. Refused so that the reason a
        // name is safe never has to be an argument about what a URL parser does.
        && !text.starts_with('.')
        && !text.contains("..")
}

// ---------------------------------------------------------------- response

/// What one request produced.
///
/// Two shapes and no third. In particular there is no "partially succeeded":
/// the count of vectors is the only thing that says which vector belongs to
/// which text, so a batch that came back short has not come back at all
/// (§26, §84).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum EmbedResponseV1 {
    Ok {
        protocol_version: u32,
        /// The dimension the vectors actually have, echoed so the client can
        /// refuse a space it did not ask for rather than index it.
        dimension: u32,
        /// One per text of the request, in the order the texts were given.
        ///
        /// Reordering by the provider's own index — OpenAI and Voyage return
        /// one, Gemini does not — happens in the worker, which refuses the
        /// whole response if an index is missing, duplicated or out of range.
        /// By the time a response is on this wire, position *is* the answer.
        vectors: Vec<Vec<f32>>,
    },
    Err {
        protocol_version: u32,
        error: WireError,
    },
}

/// What went wrong, as a closed set of facts.
///
/// The vendor does not write Note-it's error messages, and this enum is where
/// that rule becomes structural: an HTTP status, a request ID, a JSON body and
/// a TLS failure all arrive at the worker as text nobody controls, and none of
/// them crosses this boundary. What crosses is one of these words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireError {
    /// The provider could not be reached at all: DNS, connect, TLS, or a
    /// connection that died mid-request.
    Unavailable,
    /// A timeout fired — connect, request or the whole operation.
    Timeout,
    /// 401 or 403.
    Authentication,
    /// No credential was found for this provider.
    CredentialMissing,
    /// 404, or the vendor saying the model is not available.
    ModelUnavailable,
    /// 429, or 503, after the retry policy gave up.
    RateLimited,
    /// The provider answered, and the answer is not one: not JSON, truncated
    /// JSON, the wrong schema, a missing vector, a duplicated index, a vector
    /// that is not finite, or the wrong number of them.
    InvalidResponse,
    /// The vectors came back a length the request did not ask for.
    DimensionMismatch,
    /// The operation was abandoned before it finished.
    Cancelled,
    /// The request was refused by the protocol itself.
    Protocol,
    /// The worker does not implement what was asked for.
    Unsupported,
}

impl EmbedResponseV1 {
    /// A refusal in this version.
    pub fn error(error: WireError) -> Self {
        Self::Err {
            protocol_version: PROTOCOL_VERSION,
            error,
        }
    }

    /// An answer in this version, validated on the way out.
    ///
    /// The worker calls this rather than building the variant directly, so
    /// "no non-finite float ever reaches the wire" is a property of the one
    /// constructor instead of a rule every call site has to remember. JSON
    /// cannot represent `NaN` or an infinity anyway — `serde_json` writes
    /// `null` — so without this the failure would surface as a parse error on
    /// the far side, which is fail-closed but says the wrong thing.
    pub fn answer(dimension: u32, vectors: Vec<Vec<f32>>) -> Result<Self, WireError> {
        if dimension == 0 || dimension as usize > MAX_DIMENSION {
            return Err(WireError::DimensionMismatch);
        }
        if vectors.is_empty() || vectors.len() > MAX_TEXTS {
            return Err(WireError::InvalidResponse);
        }
        for vector in &vectors {
            if vector.len() != dimension as usize {
                return Err(WireError::DimensionMismatch);
            }
            if vector.iter().any(|value| !value.is_finite()) {
                return Err(WireError::InvalidResponse);
            }
        }
        Ok(Self::Ok {
            protocol_version: PROTOCOL_VERSION,
            dimension,
            vectors,
        })
    }

    /// Everything that must be true of a response before anything indexes it.
    ///
    /// Symmetrical with [`EmbedRequestV1::validate`] and for the same reason:
    /// the worker validates on the way out and the client validates on the way
    /// in, because a worker is an input too. `expected` is how many texts were
    /// sent, so "fewer vectors than inputs" and "more vectors than inputs" are
    /// both caught here rather than by whichever loop happens to zip them.
    pub fn validate(&self, expected: usize) -> Result<(), ProtocolError> {
        match self {
            Self::Ok {
                protocol_version,
                dimension,
                vectors,
            } => {
                if *protocol_version != PROTOCOL_VERSION {
                    return Err(ProtocolError::UnsupportedVersion);
                }
                if *dimension == 0 || *dimension as usize > MAX_DIMENSION {
                    return Err(ProtocolError::LimitExceeded);
                }
                if vectors.len() != expected {
                    return Err(ProtocolError::Malformed);
                }
                for vector in vectors {
                    if vector.len() != *dimension as usize {
                        return Err(ProtocolError::Malformed);
                    }
                    if vector.iter().any(|value| !value.is_finite()) {
                        return Err(ProtocolError::Malformed);
                    }
                }
                Ok(())
            }
            Self::Err {
                protocol_version, ..
            } => {
                if *protocol_version != PROTOCOL_VERSION {
                    return Err(ProtocolError::UnsupportedVersion);
                }
                Ok(())
            }
        }
    }
}

// ----------------------------------------------------------------- framing

/// Writes one length-prefixed frame.
///
/// Four bytes of big-endian length, then that many bytes of JSON. The prefix
/// is written after the body is encoded, so a length that does not match its
/// body cannot be produced here at all.
pub fn write_frame<W: Write, T: Serialize>(
    writer: &mut W,
    message: &T,
) -> Result<(), ProtocolError> {
    let body = serde_json::to_vec(message).map_err(|_| ProtocolError::Malformed)?;
    if body.is_empty() || body.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge);
    }
    let length = u32::try_from(body.len()).map_err(|_| ProtocolError::FrameTooLarge)?;
    writer
        .write_all(&length.to_be_bytes())
        .map_err(|_| ProtocolError::Io)?;
    writer.write_all(&body).map_err(|_| ProtocolError::Io)?;
    writer.flush().map_err(|_| ProtocolError::Io)?;
    Ok(())
}

/// Reads one length-prefixed frame.
///
/// **The order of the two checks below is the contract.** The length is read,
/// then compared against [`MAX_FRAME_BYTES`], and only then is a buffer
/// reserved. A reader that allocated first would let a four-byte message ask
/// for four gigabytes, which is the entire reason a length-prefixed protocol
/// needs a limit rather than a comment.
///
/// Trailing bytes after the frame are not this function's business and are not
/// read: a connection carries exactly one request and one response, so
/// anything after a complete frame is refused by the caller closing the
/// connection rather than by an ambiguity here.
pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<T, ProtocolError> {
    let mut prefix = [0u8; 4];
    read_exact(reader, &mut prefix)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge);
    }
    let mut body = vec![0u8; length];
    read_exact(reader, &mut body)?;
    serde_json::from_slice(&body).map_err(|_| ProtocolError::Malformed)
}

/// `Read::read_exact`, with the end of the stream told apart from a fault.
fn read_exact<R: Read>(reader: &mut R, buffer: &mut [u8]) -> Result<(), ProtocolError> {
    match reader.read_exact(buffer) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Err(ProtocolError::Truncated),
        Err(_) => Err(ProtocolError::Io),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> EmbedRequestV1 {
        EmbedRequestV1 {
            protocol_version: PROTOCOL_VERSION,
            provider: ProviderId::OpenAi,
            model: "text-embedding-3-small".to_string(),
            role: Role::Document,
            dimension: Some(1536),
            texts: vec!["uma nota".to_string()],
        }
    }

    #[test]
    fn a_request_survives_a_round_trip() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &request()).expect("write");
        let back: EmbedRequestV1 = read_frame(&mut buffer.as_slice()).expect("read");
        assert_eq!(back, request());
    }

    #[test]
    fn a_response_survives_a_round_trip() {
        let answer = EmbedResponseV1::answer(2, vec![vec![0.5, 0.5]]).expect("answer");
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &answer).expect("write");
        let back: EmbedResponseV1 = read_frame(&mut buffer.as_slice()).expect("read");
        assert_eq!(back, answer);
        back.validate(1).expect("valid");
    }

    #[test]
    fn a_length_beyond_the_ceiling_is_refused_before_anything_is_allocated() {
        // Four bytes claiming four gigabytes, and nothing after them. A reader
        // that reserved first would ask the allocator for the whole number.
        let framed: Vec<u8> = u32::MAX.to_be_bytes().to_vec();
        let error = read_frame::<_, EmbedRequestV1>(&mut framed.as_slice()).unwrap_err();
        assert_eq!(error, ProtocolError::FrameTooLarge);
    }

    #[test]
    fn a_zero_length_frame_is_refused() {
        let framed = 0u32.to_be_bytes().to_vec();
        assert_eq!(
            read_frame::<_, EmbedRequestV1>(&mut framed.as_slice()).unwrap_err(),
            ProtocolError::FrameTooLarge
        );
    }

    #[test]
    fn a_truncated_frame_is_refused() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &request()).expect("write");
        buffer.truncate(buffer.len() - 3);
        assert_eq!(
            read_frame::<_, EmbedRequestV1>(&mut buffer.as_slice()).unwrap_err(),
            ProtocolError::Truncated
        );
    }

    #[test]
    fn a_missing_prefix_is_refused() {
        assert_eq!(
            read_frame::<_, EmbedRequestV1>(&mut [0u8; 2].as_slice()).unwrap_err(),
            ProtocolError::Truncated
        );
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let body = br#"{"protocol_version":1,"provider":"openai","model":"m","role":"document","texts":["a"],"note_path":"/home/u/notes/x.md"}"#;
        let mut framed = (body.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(body);
        assert_eq!(
            read_frame::<_, EmbedRequestV1>(&mut framed.as_slice()).unwrap_err(),
            ProtocolError::Malformed
        );
    }

    #[test]
    fn an_unknown_enum_variant_is_refused() {
        let body = br#"{"protocol_version":1,"provider":"anthropic","model":"m","role":"document","texts":["a"]}"#;
        let mut framed = (body.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(body);
        assert_eq!(
            read_frame::<_, EmbedRequestV1>(&mut framed.as_slice()).unwrap_err(),
            ProtocolError::Malformed
        );
    }

    #[test]
    fn an_unknown_version_is_refused_by_validate() {
        let mut message = request();
        message.protocol_version = 2;
        assert_eq!(
            message.validate().unwrap_err(),
            ProtocolError::UnsupportedVersion
        );
    }

    #[test]
    fn invalid_utf8_in_the_body_is_refused() {
        let body: &[u8] = &[0x7b, 0xff, 0xfe, 0x7d];
        let mut framed = (body.len() as u32).to_be_bytes().to_vec();
        framed.extend_from_slice(body);
        assert_eq!(
            read_frame::<_, EmbedRequestV1>(&mut framed.as_slice()).unwrap_err(),
            ProtocolError::Malformed
        );
    }

    // --------------------------------------------------------- the limits

    #[test]
    fn an_empty_batch_is_refused() {
        let mut message = request();
        message.texts.clear();
        assert_eq!(
            message.validate().unwrap_err(),
            ProtocolError::LimitExceeded
        );
    }

    #[test]
    fn the_batch_ceiling_is_exact() {
        let mut message = request();
        message.texts = vec!["a".to_string(); MAX_TEXTS];
        message.validate().expect("max is allowed");
        message.texts.push("a".to_string());
        assert_eq!(
            message.validate().unwrap_err(),
            ProtocolError::LimitExceeded
        );
    }

    #[test]
    fn the_per_text_ceiling_is_exact() {
        let mut message = request();
        message.texts = vec!["a".repeat(MAX_TEXT_BYTES)];
        message.validate().expect("max is allowed");
        message.texts = vec!["a".repeat(MAX_TEXT_BYTES + 1)];
        assert_eq!(
            message.validate().unwrap_err(),
            ProtocolError::LimitExceeded
        );
    }

    #[test]
    fn the_total_ceiling_catches_what_the_per_text_one_does_not() {
        // Each text is legal on its own; together they are not.
        let each = MAX_TOTAL_TEXT_BYTES / MAX_TEXTS + 1;
        assert!(each <= MAX_TEXT_BYTES);
        let mut message = request();
        message.texts = vec!["a".repeat(each); MAX_TEXTS];
        assert_eq!(
            message.validate().unwrap_err(),
            ProtocolError::LimitExceeded
        );
    }

    #[test]
    fn a_dimension_outside_the_range_is_refused() {
        for dimension in [0, (MAX_DIMENSION + 1) as u32] {
            let mut message = request();
            message.dimension = Some(dimension);
            assert_eq!(
                message.validate().unwrap_err(),
                ProtocolError::LimitExceeded
            );
        }
        let mut message = request();
        message.dimension = Some(MAX_DIMENSION as u32);
        message.validate().expect("the ceiling itself is allowed");
    }

    // ------------------------------------------------- the model alphabet

    #[test]
    fn a_model_name_that_could_steer_a_url_is_refused() {
        // Gemini puts the model in the path. Every one of these would leave
        // the pinned endpoint while `provider` still said `gemini`.
        for hostile in [
            "../../../etc/passwd",
            "..",
            ".hidden",
            "a/b",
            "a?x=1",
            "a#frag",
            "a@evil.test",
            "http://169.254.169.254/",
            "a%2Fb",
            "a b",
            "a:embedContent",
            "",
        ] {
            assert!(!is_model_token(hostile), "accepted {hostile:?}");
        }
    }

    #[test]
    fn the_real_model_names_are_accepted() {
        for good in [
            "text-embedding-3-small",
            "text-embedding-3-large",
            "text-embedding-ada-002",
            "gemini-embedding-2",
            "gemini-embedding-001",
            "voyage-4-large",
            "voyage-4",
            "voyage-4-lite",
            "voyage-3.5",
        ] {
            assert!(is_model_token(good), "refused {good:?}");
        }
    }

    #[test]
    fn a_model_name_longer_than_the_ceiling_is_refused() {
        assert!(is_model_token(&"a".repeat(MAX_MODEL_CHARS)));
        assert!(!is_model_token(&"a".repeat(MAX_MODEL_CHARS + 1)));
    }

    // ------------------------------------------------- response validation

    #[test]
    fn a_response_may_not_carry_a_non_finite_float() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                EmbedResponseV1::answer(2, vec![vec![0.5, bad]]).unwrap_err(),
                WireError::InvalidResponse
            );
        }
    }

    #[test]
    fn a_response_with_the_wrong_count_is_refused() {
        let two = EmbedResponseV1::answer(1, vec![vec![1.0], vec![1.0]]).expect("answer");
        assert_eq!(two.validate(1).unwrap_err(), ProtocolError::Malformed);
        assert_eq!(two.validate(3).unwrap_err(), ProtocolError::Malformed);
        two.validate(2).expect("two for two");
    }

    #[test]
    fn a_response_whose_vectors_disagree_with_its_dimension_is_refused() {
        assert_eq!(
            EmbedResponseV1::answer(3, vec![vec![1.0, 1.0]]).unwrap_err(),
            WireError::DimensionMismatch
        );
    }

    #[test]
    fn a_hand_built_response_with_a_lying_dimension_is_refused_on_arrival() {
        // The constructor cannot produce this; a hostile worker can.
        let lying = EmbedResponseV1::Ok {
            protocol_version: PROTOCOL_VERSION,
            dimension: 4,
            vectors: vec![vec![1.0, 1.0]],
        };
        assert_eq!(lying.validate(1).unwrap_err(), ProtocolError::Malformed);
    }

    #[test]
    fn provider_identifiers_round_trip_through_their_stable_names() {
        for id in ProviderId::ALL {
            assert_eq!(ProviderId::parse(id.as_str()), Some(id));
        }
        assert_eq!(ProviderId::parse("anthropic"), None);
        assert_eq!(ProviderId::parse("local"), None);
        assert_eq!(ProviderId::parse(""), None);
    }
}
