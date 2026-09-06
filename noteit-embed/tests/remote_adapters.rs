//! The three adapters, driven against a local mock, over every failure
//! §26 names.
//!
//! Nothing here reaches a vendor. The base URL is redirected to a loopback
//! server by the `test-endpoints` feature, which is compiled out of every
//! shipped build — so `api.openai.com`, `generativelanguage.googleapis.com`
//! and `api.voyageai.com` appear in this repository's Rust sources exactly
//! once each, as the pinned constants in `endpoint.rs` (§49).
//!
//! Run by `scripts/check embed-tests`, which passes the feature.

#[cfg(feature = "test-endpoints")]
mod support;

#[cfg(not(feature = "test-endpoints"))]
#[test]
fn the_mock_suite_is_skipped_without_its_feature() {
    // An explicit, visible skip rather than an empty file. §48: the
    // absence of a condition is a skip somebody can see, never a pass.
    eprintln!(
        "remote_adapters: pulado — este arquivo precisa de `--features test-endpoints`; \
         `scripts/check embed-tests` o executa."
    );
}

#[cfg(feature = "test-endpoints")]
mod mocked {
    use super::support::{
        indexed_body, positional_body, MockProvider, RecordingDelay, Reply, Switch,
    };
    use noteit_embed::credential::Credential;
    use noteit_embed::endpoint::test_override;
    use noteit_embed::http::{HttpClient, NeverCancelled, RealDelay, MAX_BODY_BYTES};
    use noteit_embed::provider::{self, EmbedJob};
    use noteit_embed_protocol::{ProviderId, Role, WireError};
    use std::time::Duration;

    /// A synthetic key, obviously not a real one, and the sentinel §100
    /// asks for.
    const SENTINEL: &str = "NOTEIT_TEST_SECRET_NEVER_EXPOSE_7F4A2";
    const PRIVATE_TEXT: &str = "NOTE_TEXT_PRIVATE_456";

    fn texts(count: usize) -> Vec<String> {
        (0..count).map(|n| format!("parágrafo {n}")).collect()
    }

    fn run(
        provider: ProviderId,
        mock: &MockProvider,
        texts: &[String],
        dimension: Option<u32>,
    ) -> Result<Vec<Vec<f32>>, WireError> {
        test_override::set(provider, mock.base_url());
        provider::embed(
            &HttpClient::new(),
            provider,
            &Credential::new(SENTINEL),
            &EmbedJob {
                model: model_for(provider),
                role: Role::Document,
                dimension,
                texts,
            },
            &NeverCancelled,
            &RealDelay,
        )
    }

