//! Identity for vectors: whose model made them, over which weights, and
//! whether two of them may be compared at all.
//!
//! No provider lives here, and none is named. 4.3A measured what happens when
//! that question is answered by shape instead of by identity: truncate one
//! model's vectors to another model's dimension and every cosine is a
//! perfectly calculable number, while R@3 collapses from 0.933 to 0.133 and
//! **nothing in the arithmetic complains**. That is the defect class this
//! module exists to make structurally impossible — numbers valid, ranking
//! invalid, no error anywhere.
//!
//! So the rule is blunt: two vectors are comparable only when their
//! [`EmbeddingSpaceId`] values are *equal*. Not compatible, not the same size —
//! equal, in every field, including which bytes of weights produced them.
//!
//! ## What is deliberately not here
//!
//! No network, no credential, no model, no provider implementation. 4.3B builds
//! the frame; 4.3C fits a local provider into it and 4.3D optional remote ones,
//! and neither may need this file to change shape to fit.

use crate::hashing::sha256_hex;
use std::fmt;

/// The prefix every artifact identity is hashed under.
///
/// A domain separator, with a version in it. What it buys is **semantic**
/// separation: the same bytes cannot be read as an identity of another kind or
/// of another version of this format, and the format can change without the two
/// becoming ambiguous. It does **not** make collisions impossible, and this
/// file will not say that it does — collision and pre-image resistance stay
/// properties of SHA-256, and no prefix alters them.
pub const ARTIFACT_DOMAIN: &str = "noteit.artifact.v1\n";

// ------------------------------------------------------- canonical encoding

/// A value that may appear in a canonical object.
///
/// Deliberately two shapes and no more. Everything that has ever needed an
/// identity in this module is a digest, a UUID or a version number, so the
/// alphabet can stay narrow enough that escaping never arises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalValue<'a> {
    /// ASCII letters, digits, `-`, `_` and `.`, and at least one character.
    Token(&'a str),
    Number(u64),
}

/// Why a canonical object could not be built.
///
/// Every one of these is a programming error rather than a user's: they say
/// that something was handed to the encoder that the encoding does not define
/// an answer for. Refusing is the whole point — an encoder that guesses is an
/// encoder whose output two different inputs can share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalError {
    /// A field name is not `[a-z0-9_]+`.
    FieldName,
    /// The same field was given twice, so the object has no single reading.
    DuplicateField,
    /// A token is empty or steps outside the alphabet.
    Value,
    /// A field that must be a SHA-256 digest is not sixty-four lowercase
    /// hexadecimal characters.
    NotADigest,
}

impl fmt::Display for CanonicalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::FieldName => "nome de campo fora do alfabeto canônico",
            Self::DuplicateField => "campo repetido na codificação canônica",
            Self::Value => "valor fora do alfabeto canônico",
            Self::NotADigest => "o campo exige um digest SHA-256 hexadecimal",
        })
    }
}

impl std::error::Error for CanonicalError {}

