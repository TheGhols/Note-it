//! What a remote provider's vectors may be compared with, and how honest that
//! answer is.
//!
//! ## The recipe, version 1, in full
//!
//! So that a later version is a decision and not a drift:
//!
//! * the text reaches the vendor exactly as the chunker produced it, with **no
//!   Note-it prefix on either side**. Where a vendor has its own protocol for
//!   the document/query distinction — Voyage's `input_type`, Gemini's
//!   `taskType` — the adapter sets it; where a vendor documents none, as OpenAI
//!   does, both roles are prepared identically and that is recorded rather than
//!   compensated for with a `passage: ` the model was not trained with;
//! * no truncation is applied here. Each vendor documents its own input limit
//!   and its own truncation behaviour, and a second, different cut applied
//!   first would change what was embedded without changing what the space
//!   says;
//! * **nothing is normalised by Note-it.** The vector arrives as the vendor
//!   produced it and is used with the norm it has. Some vendors normalise and
//!   some do not — `gemini-embedding-2` auto-normalises truncated dimensions,
//!   `gemini-embedding-001` does not below 3072 — and the Core's cosine divides
//!   by the norms it is given either way, so it is correct for both without
//!   this layer silently rescaling anything (§60).
//!
//! ## Verifiable and unverifiable, said plainly
//!
//! A local artifact's identity is the digest of bytes that were loaded. There
//! is no equivalent here and this module does not invent one: what a remote
//! space records is **the name the vendor was asked for**, and how much that is
//! worth depends on whether the vendor promises the name is immutable.
//!
//! [`ArtifactIdentity::ProviderPinned`] therefore means *"the vendor names this
//! model and we recorded that name"* — strong because the vendor's own
//! versioning promises the weights behind it do not change, and weaker than a
//! digest because it is a promise rather than a measurement.
//! [`ArtifactIdentity::UnverifiableAlias`] is for a name that is openly a
//! moving target, and it is marked so the diagnostic can say so and the rebuild
//! path stays available (§32). No heuristic is offered for detecting
//! that a vendor changed the weights: a statistical test whose false negative
//! is exactly the dangerous case is worse than an admission.

use noteit_core::embedding::{ArtifactIdentity, EmbeddingSpaceId};
use noteit_embed_protocol::ProviderId;

/// The version of the **pair** of recipes — document and query together.
///
/// One number for both halves, because changing only the query side
/// invalidates comparison with already-indexed documents exactly as much as
/// changing the document side.
pub const REMOTE_EMBEDDING_RECIPE_VERSION: u32 = 1;

/// The version of Note-it's own text normalisation before embedding.
///
/// Version 1 is **none**, the same as the local provider's: the visible text
/// goes to the vendor unchanged.
pub const REMOTE_NORMALIZATION_VERSION: u32 = 1;

/// The suffix a vendor uses to say "whatever is newest".
const MUTABLE_SUFFIXES: [&str; 2] = ["-latest", "-preview"];

/// Whether a model name is one the vendor promises not to move.
///
/// A rule about the *name*, because that is all there is to go on. Anything
/// ending in a suffix that means "newest" is a moving target; every other name
/// is treated as the vendor's own version identifier, which is what
/// `text-embedding-3-small`, `gemini-embedding-001` and `voyage-4-lite` are.
pub fn is_pinned_name(model: &str) -> bool {
    !MUTABLE_SUFFIXES
        .iter()
        .any(|suffix| model.ends_with(suffix))
}

/// The identity a remote space records for its model.
pub fn identity_for(provider: ProviderId, model: &str) -> ArtifactIdentity {
    if is_pinned_name(model) {
        ArtifactIdentity::provider_pinned(provider.as_str(), model)
    } else {
        ArtifactIdentity::unverifiable_alias(model)
    }
}

/// The space a remote provider declares.
///
/// Built from configuration and never from a vendor's answer. A provider that
/// replied from a different model than it was asked for is caught where the
/// answer arrives — the dimension is checked against this — rather than by
/// letting the answer decide what space it is in, which would make the check
/// tautological.
pub fn space_for(provider: ProviderId, model: &str, dimension: usize) -> EmbeddingSpaceId {
    EmbeddingSpaceId {
        provider: provider.as_str().to_string(),
        model: model.to_string(),
        artifact: identity_for(provider, model),
        dimension,
        embedding_recipe: REMOTE_EMBEDDING_RECIPE_VERSION,
        normalization: REMOTE_NORMALIZATION_VERSION,
    }
}

