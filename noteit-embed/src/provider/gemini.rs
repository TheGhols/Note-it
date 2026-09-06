//! Google Gemini, against the official documentation as of **2026-09-06**.
//!
//! Sources read for this adapter, both primary:
//! `ai.google.dev/gemini-api/docs/embeddings` and `ai.google.dev/api/embeddings`.
//!
//! ```text
//! POST https://generativelanguage.googleapis.com
//!      /v1beta/models/{model}:batchEmbedContents
//! x-goog-api-key: $GEMINI_API_KEY
//! content-type: application/json
//!
//! { "requests": [
//!     { "model": "models/gemini-embedding-001",
//!       "content": { "parts": [ { "text": "…" } ] },
//!       "taskType": "RETRIEVAL_DOCUMENT",     ← 001 only
//!       "outputDimensionality": 3072 } ] }
//!
//! 200 { "embeddings": [ { "values": [ … ] } ] }
//! ```
//!
//! ## The two things that make this vendor different
//!
//! **The model is in the URL.** `…/models/{model}:batchEmbedContents` means a
//! model name is part of a path, which is why
//! `noteit_embed_protocol::is_model_token` exists and why it refuses `/`,
//! `..`, `?`, `#`, `@` and percent escapes. Without it, "the worker only talks
//! to three pinned hosts" would be true of the host and false of everything
//! after it. The check is applied again here, immediately before the path is
//! built, so this file does not depend on a caller having done it.
//!
//! **The response has no index.** OpenAI and Voyage return `data[].index` and
//! this returns a bare `embeddings` array, so order *is* position. That is not
//! a weaker guarantee if it is stated: the count is checked against the number
//! of requests sent, and a response of a different length is refused whole.
//! What is not done is pretend an index was validated when there was none.
//!
//! ## Roles
//!
//! `gemini-embedding-001` takes `taskType`, and the two values that matter for
//! retrieval are `RETRIEVAL_DOCUMENT` and `RETRIEVAL_QUERY`.
//! `gemini-embedding-2` does **not** accept the parameter — the documentation
//! says task context goes in the prompt instead — so this adapter sends it
//! only for models that take it. Sending it to `-2` would be a 400 on every
//! request; inventing a prompt prefix for `-2` would move both halves of the
//! recipe out of the space the weights describe, so recipe 1 for that model
//! prepares both roles identically and the space records which model it was.
//!
//! **Dimensions** are 128–3072 for both models. Input is 8 192 tokens for
//! `-2` and 2 048 for `-001`. `-2` auto-normalises truncated dimensions;
//! `-001` does not below 3072, and `crate::provider::validate_vectors` does
//! not normalise on its behalf — the Core's cosine uses the norm it is given
//! (§60).
//!
//! **Batching.** `batchEmbedContents` states no maximum; `batch_limit` uses a
//! conservative 100.

use super::{parse, validate_vectors, EmbedJob};
use crate::credential::Credential;
use crate::http::{status_to_error, Cancelled, Delay, HttpClient, HttpRequest};
use noteit_embed_protocol::{is_model_token, ProviderId, Role, WireError};
use serde::{Deserialize, Serialize};

/// The models that accept `taskType`.
///
/// A list and not a version comparison: "everything before `-2`" is a rule
/// that would have to guess about a model nobody has seen.
pub const TASK_TYPE_MODELS: [&str; 1] = ["gemini-embedding-001"];

pub fn accepts_task_type(model: &str) -> bool {
    TASK_TYPE_MODELS.contains(&model)
}

/// The vendor's word for a role.
pub const fn task_type(role: Role) -> &'static str {
    match role {
        Role::Document => "RETRIEVAL_DOCUMENT",
        Role::Query => "RETRIEVAL_QUERY",
    }
}

/// The path for one model, built only from constants and a validated token.
pub fn path_for(model: &str) -> Result<String, WireError> {
    if !is_model_token(model) {
        return Err(WireError::Unsupported);
    }
    Ok(format!("/v1beta/models/{model}:batchEmbedContents"))
}

#[derive(Serialize)]
struct Batch<'a> {
    requests: Vec<One<'a>>,
}

