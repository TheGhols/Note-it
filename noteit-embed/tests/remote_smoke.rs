//! Two things that do not run by default, for two different reasons.
//!
//! ## The sentinel sweep (§100)
//!
//! Runs always. It drives the worker through every failure that can carry a
//! vendor's words — 401, 429, 500, a body that is not JSON, a truncated one, a
//! vector full of `NaN`, and a credential that is not there — and then searches
//! everything the process produced for the two sentinels. If either appears
//! outside the mock, the phase is `BLOCKED` and this test says so.
//!
//! ## The real smoke test (§48, §90)
//!
//! `#[ignore]`, and additionally gated on `NOTEIT_REMOTE_SMOKE=1` plus a
//! credential in the environment. It is the only code in this repository that
//! can reach a vendor, it costs money, and it sends **synthetic text only** —
//! never a note, never a fixture, never anything from a store. Without the
//! variable it prints an explicit skip; a missing key is a skip and never a
//! pass.

#[cfg(feature = "test-endpoints")]
mod support;

#[cfg(not(feature = "test-endpoints"))]
#[test]
fn the_sentinel_sweep_is_skipped_without_its_feature() {
    eprintln!(
        "remote_smoke: pulado — este arquivo precisa de `--features test-endpoints`; \
         `scripts/check embed-tests` o executa."
    );
}

#[cfg(feature = "test-endpoints")]
mod sweep {
    use super::support::{indexed_body, MockProvider, RecordingDelay, Reply};
    use noteit_embed::credential::{Credential, CredentialError};
    use noteit_embed::endpoint::test_override;
    use noteit_embed::http::HttpClient;
    use noteit_embed::provider::{self, EmbedJob};
    use noteit_embed::server::{Limits, Worker};
    use noteit_embed_protocol::{
        EmbedRequestV1, EmbedResponseV1, ProviderId, Role, PROTOCOL_VERSION,
    };
    use std::path::PathBuf;

    /// The two values that must never leave the mock.
    const SECRET: &str = "NOTEIT_TEST_SECRET_NEVER_EXPOSE_7F4A2";
    const NOTE: &str = "NOTE_TEXT_PRIVATE_456";

    fn scratch(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "noteit-embed-sweep-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("mkdir");
        base
    }

    /// A provider body that names both sentinels, the way a hostile or merely
    /// chatty vendor would.
    fn hostile_body() -> String {
        format!(
            r#"{{"error":{{"message":"chave {SECRET} recusada ao embutir {NOTE}","type":"invalid_request_error","request_id":"req_SENTINEL_LEAK","param":"{NOTE}"}}}}"#
        )
    }

    /// Everything one attempt produced, as text a search can run over.
    fn transcript(outcome: &str) -> String {
        outcome.to_string()
    }

    #[test]
    fn no_failure_path_lets_a_sentinel_out() {
        let mut produced = Vec::new();

        // Every HTTP failure, with the sentinels in the body.
        for status in [400u16, 401, 403, 404, 408, 429, 500, 502, 503] {
            for provider in ProviderId::ALL {
                let mock = MockProvider::always(status, &hostile_body());
                test_override::set(provider, mock.base_url());
                let outcome = provider::embed(
                    &HttpClient::new(),
                    provider,
                    &Credential::new(SECRET),
                    &EmbedJob {
                        model: match provider {
                            ProviderId::OpenAi => "text-embedding-3-small",
                            ProviderId::Gemini => "gemini-embedding-001",
                            ProviderId::Voyage => "voyage-4",
                        },
                        role: Role::Document,
                        dimension: Some(2),
                        texts: &[format!("{NOTE} um parágrafo")],
                    },
                    &noteit_embed::http::NeverCancelled,
                    &RecordingDelay::default(),
                );
                produced.push(transcript(&format!("{outcome:?}")));
            }
        }

        // Bodies that are not answers.
        for body in [
            "<html>proxy error</html>".to_string(),
            r#"{"data":[{"index":0,"embedding":[1.0,"#.to_string(),
            r#"{"data":[{"index":0,"embedding":[1.0,null]}]}"#.to_string(),
            hostile_body(),
        ] {
            let mock = MockProvider::always(200, &body);
            test_override::set(ProviderId::OpenAi, mock.base_url());
            let outcome = provider::embed(
                &HttpClient::new(),
                ProviderId::OpenAi,
                &Credential::new(SECRET),
                &EmbedJob {
                    model: "text-embedding-3-small",
                    role: Role::Query,
                    dimension: Some(2),
                    texts: &[NOTE.to_string()],
                },
                &noteit_embed::http::NeverCancelled,
                &RecordingDelay::default(),
            );
            produced.push(transcript(&format!("{outcome:?}")));
        }

        // A truncated response and a hangup, which are transport failures
        // rather than HTTP ones.
        for reply in [Reply::Truncated(200, indexed_body(1, 2)), Reply::Hangup] {
            let mock = MockProvider::start(vec![reply]);
            test_override::set(ProviderId::OpenAi, mock.base_url());
            let outcome = provider::embed(
                &HttpClient::new(),
                ProviderId::OpenAi,
                &Credential::new(SECRET),
                &EmbedJob {
                    model: "text-embedding-3-small",
                    role: Role::Document,
                    dimension: Some(2),
                    texts: &[NOTE.to_string()],
                },
                &noteit_embed::http::NeverCancelled,
                &RecordingDelay::default(),
            );
            produced.push(transcript(&format!("{outcome:?}")));
        }

        // The whole worker, over the wire, with no credential to find. This is
        // the path a real deployment takes when a key is missing, and the
        // response that comes back is the one a client would show somebody.
        let worker = Worker::new(scratch("nocred"), Limits::default());
        let response = worker.run(
            &EmbedRequestV1 {
                protocol_version: PROTOCOL_VERSION,
                provider: ProviderId::OpenAi,
                model: "text-embedding-3-small".to_string(),
                role: Role::Document,
                dimension: Some(2),
                texts: vec![format!("{NOTE} e mais texto")],
            },
            &noteit_embed::http::NeverCancelled,
            &RecordingDelay::default(),
        );
        assert!(matches!(response, EmbedResponseV1::Err { .. }));
        produced.push(transcript(&format!("{response:?}")));
        // And what actually goes on the wire, byte for byte.
        let mut framed = Vec::new();
        noteit_embed_protocol::write_frame(&mut framed, &response).expect("frame");
        produced.push(String::from_utf8_lossy(&framed).into_owned());

        // Every credential refusal, printed.
        for error in [
            CredentialError::Missing,
            CredentialError::Insecure,
            CredentialError::Unreadable,
        ] {
            produced.push(transcript(&format!("{error:?}")));
        }
        // And the credential type itself, formatted every way there is.
        let secret = Credential::new(SECRET);
        produced.push(transcript(&format!("{secret:?}")));

        // The sweep.
        for (index, text) in produced.iter().enumerate() {
            assert!(
                !text.contains(SECRET),
                "output {index} carries the credential sentinel: {text}"
            );
            assert!(
                !text.contains(NOTE),
                "output {index} carries the note sentinel: {text}"
            );
            assert!(
                !text.contains("req_SENTINEL_LEAK"),
                "output {index} carries the vendor's request id: {text}"
            );
        }
        assert!(
            produced.len() >= 35,
            "the sweep only produced {} outputs; it is not covering the matrix",
            produced.len()
        );
    }
}

