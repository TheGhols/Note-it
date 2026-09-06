//! One adapter per vendor, and every vendor-specific fact inside one of them.
//!
//! The Core does not learn a batch ceiling, a task type or a field name
//! (§4, §29, §30). Neither does the protocol: a request says
//! `provider`, `model`, `role` and `texts`, and what those become is decided
//! here.
//!
//! ## What the three have in common, and where they stop having it
//!
//! | | OpenAI | Gemini | Voyage |
//! | --- | --- | --- | --- |
//! | path | `/v1/embeddings` | `/v1beta/models/{model}:batchEmbedContents` | `/v1/embeddings` |
//! | auth header | `authorization: Bearer …` | `x-goog-api-key: …` | `authorization: Bearer …` |
//! | document/query | no distinction | `taskType` on `-001` only | `input_type` |
//! | dimension field | `dimensions` | `outputDimensionality` | `output_dimension` |
//! | response index | `data[].index` | none — position only | `data[].index` |
//! | documented batch | none stated | none stated | 1 000 |
//!
//! ## Why the vendors' JSON is read leniently and ours is not
//!
//! `noteit-embed-protocol` uses `deny_unknown_fields`, because both ends of
//! that wire are this repository and a field one side does not know is a
//! version skew to fail on. A vendor's response is the opposite situation: it
//! is *their* schema, they add fields without asking, and refusing an answer
//! because it grew a `usage.prompt_tokens_details` would be a client that
//! breaks on a Tuesday. So the structs below name what is needed and ignore
//! the rest — and then every value that came out is validated, which is where
//! the strictness actually belongs.

mod gemini;
mod openai;
mod voyage;

use crate::credential::Credential;
use crate::http::{Cancelled, Delay, HttpClient};
use noteit_embed_protocol::{ProviderId, Role, WireError};

/// What one adapter is asked to do.
pub struct EmbedJob<'a> {
    pub model: &'a str,
    pub role: Role,
    pub dimension: Option<u32>,
    pub texts: &'a [String],
}

/// The most texts one *provider* call may carry.
///
/// A number per vendor, kept here rather than in the Core (§29). All
/// three are larger than `noteit_embed_protocol::MAX_TEXTS`, so in this build
/// the partitioning below never actually splits — which is a reason to test
/// the partitioning against small limits directly rather than a reason not to
/// have it. A vendor that lowers its ceiling, or a protocol that raises its
/// own, changes one constant here and nothing else.
pub const fn batch_limit(provider: ProviderId) -> usize {
    match provider {
        // The current guide states no maximum. A conservative number is used
        // rather than an invented one, and it is documented as conservative.
        ProviderId::OpenAi => 128,
        // `batchEmbedContents` states no maximum either.
        ProviderId::Gemini => 100,
        // Documented: "max 1,000 items".
        ProviderId::Voyage => 1000,
    }
}

/// Splits a batch into runs no larger than `limit`, preserving order.
///
/// Its own function because §29 asks for the boundaries to be tested —
/// 0, 1, `max - 1`, `max`, `max + 1`, `2 × max` and a partial last run — and a
/// loop buried inside an HTTP call is not a thing a test can put those numbers
/// into.
pub fn partition(count: usize, limit: usize) -> Vec<std::ops::Range<usize>> {
    if count == 0 || limit == 0 {
        return Vec::new();
    }
    let mut runs = Vec::with_capacity(count.div_ceil(limit));
    let mut start = 0;
    while start < count {
        let end = (start + limit).min(count);
        runs.push(start..end);
        start = end;
    }
    runs
}

/// Embeds a whole job, one provider call per partition, in order.
///
/// The runs are concatenated in order and the total is checked against the
/// number of texts asked for. A short batch is not a smaller answer — the
/// count is the only thing saying which vector belongs to which text, so a
/// partial result is refused rather than returned (§26, §84).
pub fn embed(
    client: &HttpClient,
    provider: ProviderId,
    credential: &Credential,
    job: &EmbedJob<'_>,
    cancel: &dyn Cancelled,
    delay: &dyn Delay,
) -> Result<Vec<Vec<f32>>, WireError> {
    let mut all: Vec<Vec<f32>> = Vec::with_capacity(job.texts.len());
    for run in partition(job.texts.len(), batch_limit(provider)) {
        // Between batches as well as before the first one: a job cancelled
        // halfway must stop paying for the rest of it (§57).
        if cancel.cancelled() {
            return Err(WireError::Cancelled);
        }
        let slice = &job.texts[run.clone()];
        let part = EmbedJob {
            model: job.model,
            role: job.role,
            dimension: job.dimension,
            texts: slice,
        };
        let vectors = match provider {
            ProviderId::OpenAi => openai::embed(client, credential, &part, cancel, delay),
            ProviderId::Gemini => gemini::embed(client, credential, &part, cancel, delay),
            ProviderId::Voyage => voyage::embed(client, credential, &part, cancel, delay),
        }?;
        if vectors.len() != slice.len() {
            return Err(WireError::InvalidResponse);
        }
        all.extend(vectors);
    }
    if all.len() != job.texts.len() {
        return Err(WireError::InvalidResponse);
    }
    Ok(all)
}