#[derive(Serialize)]
struct One<'a> {
    model: String,
    content: Content<'a>,
    #[serde(rename = "taskType", skip_serializing_if = "Option::is_none")]
    task_type: Option<&'static str>,
    #[serde(
        rename = "outputDimensionality",
        skip_serializing_if = "Option::is_none"
    )]
    output_dimensionality: Option<u32>,
}

#[derive(Serialize)]
struct Content<'a> {
    parts: Vec<Part<'a>>,
}

#[derive(Serialize)]
struct Part<'a> {
    text: &'a str,
}

#[derive(Deserialize)]
struct Response {
    embeddings: Vec<ContentEmbedding>,
}

#[derive(Deserialize)]
struct ContentEmbedding {
    values: Vec<f32>,
}

fn body_for(job: &EmbedJob<'_>) -> Result<Vec<u8>, WireError> {
    if !is_model_token(job.model) {
        return Err(WireError::Unsupported);
    }
    let qualified = format!("models/{}", job.model);
    let task = accepts_task_type(job.model).then(|| task_type(job.role));
    let batch = Batch {
        requests: job
            .texts
            .iter()
            .map(|text| One {
                model: qualified.clone(),
                content: Content {
                    parts: vec![Part { text }],
                },
                task_type: task,
                output_dimensionality: job.dimension,
            })
            .collect(),
    };
    serde_json::to_vec(&batch).map_err(|_| WireError::InvalidResponse)
}

