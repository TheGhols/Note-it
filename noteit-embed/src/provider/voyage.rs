//! Voyage AI, against the official documentation as of **2026-09-06**.
//!
//! Sources read for this adapter, both primary:
//! `docs.voyageai.com/reference/embeddings-api` and
//! `docs.voyageai.com/docs/rate-limits`.
//!
//! ```text
//! POST https://api.voyageai.com/v1/embeddings
//! authorization: Bearer $VOYAGE_API_KEY
//! content-type: application/json
//!
//! { "input": ["…"],
//!   "model": "voyage-4",
//!   "input_type": "document",     ← or "query", or absent
//!   "output_dimension": 1024,     ← 256 | 512 | 1024 | 2048
//!   "output_dtype": "float",
//!   "truncation": true }
//!
//! 200 { "object": "list",
//!       "data": [ { "object": "embedding", "embedding": [ … ], "index": 0 } ],
//!       "model": "voyage-4", "usage": { "total_tokens": 8 } }
//! ```
//!
//! **`input_type` is the whole reason `embed_document` and `embed_query` are
//! two functions.** The vendor prepends different instructions for each, so
//! the same text embedded under the two values produces different vectors —
//! which is the point: they are the two halves of one retrieval protocol, and
//! that is why the recipe version in an `EmbeddingSpaceId` is a version of the
//! *pair* (§5).
//!
//! **`output_dtype` is stated as `float`.** The vendor also offers `int8`,
//! `uint8`, `binary` and `ubinary`, and any of those would come back as
//! something other than an array of floats. Naming the one this adapter parses
//! means a future default change upstream is a request this build still reads
//! correctly.
//!
//! **Batching.** Documented: at most 1 000 texts per request, plus a
//! per-request token ceiling that varies by model (1M for the lite models,
//! 320K for `voyage-4`, 120K for the large and specialised ones). The text
//! ceiling in `noteit-embed-protocol` — 64 texts, 512 KiB total — is far
//! under every one of those, so no request this build can construct reaches
//! either.
//!
//! **Rate limits.** 429. The documentation does not state that a `Retry-After`
//! is sent, so `crate::http` falls back to its exponential schedule when there
//! is none — which is why the fallback exists rather than being dead code.

use super::{parse, reorder_by_index, validate_vectors, EmbedJob};
use crate::credential::Credential;
use crate::http::{status_to_error, Cancelled, Delay, HttpClient, HttpRequest};
use noteit_embed_protocol::{ProviderId, Role, WireError};
use serde::{Deserialize, Serialize};

pub const PATH: &str = "/v1/embeddings";

/// The vendor's word for a role.
pub const fn input_type(role: Role) -> &'static str {
    match role {
        Role::Document => "document",
        Role::Query => "query",
    }
}

#[derive(Serialize)]
struct Request<'a> {
    input: &'a [String],
    model: &'a str,
    input_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_dimension: Option<u32>,
    output_dtype: &'static str,
}

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
        input: job.texts,
        model: job.model,
        input_type: input_type(job.role),
        output_dimension: job.dimension,
        output_dtype: "float",
    })
    .map_err(|_| WireError::InvalidResponse)?;

    let answer = client.send(
        &HttpRequest {
            provider: ProviderId::Voyage,
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

    fn encode(role: Role, dimension: Option<u32>) -> serde_json::Value {
        let texts = vec!["um".to_string(), "dois".to_string()];
        serde_json::to_value(Request {
            input: &texts,
            model: "voyage-4",
            input_type: input_type(role),
            output_dimension: dimension,
            output_dtype: "float",
        })
        .expect("encode")
    }

    #[test]
    fn the_request_is_the_documented_shape() {
        let encoded = encode(Role::Document, Some(1024));
        assert_eq!(encoded["model"], "voyage-4");
        assert_eq!(encoded["input"][0], "um");
        assert_eq!(encoded["input"][1], "dois");
        assert_eq!(encoded["input_type"], "document");
        assert_eq!(encoded["output_dimension"], 1024);
        assert_eq!(encoded["output_dtype"], "float");
    }

    #[test]
    fn document_and_query_are_prepared_differently() {
        let document = encode(Role::Document, Some(1024));
        let query = encode(Role::Query, Some(1024));
        assert_eq!(document["input_type"], "document");
        assert_eq!(query["input_type"], "query");
        assert_ne!(document, query);
    }

    #[test]
    fn the_float_encoding_is_stated_rather_than_defaulted() {
        // `int8`, `binary` and the rest would not parse as an array of floats.
        assert_eq!(encode(Role::Query, None)["output_dtype"], "float");
    }

    #[test]
    fn an_unasked_dimension_is_left_out() {
        assert!(encode(Role::Query, None).get("output_dimension").is_none());
    }

    #[test]
    fn the_request_carries_nothing_but_the_five_documented_keys() {
        let encoded = encode(Role::Document, Some(512));
        let mut keys: Vec<&str> = encoded
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "input",
                "input_type",
                "model",
                "output_dimension",
                "output_dtype"
            ]
        );
    }

    #[test]
    fn a_documented_response_parses_and_is_reordered_by_index() {
        let body = br#"{"object":"list","data":[
            {"object":"embedding","embedding":[0.0,1.0],"index":1},
            {"object":"embedding","embedding":[1.0,0.0],"index":0}],
            "model":"voyage-4","usage":{"total_tokens":8}}"#;
        let parsed: Response = parse(body).expect("parse");
        let rows: Vec<(usize, Vec<f32>)> = parsed
            .data
            .into_iter()
            .map(|row| (row.index, row.embedding))
            .collect();
        assert_eq!(
            reorder_by_index(rows, 2).expect("reorder"),
            vec![vec![1.0, 0.0], vec![0.0, 1.0]]
        );
    }

    #[test]
    fn a_response_that_grew_a_field_still_parses() {
        let body = br#"{"object":"list","data":[{"object":"embedding","embedding":[1.0],
            "index":0,"quantization":"float"}],"model":"voyage-4",
            "usage":{"total_tokens":1,"cached_tokens":0}}"#;
        assert_eq!(parse::<Response>(body).expect("parse").data.len(), 1);
    }

    #[test]
    fn a_response_without_data_is_not_a_response() {
        // `Response` holds vectors derived from a note, so it deliberately
        // has no `Debug` and `unwrap_err` cannot be used on it — matching the
        // error is the assertion, and the absent derive is the point.
        assert!(matches!(
            parse::<Response>(br#"{"object":"list"}"#),
            Err(WireError::InvalidResponse)
        ));
    }

    #[test]
    fn the_input_type_words_are_the_vendors_own() {
        assert_eq!(input_type(Role::Document), "document");
        assert_eq!(input_type(Role::Query), "query");
    }

    #[test]
    fn a_supported_dimension_is_one_the_vendor_documents() {
        // 256, 512, 1024 and 2048 are the documented set. Not enforced here —
        // the vendor's own 400 is the authority — but recorded so a reader
        // knows what is expected to work.
        for dimension in [256u32, 512, 1024, 2048] {
            let encoded = encode(Role::Document, Some(dimension));
            assert_eq!(encoded["output_dimension"], dimension);
        }
    }

    #[test]
    fn the_path_never_carries_anything_a_caller_chose() {
        assert_eq!(PATH, "/v1/embeddings");
        assert!(!PATH.contains('{'));
    }
}