/// Puts a vendor's indexed rows back into the order they were asked for.
///
/// Shared by OpenAI and Voyage, which both answer with `data[].index`. Gemini
/// does not, and its adapter says so rather than pretending.
///
/// **The whole response is refused, never repaired.** A duplicate index, a
/// missing one or one out of range means the mapping between question and
/// answer is broken, and there is no partial reading of that which is safe:
/// picking the first of two rows claiming index 3 attaches one paragraph's
/// meaning to another paragraph's identity, which is the exact failure the
/// batch is atomic to prevent (§84).
pub fn reorder_by_index(
    mut rows: Vec<(usize, Vec<f32>)>,
    expected: usize,
) -> Result<Vec<Vec<f32>>, WireError> {
    if rows.len() != expected {
        return Err(WireError::InvalidResponse);
    }
    rows.sort_by_key(|(index, _)| *index);
    let mut ordered = Vec::with_capacity(expected);
    for (position, (index, vector)) in rows.into_iter().enumerate() {
        // After sorting, the indices must be exactly 0..expected. Anything
        // else — a gap, a repeat, a negative that arrived as a huge usize —
        // fails this one comparison.
        if index != position {
            return Err(WireError::InvalidResponse);
        }
        ordered.push(vector);
    }
    Ok(ordered)
}

/// Every vector a vendor returned, checked before any of it is believed.
///
/// Dimension exactly as asked when the caller asked; otherwise the dimension
/// of the first vector, which every other vector must then share. Finite
/// everywhere. Non-empty. §59, in one place so no adapter can forget a
/// clause of it.
pub fn validate_vectors(
    vectors: &[Vec<f32>],
    expected_count: usize,
    requested_dimension: Option<u32>,
) -> Result<u32, WireError> {
    if vectors.len() != expected_count || vectors.is_empty() {
        return Err(WireError::InvalidResponse);
    }
    let dimension = match requested_dimension {
        Some(asked) => asked as usize,
        None => vectors[0].len(),
    };
    if dimension == 0 || dimension > noteit_embed_protocol::MAX_DIMENSION {
        return Err(WireError::DimensionMismatch);
    }
    for vector in vectors {
        if vector.len() != dimension {
            return Err(WireError::DimensionMismatch);
        }
        if vector.iter().any(|value| !value.is_finite()) {
            return Err(WireError::InvalidResponse);
        }
    }
    Ok(dimension as u32)
}