pub fn embed(
    client: &HttpClient,
    credential: &Credential,
    job: &EmbedJob<'_>,
    cancel: &dyn Cancelled,
    delay: &dyn Delay,
) -> Result<Vec<Vec<f32>>, WireError> {
    let path = path_for(job.model)?;
    let body = body_for(job)?;

    let answer = client.send(
        &HttpRequest {
            provider: ProviderId::Gemini,
            path,
            body,
            // Not `authorization`. This vendor's own header, so a key never
            // arrives with a `Bearer ` prefix it did not ask for.
            auth_header: "x-goog-api-key",
            credential,
            bearer: false,
        },
        cancel,
        delay,
    )?;

    if let Some(error) = status_to_error(answer.status) {
        return Err(error);
    }
    let parsed: Response = parse(&answer.body)?;
    // No index to reorder by: this vendor answers positionally. The count is
    // the guarantee, and it is checked rather than assumed.
    if parsed.embeddings.len() != job.texts.len() {
        return Err(WireError::InvalidResponse);
    }
    let vectors: Vec<Vec<f32>> = parsed
        .embeddings
        .into_iter()
        .map(|embedding| embedding.values)
        .collect();
    validate_vectors(&vectors, job.texts.len(), job.dimension)?;
    Ok(vectors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job<'a>(
        model: &'a str,
        role: Role,
        texts: &'a [String],
        dimension: Option<u32>,
    ) -> EmbedJob<'a> {
        EmbedJob {
            model,
            role,
            dimension,
            texts,
        }
    }

    #[test]
    fn the_path_is_the_documented_one() {
        assert_eq!(
            path_for("gemini-embedding-001").expect("path"),
            "/v1beta/models/gemini-embedding-001:batchEmbedContents"
        );
    }

    #[test]
    fn a_model_name_that_could_leave_the_endpoint_is_refused_here_too() {
        // The protocol already refuses these. Checked again at the one place
        // that concatenates, so this file is correct on its own.
        for hostile in [
            "../../v1beta/models/x",
            "a/b",
            "..",
            ".hidden",
            "x?key=leak",
            "x#frag",
            "x@evil.test",
            "",
        ] {
            assert_eq!(path_for(hostile).unwrap_err(), WireError::Unsupported);
        }
    }

    #[test]
    fn a_hostile_model_name_never_reaches_a_body_either() {
        let texts = vec!["a".to_string()];
        assert_eq!(
            body_for(&job("../x", Role::Document, &texts, None)).unwrap_err(),
            WireError::Unsupported
        );
    }

    #[test]
    fn the_versioned_model_gets_a_task_type_and_the_roles_differ() {
        let texts = vec!["a".to_string()];
        let document = body_for(&job(
            "gemini-embedding-001",
            Role::Document,
            &texts,
            Some(3072),
        ))
        .expect("document");
        let query = body_for(&job(
            "gemini-embedding-001",
            Role::Query,
            &texts,
            Some(3072),
        ))
        .expect("query");
        let document: serde_json::Value = serde_json::from_slice(&document).expect("json");
        let query: serde_json::Value = serde_json::from_slice(&query).expect("json");
        assert_eq!(document["requests"][0]["taskType"], "RETRIEVAL_DOCUMENT");
        assert_eq!(query["requests"][0]["taskType"], "RETRIEVAL_QUERY");
        assert_ne!(document, query);
    }

    #[test]
    fn the_model_that_does_not_take_a_task_type_is_not_sent_one() {
        let texts = vec!["a".to_string()];
        for role in [Role::Document, Role::Query] {
            let encoded = body_for(&job("gemini-embedding-2", role, &texts, None)).expect("body");
            let value: serde_json::Value = serde_json::from_slice(&encoded).expect("json");
            assert!(
                value["requests"][0].get("taskType").is_none(),
                "gemini-embedding-2 does not accept taskType and must not be sent one"
            );
        }
    }

    #[test]
    fn the_body_is_the_documented_shape_and_carries_only_the_text() {
        let texts = vec!["primeiro".to_string(), "segundo".to_string()];
        let encoded = body_for(&job(
            "gemini-embedding-001",
            Role::Document,
            &texts,
            Some(768),
        ))
        .expect("body");
        let value: serde_json::Value = serde_json::from_slice(&encoded).expect("json");
        let requests = value["requests"].as_array().expect("array");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["model"], "models/gemini-embedding-001");
        assert_eq!(requests[0]["content"]["parts"][0]["text"], "primeiro");
        assert_eq!(requests[1]["content"]["parts"][0]["text"], "segundo");
        assert_eq!(requests[0]["outputDimensionality"], 768);
        let mut keys: Vec<&str> = requests[0]
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["content", "model", "outputDimensionality", "taskType"]
        );
    }

    #[test]
    fn an_unasked_dimension_is_left_out() {
        let texts = vec!["a".to_string()];
        let encoded =
            body_for(&job("gemini-embedding-2", Role::Query, &texts, None)).expect("body");
        let value: serde_json::Value = serde_json::from_slice(&encoded).expect("json");
        assert!(value["requests"][0].get("outputDimensionality").is_none());
    }

    #[test]
    fn a_documented_response_parses_positionally() {
        let body = br#"{"embeddings":[{"values":[1.0,0.0]},{"values":[0.0,1.0]}],
            "usageMetadata":{"promptTokenCount":4}}"#;
        let parsed: Response = parse(body).expect("parse");
        assert_eq!(parsed.embeddings.len(), 2);
        assert_eq!(parsed.embeddings[0].values, vec![1.0, 0.0]);
        assert_eq!(parsed.embeddings[1].values, vec![0.0, 1.0]);
    }

    #[test]
    fn a_response_that_grew_a_field_still_parses() {
        let body =
            br#"{"embeddings":[{"values":[1.0],"shape":[1],"statistics":{}}],"modelVersion":"x"}"#;
        let parsed: Response = parse(body).expect("a vendor may add fields");
        assert_eq!(parsed.embeddings.len(), 1);
    }

    #[test]
    fn a_response_without_the_embeddings_array_is_not_a_response() {
        // `Response` holds vectors derived from a note, so it deliberately
        // has no `Debug` and `unwrap_err` cannot be used on it — matching the
        // error is the assertion, and the absent derive is the point.
        assert!(matches!(
            parse::<Response>(br#"{"usageMetadata":{}}"#),
            Err(WireError::InvalidResponse)
        ));
    }

    #[test]
    fn the_task_type_words_are_the_vendors_own() {
        assert_eq!(task_type(Role::Document), "RETRIEVAL_DOCUMENT");
        assert_eq!(task_type(Role::Query), "RETRIEVAL_QUERY");
    }
}
