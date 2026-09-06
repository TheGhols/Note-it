//! OpenAI, against the official documentation as of **2026-09-06**.
//!
//! Sources read for this adapter, both primary:
//! `developers.openai.com/api/docs/guides/embeddings` and
//! `developers.openai.com/api/docs/guides/rate-limits`.
//!
//! ```text
//! POST https://api.openai.com/v1/embeddings
//! authorization: Bearer $OPENAI_API_KEY
//! content-type: application/json
//!
//! { "model": "text-embedding-3-small",
//!   "input": ["…", "…"],
//!   "dimensions": 1536,          ← optional; reduces the vector
//!   "encoding_format": "float" }
//!
//! 200 { "object": "list",
//!       "data": [ { "object": "embedding", "index": 0, "embedding": [ … ] } ],
//!       "model": "text-embedding-3-small",
//!       "usage": { "prompt_tokens": 8, "total_tokens": 8 } }
//! ```
//!
//! **No document/query distinction.** The guide documents no `input_type`,
//! `task_type` or prefix for these models, so recipe 1 for this provider
//! prepares both roles identically — and says so, rather than inventing a
//! `passage: ` prefix the model was not trained with. That is a fact about
//! this vendor recorded here, which is exactly where §30 says vendor
//! rules belong.
//!
//! **Models and dimensions**, from the table in the guide:
//! `text-embedding-3-small` 1536, `text-embedding-3-large` 3072,
//! `text-embedding-ada-002` 1536 fixed. Maximum input 8 192 tokens for all
//! three. The first two accept `dimensions`; `ada-002` does not, and asking it
//! for one is the vendor's 400 rather than something to guess about here.
//!
//! **Batching.** The guide states no maximum array size. `batch_limit` uses a
//! conservative 128 and is documented as conservative rather than as
//! documented.
//!
//! **Rate limits.** 429 with a `Retry-After` in seconds, and 503 for a
//! temporarily overloaded model. Both are handled by `crate::http`, which
//! honours the header up to a ceiling.

use super::{parse, reorder_by_index, validate_vectors, EmbedJob};
use crate::credential::Credential;
use crate::http::{status_to_error, Cancelled, Delay, HttpClient, HttpRequest};
use noteit_embed_protocol::{ProviderId, WireError};
use serde::{Deserialize, Serialize};

pub const PATH: &str = "/v1/embeddings";

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    input: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<u32>,
    /// Stated rather than left to the default, so the response shape this
    /// adapter parses is the one it asked for. `base64` is the alternative and
    /// would silently make `embedding` a string.
    encoding_format: &'static str,
}

/// What is read out of a 200. Unknown fields are ignored — see the module
/// documentation on `provider/mod.rs` for why the asymmetry with our own wire
/// is deliberate.
#[derive(Deserialize)]
struct Response {
    data: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    index: usize,
    embedding: Vec<f32>,
}

pub fn embed(
    client: &HttpClient,
    credential: &Credential,
    job: &EmbedJob<'_>,
    cancel: &dyn Cancelled,
    delay: &dyn Delay,
) -> Result<Vec<Vec<f32>>, WireError> {
    let body = serde_json::to_vec(&Request {
        model: job.model,
        input: job.texts,
        dimensions: job.dimension,
        encoding_format: "float",
    })
    .map_err(|_| WireError::InvalidResponse)?;

    let answer = client.send(
        &HttpRequest {
            provider: ProviderId::OpenAi,
            // A constant. The model is in the body for this vendor, so nothing
            // a caller controls reaches the path at all.
            path: PATH.to_string(),
            body,
            auth_header: "authorization",
            credential,
            bearer: true,
        },
        cancel,
        delay,
    )?;

    if let Some(error) = status_to_error(answer.status) {
        return Err(error);
    }
    let parsed: Response = parse(&answer.body)?;
    let rows: Vec<(usize, Vec<f32>)> = parsed
        .data
        .into_iter()
        .map(|row| (row.index, row.embedding))
        .collect();
    let ordered = reorder_by_index(rows, job.texts.len())?;
    validate_vectors(&ordered, job.texts.len(), job.dimension)?;
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_is_the_documented_shape() {
        let texts = vec!["um".to_string(), "dois".to_string()];
        let encoded = serde_json::to_value(Request {
            model: "text-embedding-3-small",
            input: &texts,
            dimensions: Some(1536),
            encoding_format: "float",
        })
        .expect("encode");
        assert_eq!(encoded["model"], "text-embedding-3-small");
        assert_eq!(encoded["input"][0], "um");
        assert_eq!(encoded["input"][1], "dois");
        assert_eq!(encoded["dimensions"], 1536);
        assert_eq!(encoded["encoding_format"], "float");
    }

    #[test]
    fn an_unasked_dimension_is_left_out_rather_than_sent_as_null() {
        let texts = vec!["um".to_string()];
        let encoded = serde_json::to_value(Request {
            model: "text-embedding-ada-002",
            input: &texts,
            dimensions: None,
            encoding_format: "float",
        })
        .expect("encode");
        assert!(
            encoded.get("dimensions").is_none(),
            "ada-002 does not accept `dimensions`, and a null is not the same as absent"
        );
    }

    #[test]
    fn the_request_carries_nothing_but_model_input_and_shape() {
        // The minimisation rule, asserted rather than reviewed: whatever else
        // exists in the process, four keys leave it.
        let texts = vec!["um".to_string()];
        let encoded = serde_json::to_value(Request {
            model: "m",
            input: &texts,
            dimensions: Some(8),
            encoding_format: "float",
        })
        .expect("encode");
        let object = encoded.as_object().expect("object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["dimensions", "encoding_format", "input", "model"]);
    }

    #[test]
    fn a_documented_response_parses() {
        let body = br#"{"object":"list","data":[
            {"object":"embedding","index":1,"embedding":[0.5,0.5]},
            {"object":"embedding","index":0,"embedding":[1.0,0.0]}],
            "model":"text-embedding-3-small","usage":{"prompt_tokens":8,"total_tokens":8}}"#;
        let parsed: Response = parse(body).expect("parse");
        let rows: Vec<(usize, Vec<f32>)> = parsed
            .data
            .into_iter()
            .map(|row| (row.index, row.embedding))
            .collect();
        let ordered = reorder_by_index(rows, 2).expect("reorder");
        assert_eq!(ordered, vec![vec![1.0, 0.0], vec![0.5, 0.5]]);
    }

    #[test]
    fn a_response_that_grew_a_field_still_parses() {
        let body = br#"{"object":"list","data":[{"object":"embedding","index":0,
            "embedding":[1.0],"provenance":"whatever"}],"model":"m",
            "usage":{"prompt_tokens":1,"total_tokens":1,"prompt_tokens_details":{"cached":0}},
            "system_fingerprint":"fp_x"}"#;
        let parsed: Response = parse(body).expect("a vendor may add fields");
        assert_eq!(parsed.data.len(), 1);
    }

    #[test]
    fn a_response_missing_the_data_array_is_not_a_response() {
        // `Response` holds vectors derived from a note, so it deliberately
        // has no `Debug` and `unwrap_err` cannot be used on it — matching the
        // error is the assertion, and the absent derive is the point.
        assert!(matches!(
            parse::<Response>(br#"{"object":"list"}"#),
            Err(WireError::InvalidResponse)
        ));
    }

    #[test]
    fn the_path_never_carries_anything_a_caller_chose() {
        assert_eq!(PATH, "/v1/embeddings");
        assert!(!PATH.contains('{'));
    }
}