// -------------------------------------------------------- the real thing

/// A real request to a real provider. Never runs unless somebody asks twice.
///
/// ```text
/// NOTEIT_REMOTE_SMOKE=1 OPENAI_API_KEY=… \
///   cargo test -p noteit-embed --features test-endpoints \
///   --test remote_smoke -- --ignored --nocapture
/// ```
///
/// The text is synthetic and stated here in full, so that reading this test is
/// enough to know what would be sent. No note, no fixture and nothing from a
/// store ever reaches this path (§90).
#[test]
#[ignore = "reaches a real provider and costs money; needs NOTEIT_REMOTE_SMOKE=1 and a key"]
fn a_real_provider_answers_a_synthetic_question() {
    if std::env::var("NOTEIT_REMOTE_SMOKE").as_deref() != Ok("1") {
        eprintln!("remote_smoke: pulado — NOTEIT_REMOTE_SMOKE não está em 1.");
        return;
    }
    #[cfg(feature = "test-endpoints")]
    {
        use noteit_embed::credential::{environment_variable, Credential};
        use noteit_embed::http::{HttpClient, NeverCancelled, RealDelay};
        use noteit_embed::provider::{self, EmbedJob};
        use noteit_embed_protocol::{ProviderId, Role};

        // The one and only text this test may send.
        const SYNTHETIC: &str = "noteit remote embedding smoke test";

        let mut attempted = 0;
        for provider in ProviderId::ALL {
            let Ok(key) = std::env::var(environment_variable(provider)) else {
                eprintln!(
                    "remote_smoke: {provider} pulado — {} não está no ambiente.",
                    environment_variable(provider)
                );
                continue;
            };
            attempted += 1;
            let model = match provider {
                ProviderId::OpenAi => "text-embedding-3-small",
                ProviderId::Gemini => "gemini-embedding-001",
                ProviderId::Voyage => "voyage-4-lite",
            };
            // The smallest batch there is: one text.
            let vectors = provider::embed(
                &HttpClient::new(),
                provider,
                &Credential::new(key),
                &EmbedJob {
                    model,
                    role: Role::Query,
                    dimension: None,
                    texts: &[SYNTHETIC.to_string()],
                },
                &NeverCancelled,
                &RealDelay,
            )
            .unwrap_or_else(|error| panic!("{provider} refused: {error:?}"));
            assert_eq!(vectors.len(), 1);
            assert!(!vectors[0].is_empty());
            assert!(vectors[0].iter().all(|value| value.is_finite()));
            eprintln!(
                "remote_smoke: {provider}/{model} devolveu {} dimensões.",
                vectors[0].len()
            );
        }
        if attempted == 0 {
            // An explicit skip, never a quiet pass.
            eprintln!("remote_smoke: nenhuma credencial no ambiente; nada foi medido.");
        }
    }
    #[cfg(not(feature = "test-endpoints"))]
    eprintln!("remote_smoke: pulado — precisa de `--features test-endpoints`.");
}