/// A stable directory name for one space.
///
/// The digest of a canonical encoding of every field that decides
/// comparability, so "one directory per `EmbeddingSpaceId`" (§15) is a
/// path and not a convention. Two spaces that differ in any field get two
/// directories; the same space gets the same directory on every machine and
/// every run.
///
/// **No credential enters this.** Changing an API key for the same provider and
/// model does not make the vectors geometrically incompatible, so it must not
/// invalidate the cache (§68).
pub fn space_digest(space: &EmbeddingSpaceId) -> String {
    let artifact = match &space.artifact {
        ArtifactIdentity::LocalVerified(digest) => format!("local:{digest}"),
        ArtifactIdentity::ProviderPinned {
            provider,
            pinned_id,
        } => format!("pinned:{provider}:{pinned_id}"),
        ArtifactIdentity::UnverifiableAlias { alias } => format!("alias:{alias}"),
    };
    // A field separator that cannot occur inside a field. Every component is a
    // provider identifier, a model token or a number, and none of them may
    // contain a newline — so this concatenation has one reading, which is the
    // property `canonical_object` exists for elsewhere and the reason a bare
    // `format!` of variable-length parts is not used here.
    let encoded = format!(
        "noteit.remote-space.v1\n{}\n{}\n{}\n{}\n{}\n",
        space.provider,
        space.model,
        artifact,
        space.dimension,
        format_args!("{}:{}", space.embedding_recipe, space.normalization),
    );
    noteit_core::hashing::sha256_hex(encoded.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_versioned_name_is_pinned_and_a_latest_alias_is_not() {
        for pinned in [
            "text-embedding-3-small",
            "text-embedding-3-large",
            "gemini-embedding-001",
            "gemini-embedding-2",
            "voyage-4",
            "voyage-4-lite",
        ] {
            assert!(is_pinned_name(pinned), "{pinned}");
            assert!(identity_for(ProviderId::OpenAi, pinned).is_verifiable());
        }
        for alias in ["embedding-latest", "voyage-latest", "model-preview"] {
            assert!(!is_pinned_name(alias), "{alias}");
            assert!(!identity_for(ProviderId::Voyage, alias).is_verifiable());
        }
    }

    #[test]
    fn two_providers_with_the_same_dimension_are_two_spaces() {
        // The failure 4.3A measured: equal dimension is never enough.
        let openai = space_for(ProviderId::OpenAi, "text-embedding-3-small", 1024);
        let voyage = space_for(ProviderId::Voyage, "voyage-4", 1024);
        assert_ne!(openai, voyage);
        assert_ne!(space_digest(&openai), space_digest(&voyage));
    }

    #[test]
    fn two_models_of_one_provider_are_two_spaces() {
        let small = space_for(ProviderId::OpenAi, "text-embedding-3-small", 1536);
        let large = space_for(ProviderId::OpenAi, "text-embedding-3-large", 1536);
        assert_ne!(small, large);
        assert_ne!(space_digest(&small), space_digest(&large));
    }

    #[test]
    fn two_dimensions_of_one_model_are_two_spaces() {
        let full = space_for(ProviderId::OpenAi, "text-embedding-3-small", 1536);
        let cut = space_for(ProviderId::OpenAi, "text-embedding-3-small", 512);
        assert_ne!(full, cut);
        assert_ne!(space_digest(&full), space_digest(&cut));
    }

    #[test]
    fn a_changed_recipe_or_normalisation_is_a_different_space() {
        let base = space_for(ProviderId::Voyage, "voyage-4", 1024);
        let mut recipe = base.clone();
        recipe.embedding_recipe += 1;
        let mut normalisation = base.clone();
        normalisation.normalization += 1;
        assert_ne!(space_digest(&base), space_digest(&recipe));
        assert_ne!(space_digest(&base), space_digest(&normalisation));
        assert_ne!(space_digest(&recipe), space_digest(&normalisation));
    }

    #[test]
    fn a_pinned_model_and_an_alias_of_the_same_name_are_different_spaces() {
        let pinned = space_for(ProviderId::Gemini, "gemini-embedding-001", 3072);
        let mut aliased = pinned.clone();
        aliased.artifact = ArtifactIdentity::unverifiable_alias("gemini-embedding-001");
        assert_ne!(space_digest(&pinned), space_digest(&aliased));
    }

    #[test]
    fn the_digest_is_stable_across_runs_and_is_a_digest() {
        let space = space_for(ProviderId::OpenAi, "text-embedding-3-small", 1536);
        let first = space_digest(&space);
        let second = space_digest(&space.clone());
        assert_eq!(first, second);
        assert!(noteit_core::embedding::is_digest(&first), "{first}");
    }

    #[test]
    fn no_credential_can_reach_the_digest() {
        // §68: rotating a key must not invalidate an index. There is no
        // field for one in `EmbeddingSpaceId`, so this asserts the shape that
        // makes it true rather than a behaviour that could drift.
        let space = space_for(ProviderId::OpenAi, "text-embedding-3-small", 1536);
        let encoded = format!("{space:?}");
        for word in ["key", "token", "secret", "credential", "Bearer"] {
            assert!(
                !encoded.to_lowercase().contains(word),
                "the space carries `{word}`"
            );
        }
    }

    #[test]
    fn the_recipe_is_shared_by_both_roles() {
        // One number for the pair. Asserted because the alternative — a
        // version per half — is the mistake §5 exists to prevent.
        assert_eq!(REMOTE_EMBEDDING_RECIPE_VERSION, 1);
        assert_eq!(REMOTE_NORMALIZATION_VERSION, 1);
    }
}