    fn model_for(provider: ProviderId) -> &'static str {
        match provider {
            ProviderId::OpenAi => "text-embedding-3-small",
            ProviderId::Gemini => "gemini-embedding-001",
            ProviderId::Voyage => "voyage-4",
        }
    }

    fn good_body(provider: ProviderId, count: usize, dimension: usize) -> String {
        match provider {
            ProviderId::Gemini => positional_body(count, dimension),
            _ => indexed_body(count, dimension),
        }
    }

    // ------------------------------------------------------ the happy path

    #[test]
    fn every_provider_returns_one_vector_per_text_in_order() {
        for provider in ProviderId::ALL {
            let batch = texts(3);
            let mock = MockProvider::always(200, &good_body(provider, 3, 4));
            let vectors = run(provider, &mock, &batch, Some(4)).expect("embed");
            assert_eq!(vectors.len(), 3, "{provider}");
            for vector in &vectors {
                assert_eq!(vector.len(), 4, "{provider}");
                assert!(vector.iter().all(|value| value.is_finite()), "{provider}");
            }
            assert_eq!(mock.hits(), 1, "{provider} made more than one call");
        }
    }

    #[test]
    fn document_and_query_are_prepared_differently_where_the_vendor_says_so() {
        // Voyage's `input_type` and Gemini's `taskType`. OpenAI documents
        // neither, and this asserts that difference rather than hiding it.
        for provider in ProviderId::ALL {
            let batch = texts(1);
            let mock = MockProvider::always(200, &good_body(provider, 1, 2));
            test_override::set(provider, mock.base_url());
            for role in [Role::Document, Role::Query] {
                provider::embed(
                    &HttpClient::new(),
                    provider,
                    &Credential::new(SENTINEL),
                    &EmbedJob {
                        model: model_for(provider),
                        role,
                        dimension: Some(2),
                        texts: &batch,
                    },
                    &NeverCancelled,
                    &RealDelay,
                )
                .expect("embed");
            }
            let sent = mock.bodies();
            assert_eq!(sent.len(), 2);
            match provider {
                ProviderId::OpenAi => assert_eq!(
                    sent[0], sent[1],
                    "OpenAI documents no role parameter; the two must be identical"
                ),
                _ => assert_ne!(
                    sent[0], sent[1],
                    "{provider} distinguishes document from query and the bodies must differ"
                ),
            }
        }
    }

    // ---------------------------------------------------------- the statuses

    #[test]
    fn every_documented_status_becomes_one_of_our_words() {
        let cases: [(u16, WireError); 7] = [
            (400, WireError::InvalidResponse),
            (401, WireError::Authentication),
            (403, WireError::Authentication),
            (404, WireError::ModelUnavailable),
            (408, WireError::Timeout),
            (429, WireError::RateLimited),
            (500, WireError::RateLimited),
        ];
        for (status, expected) in cases {
            for provider in ProviderId::ALL {
                let mock = MockProvider::always(
                    status,
                    r#"{"error":{"message":"o fornecedor escreveu isto","request_id":"req_abc123"}}"#,
                );
                let error = run(provider, &mock, &texts(1), Some(2)).unwrap_err();
                assert_eq!(error, expected, "{provider} on {status}");
            }
        }
    }

    #[test]
    fn a_502_and_a_503_are_retried_and_then_reported() {
        for status in [502u16, 503] {
            let mock = MockProvider::always(status, "{}");
            let error = run(ProviderId::OpenAi, &mock, &texts(1), Some(2)).unwrap_err();
            assert_eq!(error, WireError::RateLimited);
            assert_eq!(
                mock.hits(),
                noteit_embed::http::MAX_ATTEMPTS as usize,
                "{status} should be retried up to the ceiling and no further"
            );
        }
    }

    #[test]
    fn a_401_is_never_retried() {
        let mock = MockProvider::always(401, "{}");
        assert_eq!(
            run(ProviderId::OpenAi, &mock, &texts(1), Some(2)).unwrap_err(),
            WireError::Authentication
        );
        assert_eq!(
            mock.hits(),
            1,
            "a wrong key must be reported once, not three times"
        );
    }

    #[test]
    fn a_429_that_clears_is_retried_and_then_succeeds() {
        let mock = MockProvider::start(vec![
            Reply::WithHeader(429, "{}".into(), "retry-after", "1".into()),
            Reply::Status(200, indexed_body(1, 2)),
        ]);
        test_override::set(ProviderId::OpenAi, mock.base_url());
        let delay = RecordingDelay::default();
        let vectors = provider::embed(
            &HttpClient::new(),
            ProviderId::OpenAi,
            &Credential::new(SENTINEL),
            &EmbedJob {
                model: "text-embedding-3-small",
                role: Role::Document,
                dimension: Some(2),
                texts: &texts(1),
            },
            &NeverCancelled,
            &delay,
        )
        .expect("the second attempt succeeds");
        assert_eq!(vectors.len(), 1);
        assert_eq!(mock.hits(), 2);
        // The suite did not sleep: the policy computed a wait and the recording
        // clock kept it (§27).
        let waits = delay.waits();
        assert_eq!(waits.len(), 1, "one wait between two attempts");
        assert!(
            waits[0] >= Duration::from_secs(1),
            "Retry-After: 1 was not respected: {:?}",
            waits[0]
        );
    }

    #[test]
    fn the_retry_ceiling_is_a_ceiling_and_the_backoff_is_bounded() {
        let mock = MockProvider::always(429, "{}");
        test_override::set(ProviderId::Voyage, mock.base_url());
        let delay = RecordingDelay::default();
        let error = provider::embed(
            &HttpClient::new(),
            ProviderId::Voyage,
            &Credential::new(SENTINEL),
            &EmbedJob {
                model: "voyage-4",
                role: Role::Document,
                dimension: Some(2),
                texts: &texts(1),
            },
            &NeverCancelled,
            &delay,
        )
        .unwrap_err();
        assert_eq!(error, WireError::RateLimited);
        assert_eq!(mock.hits(), noteit_embed::http::MAX_ATTEMPTS as usize);
        assert_eq!(
            delay.waits().len(),
            (noteit_embed::http::MAX_ATTEMPTS - 1) as usize
        );
        assert!(
            delay.total() <= noteit_embed::http::OPERATION_DEADLINE,
            "the retries would outlive the deadline: {:?}",
            delay.total()
        );
    }

    #[test]
    fn an_absurd_retry_after_is_capped_rather_than_obeyed() {
        let mock = MockProvider::start(vec![
            Reply::WithHeader(429, "{}".into(), "retry-after", "3600".into()),
            Reply::Status(200, indexed_body(1, 2)),
        ]);
        test_override::set(ProviderId::OpenAi, mock.base_url());
        let delay = RecordingDelay::default();
        provider::embed(
            &HttpClient::new(),
            ProviderId::OpenAi,
            &Credential::new(SENTINEL),
            &EmbedJob {
                model: "text-embedding-3-small",
                role: Role::Document,
                dimension: Some(2),
                texts: &texts(1),
            },
            &NeverCancelled,
            &delay,
        )
        .expect("embed");
        let waited = delay.waits()[0];
        assert!(
            waited <= noteit_embed::http::MAX_BACKOFF + Duration::from_millis(250),
            "an hour was obeyed: {waited:?}"
        );
    }

    // --------------------------------------------------------- the bodies

    #[test]
    fn a_body_that_is_not_json_is_refused() {
        for provider in ProviderId::ALL {
            let mock = MockProvider::always(200, "<html><body>502 Bad Gateway</body></html>");
            assert_eq!(
                run(provider, &mock, &texts(1), Some(2)).unwrap_err(),
                WireError::InvalidResponse,
                "{provider}"
            );
        }
    }

    #[test]
    fn truncated_json_is_refused() {
        for provider in ProviderId::ALL {
            let mock = MockProvider::always(200, r#"{"data":[{"index":0,"embedding":[0.1,"#);
            assert_eq!(
                run(provider, &mock, &texts(1), Some(2)).unwrap_err(),
                WireError::InvalidResponse,
                "{provider}"
            );
        }
    }

    #[test]
    fn valid_json_with_the_wrong_schema_is_refused() {
        for provider in ProviderId::ALL {
            let mock = MockProvider::always(200, r#"{"result":"tudo certo","count":3}"#);
            assert_eq!(
                run(provider, &mock, &texts(1), Some(2)).unwrap_err(),
                WireError::InvalidResponse,
                "{provider}"
            );
        }
    }

    #[test]
    fn a_missing_vector_is_refused() {
        let mock = MockProvider::always(
            200,
            r#"{"data":[{"object":"embedding","index":0}],"model":"m"}"#,
        );
        assert_eq!(
            run(ProviderId::OpenAi, &mock, &texts(1), Some(2)).unwrap_err(),
            WireError::InvalidResponse
        );
    }

    #[test]
    fn a_vector_of_the_wrong_dimension_is_refused() {
        for provider in ProviderId::ALL {
            let mock = MockProvider::always(200, &good_body(provider, 1, 5));
            assert_eq!(
                run(provider, &mock, &texts(1), Some(4)).unwrap_err(),
                WireError::DimensionMismatch,
                "{provider}"
            );
        }
    }

    #[test]
    fn nan_and_the_infinities_are_refused() {
        // JSON has no literal for these, so a vendor sends them the ways a
        // vendor can: as strings, as `null`, or as a number large enough to
        // parse as an infinity.
        for body in [
            r#"{"data":[{"index":0,"embedding":[1.0,1e400]}]}"#,
            r#"{"data":[{"index":0,"embedding":[1.0,-1e400]}]}"#,
            r#"{"data":[{"index":0,"embedding":[1.0,null]}]}"#,
            r#"{"data":[{"index":0,"embedding":[1.0,"NaN"]}]}"#,
        ] {
            let mock = MockProvider::always(200, body);
            let error = run(ProviderId::OpenAi, &mock, &texts(1), Some(2)).unwrap_err();
            assert_eq!(error, WireError::InvalidResponse, "{body}");
        }
    }

    #[test]
    fn fewer_vectors_than_inputs_is_refused() {
        for provider in ProviderId::ALL {
            let mock = MockProvider::always(200, &good_body(provider, 2, 4));
            assert_eq!(
                run(provider, &mock, &texts(3), Some(4)).unwrap_err(),
                WireError::InvalidResponse,
                "{provider}"
            );
        }
    }

    #[test]
    fn more_vectors_than_inputs_is_refused() {
        for provider in ProviderId::ALL {
            let mock = MockProvider::always(200, &good_body(provider, 4, 4));
            assert_eq!(
                run(provider, &mock, &texts(3), Some(4)).unwrap_err(),
                WireError::InvalidResponse,
                "{provider}"
            );
        }
    }

    #[test]
    fn a_duplicated_or_out_of_range_index_rejects_the_whole_response() {
        for body in [
            r#"{"data":[{"index":0,"embedding":[1.0,0.0]},{"index":0,"embedding":[0.0,1.0]}]}"#,
            r#"{"data":[{"index":0,"embedding":[1.0,0.0]},{"index":7,"embedding":[0.0,1.0]}]}"#,
            r#"{"data":[{"index":1,"embedding":[1.0,0.0]},{"index":2,"embedding":[0.0,1.0]}]}"#,
        ] {
            let mock = MockProvider::always(200, body);
            assert_eq!(
                run(ProviderId::OpenAi, &mock, &texts(2), Some(2)).unwrap_err(),
                WireError::InvalidResponse,
                "{body}"
            );
        }
    }

    #[test]
    fn a_shuffled_response_is_put_back_in_order_rather_than_refused() {
        // The legitimate case the checks above must not break: the vendor may
        // answer out of order, and `index` is what makes that safe.
        let body = r#"{"data":[
            {"index":2,"embedding":[2.0,2.0]},
            {"index":0,"embedding":[0.0,0.0]},
            {"index":1,"embedding":[1.0,1.0]}]}"#;
        let mock = MockProvider::always(200, body);
        let vectors = run(ProviderId::OpenAi, &mock, &texts(3), Some(2)).expect("embed");
        assert_eq!(
            vectors,
            vec![vec![0.0, 0.0], vec![1.0, 1.0], vec![2.0, 2.0]]
        );
    }

    // ------------------------------------------------------- the transport

    #[test]
    fn a_connection_that_is_refused_is_unavailable() {
        // A port nothing is listening on: the shape of a provider that is
        // down, and of a machine with no route (§26, "DNS indisponível").
        let dead = {
            let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = probe.local_addr().expect("addr").port();
            drop(probe);
            port
        };
        test_override::set(ProviderId::OpenAi, format!("http://127.0.0.1:{dead}"));
        let delay = RecordingDelay::default();
        let error = provider::embed(
            &HttpClient::new(),
            ProviderId::OpenAi,
            &Credential::new(SENTINEL),
            &EmbedJob {
                model: "text-embedding-3-small",
                role: Role::Document,
                dimension: Some(2),
                texts: &texts(1),
            },
            &NeverCancelled,
            &delay,
        )
        .unwrap_err();
        assert_eq!(error, WireError::Unavailable);
    }

    #[test]
    fn a_connection_closed_without_an_answer_is_not_a_success() {
        let hangup = MockProvider::start(vec![Reply::Hangup]);
        let error = run(ProviderId::OpenAi, &hangup, &texts(1), Some(2)).unwrap_err();
        assert!(
            matches!(error, WireError::Unavailable | WireError::InvalidResponse),
            "a hangup produced {error:?}"
        );
    }

    #[test]
    fn a_body_cut_short_is_not_a_smaller_answer() {
        let mock = MockProvider::start(vec![Reply::Truncated(200, indexed_body(1, 4))]);
        let error = run(ProviderId::OpenAi, &mock, &texts(1), Some(4)).unwrap_err();
        assert!(
            matches!(error, WireError::InvalidResponse | WireError::Unavailable),
            "a truncated body produced {error:?}"
        );
    }

    #[test]
    fn a_body_larger_than_the_ceiling_is_refused_rather_than_read() {
        let mock = MockProvider::start(vec![Reply::Enormous(MAX_BODY_BYTES as usize + 1024)]);
        let error = run(ProviderId::OpenAi, &mock, &texts(1), Some(2)).unwrap_err();
        assert_eq!(error, WireError::InvalidResponse);
    }

    // ---------------------------------------------------------- redirects

    #[test]
    fn a_redirect_is_never_followed() {
        // The assertion that matters is not which error comes back: it is that
        // the destination the `Location` named received **nothing**. An
        // `Authorization` header and a note's text must not reach a host
        // nobody pinned (§64).
        let destination = MockProvider::always(200, &indexed_body(1, 2));
        for status in [301u16, 302, 303, 307, 308] {
            let source = MockProvider::start(vec![Reply::Redirect(
                status,
                format!("{}/v1/embeddings", destination.base_url()),
            )]);
            let outcome = run(ProviderId::OpenAi, &source, &texts(1), Some(2));
            assert!(outcome.is_err(), "{status} produced an answer");
            assert_eq!(
                destination.hits(),
                0,
                "{status} was followed and the credential left the pinned host"
            );
        }
    }

    // ------------------------------------------------------- cancellation

    #[test]
    fn a_cancelled_operation_stops_before_it_asks() {
        let mock = MockProvider::always(200, &indexed_body(1, 2));
        test_override::set(ProviderId::OpenAi, mock.base_url());
        let switch = Switch::default();
        switch.flip();
        let error = provider::embed(
            &HttpClient::new(),
            ProviderId::OpenAi,
            &Credential::new(SENTINEL),
            &EmbedJob {
                model: "text-embedding-3-small",
                role: Role::Document,
                dimension: Some(2),
                texts: &texts(1),
            },
            &switch,
            &RealDelay,
        )
        .unwrap_err();
        assert_eq!(error, WireError::Cancelled);
        assert_eq!(mock.hits(), 0, "a cancelled job still paid for a request");
    }

    // -------------------------------------------------------- the batches

    #[test]
    fn a_full_protocol_batch_is_one_provider_call() {
        // Every provider's own ceiling is above the protocol's, so the whole
        // batch fits in one request and the user pays for one.
        for provider in ProviderId::ALL {
            let batch = texts(noteit_embed_protocol::MAX_TEXTS);
            let mock = MockProvider::always(
                200,
                &good_body(provider, noteit_embed_protocol::MAX_TEXTS, 2),
            );
            let vectors = run(provider, &mock, &batch, Some(2)).expect("embed");
            assert_eq!(vectors.len(), noteit_embed_protocol::MAX_TEXTS);
            assert_eq!(mock.hits(), 1, "{provider} split a batch that fits");
        }
    }

    // --------------------------------------------------------- the privacy

    #[test]
    fn no_error_this_module_produces_carries_a_secret_or_a_note() {
        // `WireError` is a fieldless enum, so this is structurally true — and
        // asserted anyway, because the structural argument stops being true
        // the day somebody adds a payload to a variant "just for debugging".
        let batch = vec![format!("{PRIVATE_TEXT} — o corpo de uma nota")];
        for status in [400u16, 401, 403, 404, 429, 500] {
            let mock = MockProvider::always(
                status,
                &format!(
                    r#"{{"error":{{"message":"a chave {SENTINEL} falhou ao embutir {PRIVATE_TEXT}","request_id":"req_9"}}}}"#
                ),
            );
            let error = run(ProviderId::OpenAi, &mock, &batch, Some(2)).unwrap_err();
            let printed = format!("{error:?}");
            assert!(!printed.contains(SENTINEL), "{status}: {printed}");
            assert!(!printed.contains(PRIVATE_TEXT), "{status}: {printed}");
            assert!(!printed.contains("req_9"), "{status}: {printed}");
        }
    }

    #[test]
    fn the_request_carries_the_text_and_nothing_that_identifies_the_note() {
        // The text necessarily goes to the provider — that is what embedding
        // is. What must not go is anything else (§19).
        let batch = vec![PRIVATE_TEXT.to_string()];
        for provider in ProviderId::ALL {
            let mock = MockProvider::always(200, &good_body(provider, 1, 2));
            run(provider, &mock, &batch, Some(2)).expect("embed");
            let sent = mock.bodies();
            assert_eq!(sent.len(), 1);
            let body = &sent[0];
            assert!(
                body.contains(PRIVATE_TEXT),
                "{provider} sent no text at all"
            );
            for forbidden in [
                "note_id",
                "source_revision",
                "revision",
                "chunk_id",
                "path",
                "front_matter",
                "created_at",
                "updated_at",
                "store",
                "uuid",
            ] {
                assert!(
                    !body.contains(forbidden),
                    "{provider} sent `{forbidden}` in {body}"
                );
            }
            // And the key is in a header, never in the body somebody might log.
            assert!(
                !body.contains(SENTINEL),
                "{provider} put the key in the body"
            );
        }
    }
}
