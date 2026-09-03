//! The token that answers "is the note still the one I read?".
//!
//! The writer lease in [`crate::coordination`] answers a different question —
//! *who may write now* — and it answers it correctly. It serialises writers, so
//! two of them never interleave. What it cannot see is a writer holding a base
//! it read minutes ago:
//!
//! ```text
//! T0  a client reads the note                      base = "SHARED-BASE"
//! T1  somebody else appends, coordinated           committed
//! T2  the client writes back what it built from T0 committed
//!     -> T1 is gone, and nothing failed
//! ```
//!
//! Both writes took the lease. Both were told they committed. The lease did its
//! job. The missing question is the one this module exists to ask, and the two
//! mechanisms are orthogonal: a lease without revisions loses stale writes, and
//! revisions without a lease would let two writers interleave mid-file.
//!
//! ## What the token is
//!
//! The SHA-256 of the exact bytes the note would be persisted as — the same
//! [`NoteDocument::serialize`] the atomic writer stores. That choice is
//! deliberate and it is the whole design:
//!
//! - it covers *everything* a later write could overwrite — identifier, body,
//!   tags, properties, colour, paper, font size, timestamps, and the unknown
//!   front matter Note-it preserves for other tools — without this module
//!   keeping a second opinion about what a note consists of;
//! - it is deterministic: the field order is the struct's, tags keep their
//!   order, properties are sorted by semantic identity, unknown keys live in a
//!   `BTreeMap`, and timestamps are RFC 3339 in UTC. No hash map iteration, no
//!   address, no locale, no clock;
//! - it says nothing about *where* the note is. The same note under a symlinked
//!   store, a `..` path or a different `XDG_DATA_HOME` has the same revision,
//!   because the path is not part of what was read.
//!
//! ## What it deliberately is not
//!
//! **Not `mtime`.** Its resolution varies by filesystem, it can be set to
//! anything by anyone, and two different writes can land in the same tick.
//!
//! **Not `updated_at`.** That field is domain information — it moves when the
//! *text* changes and deliberately stays put when a tag does. A tag change is
//! invisible to it and would let a stale write through.
//!
//! **Not a secret.** A revision identifies a version; it protects nobody from
//! reading anything. It is validated on the way in only so that a malformed
//! token is refused rather than mistaken for "no precondition given".

use crate::hashing::sha256_hex;
use crate::model::NoteDocument;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// The number of characters in the published form: SHA-256 as lowercase hex.
pub const REVISION_HEX_LENGTH: usize = 64;

/// Why a string is not a revision.
///
/// Separate from every other error because the caller's mistake is specific:
/// they sent a precondition that cannot name any version at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevisionFormatError {
    /// Not [`REVISION_HEX_LENGTH`] characters.
    Length(usize),
    /// Something other than `0-9a-f`. Uppercase lands here on purpose: the
    /// published format is lowercase, and quietly accepting `ABC…` would make
    /// two spellings of one digest compare unequal somewhere else.
    NotLowercaseHex,
}

impl fmt::Display for RevisionFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length(actual) => write!(
                formatter,
                "a revisão precisa ter {REVISION_HEX_LENGTH} caracteres hexadecimais, e esta tem {actual}"
            ),
            Self::NotLowercaseHex => formatter.write_str(
                "a revisão só aceita dígitos hexadecimais minúsculos (0-9, a-f)",
            ),
        }
    }
}

impl std::error::Error for RevisionFormatError {}

/// One note, at one exact version.
///
/// Opaque on purpose: a consumer stores it and hands it back, and never has to
/// recompute it or know how it was made.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteRevision(String);

impl NoteRevision {
    /// The revision of a document as it stands right now.
    ///
    /// Fails only when the document cannot be serialised at all — the same
    /// failure that would stop it being written. There is deliberately no
    /// fallback value: a revision nobody could compute must not become a
    /// revision that compares equal to something.
    pub fn for_document(document: &NoteDocument) -> Result<Self, String> {
        let canonical = document.serialize()?;
        Ok(Self(sha256_hex(canonical.as_bytes())))
    }