/// The one canonical encoding in this crate, and the only way an identity is
/// built.
///
/// JSON with the keys in lexicographic order, no whitespace and no escapes,
/// UTF-8. The important part is not the syntax, it is what the syntax rules
/// out. Hashing `a ‖ b ‖ c` over variable-length components is ambiguous —
/// two different decompositions can produce one byte string, and then two
/// different artifacts get one identity. Named fields in a fixed order remove
/// the ambiguity by construction, and refusing anything outside the alphabet
/// removes the escaping routine whose bugs would put it back.
///
/// There is one of these on purpose. An identity assembled from `format!` in
/// three places is three encodings that will disagree the first time one of
/// them is edited.
pub fn canonical_object(fields: &[(&str, CanonicalValue<'_>)]) -> Result<String, CanonicalError> {
    let mut sorted: Vec<&(&str, CanonicalValue<'_>)> = fields.iter().collect();
    sorted.sort_by(|left, right| left.0.cmp(right.0));

    let mut encoded = String::from("{");
    let mut previous: Option<&str> = None;
    for (name, value) in sorted {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(CanonicalError::FieldName);
        }
        if previous == Some(*name) {
            return Err(CanonicalError::DuplicateField);
        }
        if previous.is_some() {
            encoded.push(',');
        }
        previous = Some(name);

        encoded.push('"');
        encoded.push_str(name);
        encoded.push_str("\":");
        match value {
            CanonicalValue::Number(number) => encoded.push_str(&number.to_string()),
            CanonicalValue::Token(token) => {
                if token.is_empty() || !token.bytes().all(is_token_byte) {
                    return Err(CanonicalError::Value);
                }
                encoded.push('"');
                encoded.push_str(token);
                encoded.push('"');
            }
        }
    }
    encoded.push('}');
    Ok(encoded)
}

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.'
}

/// Whether a string is exactly what this crate calls a digest.
pub fn is_digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

// ------------------------------------------------------------ the artifact

/// What proves that two sets of weights are the same weights.
///
/// The model's *name* does not: the failure 4.3A measured comes back whole if
/// the weights, the tokenizer, the normalisation or the recipe change while the
/// name and the dimension stay put. Each component enters as a fixed-length
/// digest or an integer, so the encoding below has nothing variable to
/// disambiguate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtifactManifestV1 {
    /// SHA-256 of the weight bytes **as loaded**, not as advertised.
    pub weights_sha256: String,
    /// SHA-256 of the tokenizer that goes with them.
    pub tokenizer_sha256: String,
    /// Version of the document/query recipe *pair*. One number for both halves,
    /// because changing only the query side invalidates comparison with already
    /// indexed documents exactly as much as changing the document side.
    pub embedding_recipe_version: u32,
    /// Version of Note-it's own text normalisation.
    pub normalization_version: u32,
}

impl ArtifactManifestV1 {
    /// `sha256("noteit.artifact.v1\n" ‖ canonical_object(self))`.
    ///
    /// Computed over bytes that were actually loaded, never over a filename or
    /// a version string somebody typed. Swap the weights file for another of
    /// the same size and the digest moves, which invalidates the index — not a
    /// promise that nobody will swap it, a check that fails when they do.
    pub fn identity(&self) -> Result<ArtifactIdentity, CanonicalError> {
        if !is_digest(&self.weights_sha256) || !is_digest(&self.tokenizer_sha256) {
            return Err(CanonicalError::NotADigest);
        }
        let encoded = canonical_object(&[
            (
                "embedding_recipe_version",
                CanonicalValue::Number(u64::from(self.embedding_recipe_version)),
            ),
            (
                "normalization_version",
                CanonicalValue::Number(u64::from(self.normalization_version)),
            ),
            (
                "tokenizer_sha256",
                CanonicalValue::Token(&self.tokenizer_sha256),
            ),
            (
                "weights_sha256",
                CanonicalValue::Token(&self.weights_sha256),
            ),
        ])?;
        let mut input = String::with_capacity(ARTIFACT_DOMAIN.len() + encoded.len());
        input.push_str(ARTIFACT_DOMAIN);
        input.push_str(&encoded);
        Ok(ArtifactIdentity::LocalVerified(LocalArtifactDigest(
            sha256_hex(input.as_bytes()),
        )))
    }
}

/// The digest of an [`ArtifactManifestV1`].
///
/// Can only be constructed through [`ArtifactManifestV1::identity`], guaranteeing
/// that a local verified artifact identity was derived from a valid manifest
/// over actually loaded component digests.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocalArtifactDigest(String);

impl LocalArtifactDigest {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for LocalArtifactDigest {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LocalArtifactDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// What is known about the artifact behind a space.
///
/// Three answers, representing three distinct trust and reproducibility guarantees:
///
/// 1. A local artifact whose weights and tokenizer were loaded and hashed
///    into a canonical manifest digest ([`ArtifactIdentity::LocalVerified`]).
/// 2. A remote provider offering an immutable version or snapshot identifier
///    ([`ArtifactIdentity::ProviderPinned`]). We do not hold the bytes, but the
///    provider promises immutability under that identifier.
/// 3. A remote provider with a mutable alias like `model-latest`
///    ([`ArtifactIdentity::UnverifiableAlias`]). The weights can change on the
///    remote side without detection, and this is recorded explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ArtifactIdentity {
    /// The digest of an [`ArtifactManifestV1`] over bytes that were loaded.
    LocalVerified(LocalArtifactDigest),
    /// An immutable identifier or snapshot provided by a remote provider.
    /// Does not pretend to be a local byte hash.
    ProviderPinned { provider: String, pinned_id: String },
    /// The provider names its model with a mutable alias. Nothing here can
    /// detect that the weights changed on the other side, and no heuristic is
    /// offered: a statistical test whose false negative is exactly the
    /// dangerous case is worse than an admission.
    UnverifiableAlias { alias: String },
}

impl ArtifactIdentity {
    pub fn provider_pinned(provider: impl Into<String>, pinned_id: impl Into<String>) -> Self {
        Self::ProviderPinned {
            provider: provider.into(),
            pinned_id: pinned_id.into(),
        }
    }