/// Parses a vendor body, or says the answer was not one.
///
/// The parse error is dropped on purpose. `serde_json`'s message quotes the
/// input at the position it failed, and the input is a provider's response to
/// a request that carried the user's notes.
pub fn parse<T: for<'de> serde::Deserialize<'de>>(body: &[u8]) -> Result<T, WireError> {
    serde_json::from_slice(body).map_err(|_| WireError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitioning_covers_the_boundaries_the_specification_names() {
        let max = 4;
        assert_eq!(partition(0, max), Vec::<std::ops::Range<usize>>::new());
        assert_eq!(partition(1, max), vec![0..1]);
        assert_eq!(partition(max - 1, max), vec![0..3]);
        assert_eq!(partition(max, max), vec![0..4]);
        assert_eq!(partition(max + 1, max), vec![0..4, 4..5]);
        assert_eq!(partition(2 * max, max), vec![0..4, 4..8]);
        // A partial last run, which is the case an off-by-one loses.
        assert_eq!(partition(2 * max + 3, max), vec![0..4, 4..8, 8..11]);
    }

    #[test]
    fn partitioning_preserves_order_and_covers_everything_exactly_once() {
        for count in 0..40usize {
            for limit in 1..9usize {
                let runs = partition(count, limit);
                let mut seen: Vec<usize> = Vec::new();
                for run in &runs {
                    assert!(run.len() <= limit);
                    assert!(!run.is_empty());
                    seen.extend(run.clone());
                }
                assert_eq!(seen, (0..count).collect::<Vec<_>>(), "{count} by {limit}");
            }
        }
    }

    #[test]
    fn a_zero_limit_partitions_nothing_rather_than_looping_forever() {
        assert_eq!(partition(10, 0), Vec::<std::ops::Range<usize>>::new());
    }

    #[test]
    fn every_provider_batch_limit_is_at_least_the_protocol_ceiling() {
        // If one were smaller, `partition` would be doing real work in this
        // build and the tests above would be the only thing covering it.
        for provider in ProviderId::ALL {
            assert!(
                batch_limit(provider) >= noteit_embed_protocol::MAX_TEXTS,
                "{provider} batches fewer than the protocol allows"
            );
        }
    }

    // ------------------------------------------------------------ ordering

    #[test]
    fn indexed_rows_come_back_in_the_order_they_were_asked_for() {
        let rows = vec![(2, vec![2.0]), (0, vec![0.0]), (1, vec![1.0])];
        let ordered = reorder_by_index(rows, 3).expect("reorder");
        assert_eq!(ordered, vec![vec![0.0], vec![1.0], vec![2.0]]);
    }

    #[test]
    fn a_duplicated_index_rejects_the_whole_response() {
        let rows = vec![(0, vec![0.0]), (0, vec![9.0]), (2, vec![2.0])];
        assert_eq!(
            reorder_by_index(rows, 3).unwrap_err(),
            WireError::InvalidResponse
        );
    }

    #[test]
    fn a_missing_index_rejects_the_whole_response() {
        let rows = vec![(0, vec![0.0]), (2, vec![2.0])];
        assert_eq!(
            reorder_by_index(rows, 3).unwrap_err(),
            WireError::InvalidResponse
        );
    }

    #[test]
    fn an_index_out_of_range_rejects_the_whole_response() {
        for bad in [3usize, 99, usize::MAX] {
            let rows = vec![(0, vec![0.0]), (1, vec![1.0]), (bad, vec![2.0])];
            assert_eq!(
                reorder_by_index(rows, 3).unwrap_err(),
                WireError::InvalidResponse
            );
        }
    }

    #[test]
    fn fewer_or_more_rows_than_inputs_rejects_the_whole_response() {
        assert_eq!(
            reorder_by_index(vec![(0, vec![0.0])], 2).unwrap_err(),
            WireError::InvalidResponse
        );
        assert_eq!(
            reorder_by_index(vec![(0, vec![0.0]), (1, vec![1.0])], 1).unwrap_err(),
            WireError::InvalidResponse
        );
    }

    // ---------------------------------------------------------- validation

    #[test]
    fn a_vector_that_is_not_finite_is_refused() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                validate_vectors(&[vec![1.0, bad]], 1, Some(2)).unwrap_err(),
                WireError::InvalidResponse
            );
        }
    }

    #[test]
    fn a_vector_of_the_wrong_length_is_refused() {
        assert_eq!(
            validate_vectors(&[vec![1.0, 1.0]], 1, Some(3)).unwrap_err(),
            WireError::DimensionMismatch
        );
    }

    #[test]
    fn vectors_of_differing_lengths_are_refused_even_with_no_dimension_asked() {
        assert_eq!(
            validate_vectors(&[vec![1.0, 1.0], vec![1.0]], 2, None).unwrap_err(),
            WireError::DimensionMismatch
        );
    }

    #[test]
    fn an_unasked_dimension_is_taken_from_the_answer() {
        assert_eq!(validate_vectors(&[vec![1.0; 7]], 1, None).expect("ok"), 7);
    }

    #[test]
    fn an_empty_answer_is_refused() {
        assert_eq!(
            validate_vectors(&[], 0, Some(4)).unwrap_err(),
            WireError::InvalidResponse
        );
        assert_eq!(
            validate_vectors(&[vec![]], 1, None).unwrap_err(),
            WireError::DimensionMismatch
        );
    }

    #[test]
    fn a_dimension_beyond_the_protocol_ceiling_is_refused() {
        let huge = (noteit_embed_protocol::MAX_DIMENSION + 1) as u32;
        assert_eq!(
            validate_vectors(&[vec![1.0]], 1, Some(huge)).unwrap_err(),
            WireError::DimensionMismatch
        );
    }
}