    /// Reads a revision a caller supplied.
    ///
    /// Refuses anything that is not exactly the published form. The length is
    /// checked before the characters so an enormous argument is rejected on
    /// sight rather than scanned.
    pub fn parse(raw: &str) -> Result<Self, RevisionFormatError> {
        if raw.len() != REVISION_HEX_LENGTH {
            return Err(RevisionFormatError::Length(raw.chars().count()));
        }
        if !raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(RevisionFormatError::NotLowercaseHex);
        }
        Ok(Self(raw.to_string()))
    }

    /// The published form: sixty-four lowercase hexadecimal characters.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NoteRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for NoteRevision {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for NoteRevision {
    /// Validates on the way in, and that is the point.
    ///
    /// A `WriteOperation` carrying a precondition travels over the control
    /// socket to the authority that will act on it. A malformed token accepted
    /// here would reach the comparison as *something*, and the one outcome that
    /// must never happen is a bad precondition quietly becoming no precondition
    /// at all.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{NoteMetadata, NoteProperty};

    fn note(body: &str) -> NoteDocument {
        let mut document = NoteDocument::new_empty();
        document.content = body.to_string();
        document
    }

    #[test]
    fn the_same_document_always_has_the_same_revision() {
        let document = note("SHARED-BASE");
        let first = NoteRevision::for_document(&document).expect("revision");
        let second = NoteRevision::for_document(&document).expect("revision");
        assert_eq!(first, second);
        // And a clone is the same note, not a different one.
        assert_eq!(
            first,
            NoteRevision::for_document(&document.clone()).expect("revision")
        );
    }

    #[test]
    fn the_published_form_is_sixty_four_lowercase_hex_characters() {
        let revision = NoteRevision::for_document(&note("x")).expect("revision");
        assert_eq!(revision.as_str().len(), REVISION_HEX_LENGTH);
        assert!(revision.as_str().chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(revision.as_str(), revision.as_str().to_lowercase());
    }

    #[test]
    fn every_persisted_field_moves_the_revision() {
        // The whole contract: anything a later write could overwrite has to be
        // visible here, or a stale write slips through on the field this misses.
        let base = note("BODY");
        let unchanged = NoteRevision::for_document(&base).expect("revision");

        let mut body = base.clone();
        body.content = "OTHER".into();
        assert_ne!(unchanged, NoteRevision::for_document(&body).expect("r"));

        let mut tagged = base.clone();
        tagged.user_metadata =
            NoteMetadata::try_new(vec!["etiqueta".into()], vec![]).expect("meta");
        assert_ne!(unchanged, NoteRevision::for_document(&tagged).expect("r"));

        let mut propertied = base.clone();
        propertied.user_metadata = NoteMetadata::try_new(
            vec![],
            vec![NoteProperty {
                key: "chave".into(),
                value: "valor".into(),
            }],
        )
        .expect("meta");
        assert_ne!(
            unchanged,
            NoteRevision::for_document(&propertied).expect("r")
        );

        let mut coloured = base.clone();
        coloured.metadata.color = "blue".into();
        assert_ne!(unchanged, NoteRevision::for_document(&coloured).expect("r"));

        let mut papered = base.clone();
        papered.metadata.paper_type = "lined".into();
        assert_ne!(unchanged, NoteRevision::for_document(&papered).expect("r"));

        let mut intense = base.clone();
        intense.metadata.paper_intensity = "strong".into();
        assert_ne!(unchanged, NoteRevision::for_document(&intense).expect("r"));

        let mut sized = base.clone();
        sized.metadata.font_size = 22;
        assert_ne!(unchanged, NoteRevision::for_document(&sized).expect("r"));

        let mut touched = base.clone();
        touched.metadata.updated_at =
            Some(touched.metadata.updated_at.expect("seeded") + chrono::Duration::seconds(1));
        assert_ne!(unchanged, NoteRevision::for_document(&touched).expect("r"));

        let mut renamed = base.clone();
        renamed.metadata.id = uuid::Uuid::new_v4();
        assert_ne!(unchanged, NoteRevision::for_document(&renamed).expect("r"));
    }

    #[test]
    fn two_different_notes_do_not_share_a_revision() {
        assert_ne!(
            NoteRevision::for_document(&note("one")).expect("r"),
            NoteRevision::for_document(&note("two")).expect("r")
        );
    }

    #[test]
    fn parsing_refuses_everything_that_is_not_the_published_form() {
        let valid = NoteRevision::for_document(&note("x")).expect("revision");
        assert_eq!(
            NoteRevision::parse(valid.as_str()).expect("round trip"),
            valid
        );

        assert!(matches!(
            NoteRevision::parse(""),
            Err(RevisionFormatError::Length(0))
        ));
        assert!(matches!(
            NoteRevision::parse(&"a".repeat(63)),
            Err(RevisionFormatError::Length(63))
        ));
        assert!(matches!(
            NoteRevision::parse(&"a".repeat(65)),
            Err(RevisionFormatError::Length(65))
        ));
        // Uppercase is refused rather than folded: one digest, one spelling.
        assert!(matches!(
            NoteRevision::parse(&valid.as_str().to_uppercase()),
            Err(RevisionFormatError::NotLowercaseHex)
        ));
        assert!(matches!(
            NoteRevision::parse(&"g".repeat(64)),
            Err(RevisionFormatError::NotLowercaseHex)
        ));
        // A path must never be mistaken for a token.
        assert!(NoteRevision::parse("../../etc/passwd").is_err());
        // An enormous argument is refused on its length, not scanned.
        assert!(matches!(
            NoteRevision::parse(&"a".repeat(1_000_000)),
            Err(RevisionFormatError::Length(1_000_000))
        ));
    }

    #[test]
    fn a_malformed_revision_never_deserialises_into_nothing() {
        // The bypass this guards: if a bad token decoded as `None`, a client
        // could send rubbish and get an unconditional write.
        assert!(serde_json::from_str::<NoteRevision>("\"nope\"").is_err());
        assert!(serde_json::from_str::<Option<NoteRevision>>("\"nope\"").is_err());
        assert_eq!(
            serde_json::from_str::<Option<NoteRevision>>("null").expect("null is no precondition"),
            None
        );

        let valid = NoteRevision::for_document(&note("x")).expect("revision");
        let encoded = serde_json::to_string(&valid).expect("encode");
        assert_eq!(encoded, format!("\"{valid}\""));
        assert_eq!(
            serde_json::from_str::<NoteRevision>(&encoded).expect("decode"),
            valid
        );
    }
}