    pub fn unverifiable_alias(alias: impl Into<String>) -> Self {
        Self::UnverifiableAlias {
            alias: alias.into(),
        }
    }

    /// Whether this identity carries an explicit immutability guarantee.
    pub fn is_verifiable(&self) -> bool {
        match self {
            Self::LocalVerified(_) | Self::ProviderPinned { .. } => true,
            Self::UnverifiableAlias { .. } => false,
        }
    }
}

// --------------------------------------------------------------- the space

/// Which entries may be compared with which.
///
/// Equality is all of it, and every field is load-bearing:
///
/// * `provider` and `model`, because a name is what changes when the numbers
///   silently stop meaning the same thing;
/// * `artifact`, because a name is not enough — §5.1 of the specification;
/// * `dimension`, the one *actually* used and not the model's maximum;
/// * `embedding_recipe` and `normalization`, because the same weights fed
///   differently prepared text produce vectors that are not in the same space.
///
/// A new model from the same provider is a new space until somebody proves
/// otherwise. `model-v1` and `model-v2` are not one space because the vendor is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmbeddingSpaceId {
    /// A label, never a switch. Nothing in this crate branches on its value —
    /// see `docs/semantic-retrieval.md` §23.
    pub provider: String,
    pub model: String,
    pub artifact: ArtifactIdentity,
    pub dimension: usize,
    pub embedding_recipe: u32,
    pub normalization: u32,
}

/// What an entry is: a note's text, or somebody's question.
///
/// **Not part of [`EmbeddingSpaceId`], and that is the correction 4.3A.R1
/// made.** 4.3A put the role inside the space and also demanded exact equality
/// to compare — which contradicted itself, because a search compares a query
/// vector against document vectors. Under that rule no search would ever have
/// been valid.
///
/// The two recipes may and should differ — `e5` wants `passage: ` and
/// `query: `, other vendors prepend their own instructions — and they differ
/// *in order to* produce comparable vectors. Which is why the version is of the
/// pair, in the space, and the role is here instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmbeddingRole {
    Document,
    Query,
}

// -------------------------------------------------------------- the vector

/// A vector that is known to be usable before anything tries to use it.
///
/// There is no way to build one carrying a `NaN`, an infinity, no components or
/// a zero norm, so nothing downstream has to check and no cosine can come back
/// as `NaN`. The alternative — validating at the point of comparison — means
/// every comparison is a place the check can be forgotten.
#[derive(Clone, PartialEq)]
pub struct EmbeddingVector {
    values: Vec<f32>,
    /// Kept because every comparison needs it and nothing can change it.
    norm: f64,
}

impl EmbeddingVector {
    /// Validates and takes ownership, or refuses.
    pub fn new(values: Vec<f32>) -> Result<Self, SemanticError> {
        if values.is_empty() {
            return Err(SemanticError::InvalidVector);
        }
        let mut squared = 0.0f64;
        for value in &values {
            if !value.is_finite() {
                return Err(SemanticError::InvalidVector);
            }
            squared += f64::from(*value) * f64::from(*value);
        }
        let norm = squared.sqrt();
        // A zero vector has no direction, so it has no cosine with anything.
        // Refused rather than answered with `0.0`, which would be a number
        // where there is no answer, and rather than `NaN`, which would rank.
        if !norm.is_finite() || norm <= 0.0 {
            return Err(SemanticError::InvalidVector);
        }
        Ok(Self { values, norm })
    }

    pub fn dimension(&self) -> usize {
        self.values.len()
    }

    /// Cosine similarity against another vector of the same dimension.
    ///
    /// Both vectors are valid by construction and both norms are positive, so
    /// the result is a real number in `[-1, 1]` up to floating-point error, and
    /// there is no case left in which it is not.
    pub fn cosine(&self, other: &Self) -> Result<f64, SemanticError> {
        if self.values.len() != other.values.len() {
            return Err(SemanticError::DimensionMismatch {
                expected: self.values.len(),
                actual: other.values.len(),
            });
        }
        let mut product = 0.0f64;
        for (left, right) in self.values.iter().zip(&other.values) {
            product += f64::from(*left) * f64::from(*right);
        }
        Ok(product / (self.norm * other.norm))
    }
}

/// Redacted on purpose.
///
/// A vector is derived from a private note, and a `Debug` that prints a
/// thousand floats puts that derivation into every log line and every panic
/// message that ever formats a record. The dimension is the part that helps
/// somebody debugging; the components are the part that leaks.
impl fmt::Debug for EmbeddingVector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddingVector")
            .field("dimension", &self.values.len())
            .field("values", &"<redacted>")
            .finish()
    }
}

/// One vector, with everything needed to know whether it may be compared.
///
/// The space travels with the vector rather than being looked up from whoever
/// happens to be holding it, so a provider that answered from a different model
/// than it advertises is caught at the boundary instead of ranking.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedding {
    space: EmbeddingSpaceId,
    role: EmbeddingRole,
    vector: EmbeddingVector,
}

impl Embedding {
    /// Binds a vector to a space, refusing the pair that cannot be true.
    pub fn new(
        space: EmbeddingSpaceId,
        role: EmbeddingRole,
        vector: EmbeddingVector,
    ) -> Result<Self, SemanticError> {
        if vector.dimension() != space.dimension {
            return Err(SemanticError::DimensionMismatch {
                expected: space.dimension,
                actual: vector.dimension(),
            });
        }
        Ok(Self {
            space,
            role,
            vector,
        })
    }

    pub fn space(&self) -> &EmbeddingSpaceId {
        &self.space
    }

    pub fn role(&self) -> EmbeddingRole {
        self.role
    }

    pub fn vector(&self) -> &EmbeddingVector {
        &self.vector
    }

    pub fn into_vector(self) -> EmbeddingVector {
        self.vector
    }
}

/// Cosine between two embeddings, or a refusal that says which rule broke.
///
/// The space is checked first and it is checked whole. Equal dimension is
/// **never** enough: 4.3A truncated one model's vectors to another's dimension
/// and got a working cosine and a ranking that had collapsed, which is why the
/// order of these two checks is the contract and not a preference.
///
/// The role is not checked, and must not be: a search is exactly the comparison
/// of a query vector with document vectors, so refusing across roles would
/// refuse every search there is.
pub fn cosine(left: &Embedding, right: &Embedding) -> Result<f64, SemanticError> {
    if left.space != right.space {
        return Err(SemanticError::SpaceMismatch);
    }
    left.vector.cosine(&right.vector)
}

// --------------------------------------------------------------- the errors

/// What can go wrong in the semantic half, as facts rather than as sentences.
///
/// Typed inside the Core, and the public message is Note-it's own — the lesson
/// of 4.2R.R1 applied before the defect exists. A vendor's library writes
/// request IDs and fragments nobody controls into its error strings, and an
/// answer that echoes one is the leak `scripts/check-mcp-boundary` already
/// forbids by `format!`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticError {
    /// There is no provider, or it could not answer at all.
    Unavailable,
    /// The provider answered, and the answer is not one: the wrong number of
    /// vectors for the batch, or a role that was not the one asked for.
    InvalidResponse,
    /// A vector is not the length its space says it is.
    DimensionMismatch { expected: usize, actual: usize },
    /// The two sides are not in the same space. Never a number.
    SpaceMismatch,
    /// A vector carries `NaN`, an infinity, no components, or has zero norm.
    InvalidVector,
    /// A record was produced by a different chunker than the one running.
    ChunkerMismatch { expected: u32, actual: u32 },
    /// A document could not be reduced to the canonical form an identity needs.
    Unindexable,
}

impl fmt::Display for SemanticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("o canal semântico não está disponível"),
            Self::InvalidResponse => {
                formatter.write_str("a resposta do provider não tem a forma esperada")
            }
            Self::DimensionMismatch { expected, actual } => write!(
                formatter,
                "o vetor tem dimensão {actual} onde o espaço declara {expected}"
            ),
            Self::SpaceMismatch => {
                formatter.write_str("os vetores não pertencem ao mesmo espaço de embedding")
            }
            Self::InvalidVector => formatter.write_str("o vetor não é comparável"),
            Self::ChunkerMismatch { expected, actual } => write!(
                formatter,
                "o registro veio do chunker v{actual} e o corrente é v{expected}"
            ),
            Self::Unindexable => {
                formatter.write_str("o documento não pôde ser reduzido à forma canônica")
            }
        }
    }
}

impl std::error::Error for SemanticError {}

impl From<CanonicalError> for SemanticError {
    fn from(_: CanonicalError) -> Self {
        // Deliberately lossy. A canonical-encoding failure is a programming
        // error inside this crate, and its detail belongs in a test's message,
        // not in anything that could travel.
        Self::Unindexable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: u8) -> String {
        sha256_hex(&[byte])
    }

    fn manifest() -> ArtifactManifestV1 {
        ArtifactManifestV1 {
            weights_sha256: digest(1),
            tokenizer_sha256: digest(2),
            embedding_recipe_version: 1,
            normalization_version: 1,
        }
    }

    fn space(dimension: usize) -> EmbeddingSpaceId {
        EmbeddingSpaceId {
            provider: "test".to_string(),
            model: "model-a".to_string(),
            artifact: manifest().identity().expect("identity"),
            dimension,
            embedding_recipe: 1,
            normalization: 1,
        }
    }

    fn vector(space: &EmbeddingSpaceId, role: EmbeddingRole, values: &[f32]) -> Embedding {
        Embedding::new(
            space.clone(),
            role,
            EmbeddingVector::new(values.to_vec()).expect("a usable vector"),
        )
        .expect("the vector fits the space")
    }

    // ------------------------------------------------- canonical encoding

    #[test]
    fn a_canonical_object_sorts_its_keys_and_says_nothing_else() {
        let encoded = canonical_object(&[
            ("zeta", CanonicalValue::Number(2)),
            ("alpha", CanonicalValue::Token("abc-1")),
        ])
        .expect("encodes");
        assert_eq!(encoded, r#"{"alpha":"abc-1","zeta":2}"#);
    }

    #[test]
    fn the_order_the_object_is_built_in_cannot_change_its_encoding() {
        let one = canonical_object(&[
            ("a", CanonicalValue::Number(1)),
            ("b", CanonicalValue::Number(2)),
            ("c", CanonicalValue::Number(3)),
        ])
        .expect("encodes");
        let other = canonical_object(&[
            ("c", CanonicalValue::Number(3)),
            ("a", CanonicalValue::Number(1)),
            ("b", CanonicalValue::Number(2)),
        ])
        .expect("encodes");
        assert_eq!(one, other);
    }

    #[test]
    fn the_encoder_refuses_rather_than_escaping() {
        assert_eq!(
            canonical_object(&[("a", CanonicalValue::Token("with\"quote"))]),
            Err(CanonicalError::Value)
        );
        assert_eq!(
            canonical_object(&[("a", CanonicalValue::Token(""))]),
            Err(CanonicalError::Value)
        );
        assert_eq!(
            canonical_object(&[("Uppercase", CanonicalValue::Number(1))]),
            Err(CanonicalError::FieldName)
        );
        assert_eq!(
            canonical_object(&[
                ("a", CanonicalValue::Number(1)),
                ("a", CanonicalValue::Number(2)),
            ]),
            Err(CanonicalError::DuplicateField)
        );
    }

    // -------------------------------------------------- artifact identity

    #[test]
    fn the_same_manifest_is_the_same_identity_every_time() {
        assert_eq!(manifest().identity(), manifest().identity());
        // And it is pinned, so a change to the encoding is a failing test
        // rather than an index that silently stops matching.
        let ArtifactIdentity::LocalVerified(digest) = manifest().identity().expect("identity")
        else {
            panic!("a manifest produces a local verified identity");
        };
        assert!(is_digest(digest.as_str()));
        assert_eq!(
            digest.as_str(),
            sha256_hex(
                format!(
                    "{ARTIFACT_DOMAIN}{}",
                    canonical_object(&[
                        ("embedding_recipe_version", CanonicalValue::Number(1)),
                        ("normalization_version", CanonicalValue::Number(1)),
                        (
                            "tokenizer_sha256",
                            CanonicalValue::Token(&manifest().tokenizer_sha256)
                        ),
                        (
                            "weights_sha256",
                            CanonicalValue::Token(&manifest().weights_sha256)
                        ),
                    ])
                    .expect("encodes")
                )
                .as_bytes()
            )
        );
    }

    #[test]
    fn changing_any_component_changes_the_identity() {
        let base = manifest().identity().expect("identity");

        let mut weights = manifest();
        weights.weights_sha256 = digest(9);
        assert_ne!(weights.identity().expect("identity"), base);

        let mut tokenizer = manifest();
        tokenizer.tokenizer_sha256 = digest(9);
        assert_ne!(tokenizer.identity().expect("identity"), base);

        let mut recipe = manifest();
        recipe.embedding_recipe_version = 2;
        assert_ne!(recipe.identity().expect("identity"), base);

        let mut normalization = manifest();
        normalization.normalization_version = 2;
        assert_ne!(normalization.identity().expect("identity"), base);
    }

    #[test]
    fn swapping_the_two_digests_is_a_different_artifact() {
        // The property a plain concatenation cannot give: two components of the
        // same length, exchanged, must not hash to the same identity.
        let mut swapped = manifest();
        std::mem::swap(&mut swapped.weights_sha256, &mut swapped.tokenizer_sha256);
        assert_ne!(
            swapped.identity().expect("identity"),
            manifest().identity().expect("identity")
        );
    }

    #[test]
    fn a_manifest_that_is_not_digests_is_refused() {
        let mut broken = manifest();
        broken.weights_sha256 = "not-a-digest".to_string();
        assert_eq!(broken.identity(), Err(CanonicalError::NotADigest));
    }

    #[test]
    fn remote_pinned_identities_follow_provider_and_pinned_id() {
        let one = ArtifactIdentity::provider_pinned("remote-vendor", "snapshot-2026-09-01");
        let same = ArtifactIdentity::provider_pinned("remote-vendor", "snapshot-2026-09-01");
        let different_snapshot =
            ArtifactIdentity::provider_pinned("remote-vendor", "snapshot-2026-09-02");
        let different_vendor =
            ArtifactIdentity::provider_pinned("other-vendor", "snapshot-2026-09-01");

        assert_eq!(one, same);
        assert_ne!(one, different_snapshot);
        assert_ne!(one, different_vendor);
        assert!(one.is_verifiable());
    }

    #[test]
    fn remote_pinned_and_unverifiable_alias_and_local_verified_are_all_distinct() {
        let local = manifest().identity().expect("identity");
        let pinned = ArtifactIdentity::provider_pinned("vendor", "v1.0.0");
        let alias = ArtifactIdentity::unverifiable_alias("model-latest");

        assert_ne!(local, pinned);
        assert_ne!(local, alias);
        assert_ne!(pinned, alias);

        assert!(local.is_verifiable());
        assert!(pinned.is_verifiable());
        assert!(
            !alias.is_verifiable(),
            "a mutable alias is explicitly unverifiable"
        );
    }

    // -------------------------------------------------------- the vector

    #[test]
    fn a_vector_that_could_not_be_compared_never_exists() {
        assert_eq!(
            EmbeddingVector::new(vec![]).unwrap_err(),
            SemanticError::InvalidVector
        );
        assert_eq!(
            EmbeddingVector::new(vec![0.0, 0.0]).unwrap_err(),
            SemanticError::InvalidVector
        );
        assert_eq!(
            EmbeddingVector::new(vec![1.0, f32::NAN]).unwrap_err(),
            SemanticError::InvalidVector
        );
        assert_eq!(
            EmbeddingVector::new(vec![1.0, f32::INFINITY]).unwrap_err(),
            SemanticError::InvalidVector
        );
        assert_eq!(
            EmbeddingVector::new(vec![1.0, f32::NEG_INFINITY]).unwrap_err(),
            SemanticError::InvalidVector
        );
    }

    #[test]
    fn a_vector_does_not_print_itself() {
        let shown = format!("{:?}", EmbeddingVector::new(vec![0.5, 0.5]).expect("valid"));
        assert!(shown.contains("dimension: 2"));
        assert!(shown.contains("<redacted>"));
        assert!(!shown.contains("0.5"));
    }

    #[test]
    fn the_known_cosines_are_the_known_cosines() {
        let space = space(2);
        let same = vector(&space, EmbeddingRole::Document, &[1.0, 0.0]);
        let orthogonal = vector(&space, EmbeddingRole::Document, &[0.0, 1.0]);
        let opposite = vector(&space, EmbeddingRole::Document, &[-1.0, 0.0]);
        let query = vector(&space, EmbeddingRole::Query, &[1.0, 0.0]);

        let close = |left: &Embedding, right: &Embedding, expected: f64| {
            let value = cosine(left, right).expect("comparable");
            assert!(
                (value - expected).abs() < 1e-9,
                "expected {expected}, got {value}"
            );
        };
        close(&query, &same, 1.0);
        close(&query, &orthogonal, 0.0);
        close(&query, &opposite, -1.0);
    }

    #[test]
    fn a_document_and_a_query_of_the_same_space_are_comparable() {
        let space = space(3);
        let document = vector(&space, EmbeddingRole::Document, &[1.0, 0.0, 0.0]);
        let query = vector(&space, EmbeddingRole::Query, &[1.0, 0.0, 0.0]);
        assert_eq!(document.space(), query.space());
        assert_ne!(document.role(), query.role());
        assert!(cosine(&document, &query).is_ok());
    }

    // ------------------------------------------------ the space is the rule

    /// Every way two spaces can differ, each with the same dimension so that
    /// shape can never be what refused them.
    #[test]
    fn equal_dimension_is_never_enough() {
        let base = space(3);
        let document = vector(&base, EmbeddingRole::Document, &[1.0, 0.0, 0.0]);

        let mut other_provider = base.clone();
        other_provider.provider = "other".to_string();

        let mut other_model = base.clone();
        other_model.model = "model-b".to_string();

        let mut other_artifact = base.clone();
        let mut different = manifest();
        different.weights_sha256 = digest(200);
        other_artifact.artifact = different.identity().expect("identity");

        let mut other_recipe = base.clone();
        other_recipe.embedding_recipe = 2;

        let mut other_normalization = base.clone();
        other_normalization.normalization = 2;

        let mut pinned = base.clone();
        pinned.artifact = ArtifactIdentity::provider_pinned("remote-vendor", "snapshot-1");

        let mut alias = base.clone();
        alias.artifact = ArtifactIdentity::unverifiable_alias("model-latest");

        for space in [
            other_provider,
            other_model,
            other_artifact,
            other_recipe,
            other_normalization,
            pinned,
            alias,
        ] {
            assert_eq!(space.dimension, base.dimension, "same shape, on purpose");
            let query = vector(&space, EmbeddingRole::Query, &[1.0, 0.0, 0.0]);
            assert_eq!(
                cosine(&document, &query),
                Err(SemanticError::SpaceMismatch),
                "a comparable-looking pair from a different space must be refused"
            );
        }
    }

    #[test]
    fn a_vector_that_does_not_fit_its_space_never_becomes_an_embedding() {
        assert_eq!(
            Embedding::new(
                space(3),
                EmbeddingRole::Document,
                EmbeddingVector::new(vec![1.0, 0.0]).expect("valid"),
            )
            .unwrap_err(),
            SemanticError::DimensionMismatch {
                expected: 3,
                actual: 2
            }
        );
    }

    #[test]
    fn comparing_two_lengths_refuses_with_the_two_lengths() {
        let short = EmbeddingVector::new(vec![1.0, 0.0]).expect("valid");
        let long = EmbeddingVector::new(vec![1.0, 0.0, 0.0]).expect("valid");
        assert_eq!(
            short.cosine(&long),
            Err(SemanticError::DimensionMismatch {
                expected: 2,
                actual: 3
            })
        );
    }
}
