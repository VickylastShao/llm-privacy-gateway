use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    routing::post,
};
use gateway::{
    ActivityEvent, ActivitySink, AuthProfile, GatewayConfig, GatewayError, GatewayProtocol,
    GatewayState, RuntimeProfile, authenticate, gateway_router, local_key_digest, router,
};
use http_body_util::BodyExt;
use privacy_core::{EntityKind, PolicyAction, StaticDictionary};
use secrecy::SecretString;
use session_map::MemoryBackend;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use uuid::Uuid;

type Captures = Arc<Mutex<Vec<(String, String)>>>;

#[test]
fn runtime_profile_uses_its_custom_dictionary_and_phone_detector() {
    let dictionary = StaticDictionary::new(["Project Aurora"])
        .unwrap_or_else(|error| panic!("dictionary: {error}"));
    let profile = RuntimeProfile::with_mapping_backend_and_dictionary(
        Uuid::nil(),
        GatewayProtocol::OpenAi,
        "local-key".to_owned(),
        "https://api.example.test/v1".to_owned(),
        SecretString::from("upstream-key"),
        [1; 32],
        MemoryBackend::default(),
        dictionary,
    );

    let custom = profile
        .engine
        .inspect("send Project Aurora", &profile.policy)
        .unwrap_or_else(|error| panic!("custom inspection: {error}"));
    assert_eq!(custom.decisions().len(), 1);
    assert!(matches!(
        custom.decisions()[0].detection().kind(),
        EntityKind::Custom(term) if term == "Project Aurora"
    ));
    assert_eq!(custom.decisions()[0].action(), PolicyAction::Pseudonymize);

    let phone = profile
        .engine
        .inspect("联系 13800138000", &profile.policy)
        .unwrap_or_else(|error| panic!("phone inspection: {error}"));
    assert_eq!(phone.decisions().len(), 1);
    assert_eq!(phone.decisions()[0].detection().kind(), &EntityKind::Phone);
}

#[derive(Default)]
struct CollectingSink {
    events: Mutex<Vec<ActivityEvent>>,
}

impl ActivitySink for CollectingSink {
    fn publish(&self, event: ActivityEvent) -> bool {
        self.events
            .lock()
            .unwrap_or_else(|_| panic!("activity lock"))
            .push(event);
        true
    }
}

struct RejectingSink;

impl ActivitySink for RejectingSink {
    fn publish(&self, _event: ActivityEvent) -> bool {
        false
    }
}

fn local_state() -> std::sync::Arc<GatewayState> {
    GatewayState::new([RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::OpenAi,
        "local-limit-key".to_owned(),
        "http://127.0.0.1:9".to_owned(),
        SecretString::from("upstream-key"),
        [1; 32],
    )])
    .unwrap_or_else(|error| panic!("gateway state: {error}"))
}

fn unused_loopback_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap_or_else(|error| panic!("temporary bind: {error}"));
    listener
        .local_addr()
        .unwrap_or_else(|error| panic!("temporary address: {error}"))
        .port()
}

#[test]
fn listener_refuses_non_loopback_addresses() {
    assert!(matches!(
        GatewayConfig::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 17891)),
        Err(GatewayError::NonLoopbackBind)
    ));
    assert!(GatewayConfig::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 17891)).is_ok());
}

#[tokio::test]
async fn gateway_can_stop_gracefully_after_a_local_shutdown_signal() {
    let config = GatewayConfig::new(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        unused_loopback_port(),
    ))
    .unwrap_or_else(|error| panic!("config: {error}"));
    let (shutdown, receiver) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(gateway::start_with_shutdown(
        config,
        local_state(),
        async move {
            let _ = receiver.await;
        },
    ));
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    shutdown
        .send(())
        .unwrap_or_else(|_| panic!("shutdown send"));
    assert!(
        task.await
            .unwrap_or_else(|error| panic!("gateway task: {error}"))
            .is_ok()
    );
}

#[tokio::test]
async fn listener_reports_an_occupied_loopback_port_before_serving() {
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("occupy port: {error}"));
    let address = occupied
        .local_addr()
        .unwrap_or_else(|error| panic!("occupied address: {error}"));
    let config = GatewayConfig::new(address).unwrap_or_else(|error| panic!("config: {error}"));

    assert!(matches!(
        gateway::bind_listener(config).await,
        Err(gateway::GatewayError::Listener)
    ));
}

#[test]
fn authentication_is_constant_time_digest_based_and_protocol_scoped() {
    let profile = AuthProfile {
        id: Uuid::nil(),
        protocol: GatewayProtocol::OpenAi,
        local_key_digest: local_key_digest("local-test-key"),
    };
    let mut openai = axum::http::HeaderMap::new();
    openai.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer local-test-key"),
    );
    assert_eq!(
        authenticate(
            &openai,
            GatewayProtocol::OpenAi,
            std::slice::from_ref(&profile),
        ),
        Ok(Uuid::nil())
    );
    assert!(matches!(
        authenticate(
            &openai,
            GatewayProtocol::Anthropic,
            std::slice::from_ref(&profile),
        ),
        Err(GatewayError::Unauthorized)
    ));
    let mut anthropic = axum::http::HeaderMap::new();
    anthropic.insert("x-api-key", HeaderValue::from_static("local-test-key"));
    assert!(matches!(
        authenticate(&anthropic, GatewayProtocol::Anthropic, &[profile]),
        Err(GatewayError::ProtocolMismatch)
    ));
}

#[tokio::test]
async fn shell_has_request_ids_and_no_cors_header() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .unwrap_or_else(|error| panic!("request must build: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("router must respond: {error}"));
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(response.headers().contains_key("x-request-id"));
    assert!(
        !response
            .headers()
            .contains_key("access-control-allow-origin")
    );
}

#[tokio::test]
async fn unknown_routes_and_oversized_requests_fail_closed() {
    let response = gateway_router(local_state())
        .oneshot(
            Request::builder()
                .uri("/v1/unsupported")
                .body(axum::body::Body::empty())
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway: {error}"));
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = gateway_router(local_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(header::AUTHORIZATION, "Bearer local-limit-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(vec![b'x'; 4 * 1024 * 1024 + 1]))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway: {error}"));
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn malformed_json_is_rejected_as_bad_request_before_forwarding() {
    let response = gateway_router(local_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(header::AUTHORIZATION, "Bearer local-limit-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from("{"))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway: {error}"));
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn blocked_sensitive_request_publishes_one_safe_activity() {
    const CANARY: &str = "sk-test-abcdefghijklmnopqrstuvwxyz";
    let sink = Arc::new(CollectingSink::default());
    let state = GatewayState::new_with_sink(
        [RuntimeProfile::new(
            Uuid::nil(),
            GatewayProtocol::OpenAi,
            "local-activity-key".to_owned(),
            "http://127.0.0.1:9/v1".to_owned(),
            SecretString::from("upstream-key"),
            [20; 32],
        )],
        sink.clone(),
    )
    .unwrap_or_else(|error| panic!("gateway state: {error}"));

    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(header::AUTHORIZATION, "Bearer local-activity-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(format!(
                    r#"{{"model":"deepseek-chat","messages":[{{"role":"user","content":"api_key={CANARY}"}}]}}"#
                )))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway: {error}"));

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let events = sink
        .events
        .lock()
        .unwrap_or_else(|_| panic!("activity lock"));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].status, 403);
    assert_eq!(events[0].model, "deepseek-chat");
    assert_eq!(events[0].report.category_counts().get("api_key"), Some(&1));
    assert!(!format!("{:?}", events[0]).contains(CANARY));
}

#[tokio::test]
async fn activity_replaces_sensitive_model_and_request_id_metadata() {
    const MODEL_CANARY: &str = "sk-proj-ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";
    const REQUEST_CANARY: &str = "sk-ant-api03-abcdefghijklmnopqrstuvwx123456";
    let sink = Arc::new(CollectingSink::default());
    let state = GatewayState::new_with_sink(
        [RuntimeProfile::new(
            Uuid::nil(),
            GatewayProtocol::OpenAi,
            "local-metadata-key".to_owned(),
            "http://127.0.0.1:9/v1".to_owned(),
            SecretString::from("upstream-key"),
            [23; 32],
        )],
        sink.clone(),
    )
    .unwrap_or_else(|error| panic!("gateway state: {error}"));

    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(header::AUTHORIZATION, "Bearer local-metadata-key")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-request-id", REQUEST_CANARY)
                .body(axum::body::Body::from(format!(
                    r#"{{"model":"{MODEL_CANARY}","messages":[{{"role":"user","content":"api_key=sk-test-abcdefghijklmnopqrstuvwxyz"}}]}}"#
                )))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway: {error}"));

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let events = sink
        .events
        .lock()
        .unwrap_or_else(|_| panic!("activity lock"));
    assert_eq!(events.len(), 1);
    assert!(events[0].model.is_empty());
    assert_ne!(events[0].request_id, REQUEST_CANARY);
    assert!(Uuid::parse_str(&events[0].request_id).is_ok());
    let debug = format!("{:?}", events[0]);
    assert!(!debug.contains(MODEL_CANARY));
    assert!(!debug.contains(REQUEST_CANARY));
}

#[tokio::test]
async fn sink_rejection_does_not_fail_transformed_requests() {
    let mock = axum::Router::new().route(
        "/v1/chat/completions",
        post(|| async { "{\"id\":\"mock\"}" }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let state = GatewayState::new_with_sink(
        [RuntimeProfile::new(
            Uuid::nil(),
            GatewayProtocol::OpenAi,
            "local-rejecting-sink-key".to_owned(),
            format!("http://{address}/v1"),
            SecretString::from("upstream-key"),
            [21; 32],
        )],
        Arc::new(RejectingSink),
    )
    .unwrap_or_else(|error| panic!("gateway state: {error}"));

    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(
                    header::AUTHORIZATION,
                    "Bearer local-rejecting-sink-key",
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    r#"{"model":"test","messages":[{"role":"user","content":"11010519491231002X"}]}"#,
                ))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway: {error}"));

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn transformed_canary_never_reaches_mock_upstream_in_plaintext() {
    let captured = Arc::new(Mutex::new(String::new()));
    let mock = axum::Router::new()
        .route(
            "/v1/chat/completions",
            post(
                |State(captured): State<Arc<Mutex<String>>>, body: String| async move {
                    *captured.lock().unwrap_or_else(|_| panic!("capture lock")) = body;
                    "{\"id\":\"mock\"}"
                },
            ),
        )
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let profile = RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::OpenAi,
        "local-test-key".to_owned(),
        format!("http://{address}/v1"),
        SecretString::from("upstream-key"),
        [4; 32],
    );
    let state =
        GatewayState::new([profile]).unwrap_or_else(|error| panic!("gateway state: {error}"));
    let body = "{\"model\":\"test\",\"messages\":[{\"role\":\"user\",\"content\":\"canary@example.test\"}]}";
    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(header::AUTHORIZATION, "Bearer local-test-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(body))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !captured
            .lock()
            .unwrap_or_else(|_| panic!("capture lock"))
            .contains("canary@example.test")
    );
}

#[tokio::test]
async fn openai_sse_response_restores_pseudonyms_before_returning_to_client() {
    let mock = axum::Router::new().route("/v1/chat/completions", post(|body: String| async move {
        let token = serde_json::from_str::<serde_json::Value>(&body).unwrap_or_else(|error| panic!("mock JSON: {error}"))["messages"][0]["content"].as_str().unwrap_or_else(|| panic!("content must be string")).to_owned();
        let split = token.len() / 2;
        ([("content-type", "text/event-stream")], format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\ndata: [DONE]\n\n", &token[..split], &token[split..]))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let profile = RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::OpenAi,
        "local-sse-key".to_owned(),
        format!("http://{address}/v1"),
        SecretString::from("upstream-key"),
        [9; 32],
    );
    let state =
        GatewayState::new([profile]).unwrap_or_else(|error| panic!("gateway state: {error}"));
    let response = gateway_router(state).oneshot(Request::builder().method("POST").uri("/v1/chat/completions").header(header::AUTHORIZATION, "Bearer local-sse-key").header(header::CONTENT_TYPE, "application/json").body(axum::body::Body::from("{\"model\":\"test\",\"stream\":true,\"messages\":[{\"role\":\"user\",\"content\":\"alice@example.test\"}]}" )).unwrap_or_else(|error| panic!("request: {error}"))).await.unwrap_or_else(|error| panic!("gateway response: {error}"));
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap_or_else(|error| panic!("body: {error}"))
        .to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap_or_else(|error| panic!("UTF-8: {error}"));
    assert!(text.contains("alice@example.test"));
    assert!(!text.contains("__LPG_"));
}

#[tokio::test]
async fn anthropic_sse_response_restores_pseudonyms_before_returning_to_client() {
    let mock = axum::Router::new().route("/v1/messages", post(|body: String| async move {
        let token = serde_json::from_str::<serde_json::Value>(&body)
            .unwrap_or_else(|error| panic!("mock JSON: {error}"))["messages"][0]["content"]
            .as_str()
            .unwrap_or_else(|| panic!("content must be string"))
            .to_owned();
        let split = token.len() / 2;
        (
            [("content-type", "text/event-stream")],
            format!(
                "event: content_block_delta\r\ndata: {{\"delta\":{{\"text\":\"{}\"}}}}\r\n\r\nevent: content_block_delta\r\ndata: {{\"delta\":{{\"text\":\"{}\"}}}}\r\n\r\n",
                &token[..split],
                &token[split..]
            ),
        )
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let profile = RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::Anthropic,
        "local-anthropic-sse-key".to_owned(),
        format!("http://{address}"),
        SecretString::from("upstream-key"),
        [10; 32],
    );
    let state =
        GatewayState::new([profile]).unwrap_or_else(|error| panic!("gateway state: {error}"));
    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("x-api-key", "local-anthropic-sse-key")
                .header("anthropic-version", "2023-06-01")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    "{\"model\":\"test\",\"stream\":true,\"max_tokens\":32,\"messages\":[{\"role\":\"user\",\"content\":\"alice@example.test\"}]}",
                ))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap_or_else(|error| panic!("body: {error}"))
        .to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap_or_else(|error| panic!("UTF-8: {error}"));
    assert!(text.contains("alice@example.test"));
    assert!(!text.contains("__LPG_"));
    assert!(text.contains("\r\n\r\n"));
}

#[tokio::test]
async fn anthropic_gateway_forwards_signed_thinking_blocks_as_raw_json() {
    let captured = Arc::new(Mutex::new(String::new()));
    let mock = axum::Router::new()
        .route(
            "/v1/messages",
            post(
                |State(captured): State<Arc<Mutex<String>>>, body: String| async move {
                    *captured.lock().unwrap_or_else(|_| panic!("capture lock")) = body;
                    "{\"id\":\"mock\"}"
                },
            ),
        )
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let profile = RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::Anthropic,
        "local-raw-key".to_owned(),
        format!("http://{address}"),
        SecretString::from("upstream-key"),
        [11; 32],
    );
    let state =
        GatewayState::new([profile]).unwrap_or_else(|error| panic!("gateway state: {error}"));
    let signed = r#"{ "type" : "thinking", "thinking" : "opaque", "signature" : "sig==" }"#;
    let request = format!(
        r#"{{"model":"test","messages":[{{"role":"assistant","content":[{signed},{{"type":"tool_result","tool_use_id":"u1","content":"canary@example.test"}}]}}]}}"#
    );
    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("x-api-key", "local-raw-key")
                .header("anthropic-version", "2023-06-01")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(request))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));
    assert_eq!(response.status(), StatusCode::OK);
    let captured = captured
        .lock()
        .unwrap_or_else(|_| panic!("capture lock"))
        .to_owned();
    assert!(captured.contains(signed));
    assert!(!captured.contains("canary@example.test"));
}

#[tokio::test]
async fn responses_and_count_tokens_do_not_forward_plaintext_canaries() {
    let captured: Captures = Arc::new(Mutex::new(Vec::new()));
    let mock = axum::Router::new()
        .route(
            "/v1/responses",
            post(
                |State(captured): State<Captures>, body: String| async move {
                    captured
                        .lock()
                        .unwrap_or_else(|_| panic!("capture lock"))
                        .push(("responses".to_owned(), body));
                    "{\"id\":\"resp_mock\"}"
                },
            ),
        )
        .route(
            "/v1/messages/count_tokens",
            post(
                |State(captured): State<Captures>, body: String| async move {
                    captured
                        .lock()
                        .unwrap_or_else(|_| panic!("capture lock"))
                        .push(("count_tokens".to_owned(), body));
                    "{\"input_tokens\":1}"
                },
            ),
        )
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let openai_id = Uuid::new_v4();
    let anthropic_id = Uuid::new_v4();
    let state = GatewayState::new([
        RuntimeProfile::new(
            openai_id,
            GatewayProtocol::OpenAi,
            "local-responses-key".to_owned(),
            format!("http://{address}/v1"),
            SecretString::from("upstream-key"),
            [12; 32],
        ),
        RuntimeProfile::new(
            anthropic_id,
            GatewayProtocol::Anthropic,
            "local-count-key".to_owned(),
            format!("http://{address}"),
            SecretString::from("upstream-key"),
            [13; 32],
        ),
    ])
    .unwrap_or_else(|error| panic!("gateway state: {error}"));

    let response = gateway_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/responses")
                .header(header::AUTHORIZATION, "Bearer local-responses-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    "{\"model\":\"test\",\"input\":\"responses-canary@example.test\"}",
                ))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));
    assert_eq!(response.status(), StatusCode::OK);

    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages/count_tokens")
                .header("x-api-key", "local-count-key")
                .header("anthropic-version", "2023-06-01")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    "{\"model\":\"test\",\"messages\":[{\"role\":\"user\",\"content\":\"count-canary@example.test\"}]}",
                ))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));
    assert_eq!(response.status(), StatusCode::OK);

    let captured = captured.lock().unwrap_or_else(|_| panic!("capture lock"));
    assert_eq!(captured.len(), 2);
    assert!(
        captured
            .iter()
            .all(|(_, body)| !body.contains("responses-canary@example.test")
                && !body.contains("count-canary@example.test"))
    );
}

#[tokio::test]
async fn openai_upstream_base_url_is_used_without_injecting_a_v1_prefix() {
    let mock = axum::Router::new().route(
        "/compat/chat/completions",
        post(|| async { "{\"id\":\"mock\"}" }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let profile = RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::OpenAi,
        "local-base-url-key".to_owned(),
        format!("http://{address}/compat"),
        SecretString::from("upstream-key"),
        [14; 32],
    );
    let state =
        GatewayState::new([profile]).unwrap_or_else(|error| panic!("gateway state: {error}"));
    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(header::AUTHORIZATION, "Bearer local-base-url-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    "{\"model\":\"test\",\"messages\":[{\"role\":\"user\",\"content\":\"hello\"}]}",
                ))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn openai_json_response_restores_pseudonyms_before_returning_to_client() {
    let mock = axum::Router::new().route(
        "/v1/chat/completions",
        post(|body: String| async move {
            let token = serde_json::from_str::<serde_json::Value>(&body)
                .unwrap_or_else(|error| panic!("mock JSON: {error}"))["messages"][0]["content"]
                .as_str()
                .unwrap_or_else(|| panic!("content must be string"))
                .to_owned();
            serde_json::json!({
                "choices": [{"message": {"content": token, "reasoning_content": "ok"}}]
            })
            .to_string()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let profile = RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::OpenAi,
        "local-json-key".to_owned(),
        format!("http://{address}/v1"),
        SecretString::from("upstream-key"),
        [16; 32],
    );
    let state =
        GatewayState::new([profile]).unwrap_or_else(|error| panic!("gateway state: {error}"));
    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(header::AUTHORIZATION, "Bearer local-json-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    "{\"model\":\"test\",\"messages\":[{\"role\":\"user\",\"content\":\"alice@example.test\"}]}",
                ))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap_or_else(|error| panic!("body: {error}"))
        .to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap_or_else(|error| panic!("UTF-8: {error}"));
    assert!(body.contains("alice@example.test"));
    assert!(!body.contains("__LPG_"));
}

#[tokio::test]
async fn models_route_authenticates_locally_and_uses_the_configured_openai_base_url() {
    let mock = axum::Router::new().route(
        "/compat/models",
        axum::routing::get(|| async { "{\"object\":\"list\",\"data\":[]}" }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let profile = RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::OpenAi,
        "local-models-key".to_owned(),
        format!("http://{address}/compat"),
        SecretString::from("upstream-key"),
        [19; 32],
    );
    let state =
        GatewayState::new([profile]).unwrap_or_else(|error| panic!("gateway state: {error}"));
    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header(header::AUTHORIZATION, "Bearer local-models-key")
                .body(axum::body::Body::empty())
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn gateway_forwards_only_safe_openai_request_and_response_headers() {
    let captured = Arc::new(Mutex::new(None::<HeaderMap>));
    let handler_capture = Arc::clone(&captured);
    let mock = axum::Router::new().route(
        "/v1/chat/completions",
        post(move |headers: HeaderMap| {
            let handler_capture = Arc::clone(&handler_capture);
            async move {
                *handler_capture
                    .lock()
                    .unwrap_or_else(|_| panic!("capture lock")) = Some(headers);
                let mut response = axum::response::Response::new(axum::body::Body::from(
                    r#"{"id":"mock","choices":[{"message":{"content":"ok"}}]}"#,
                ));
                response
                    .headers_mut()
                    .insert("content-type", HeaderValue::from_static("application/json"));
                response
                    .headers_mut()
                    .insert("retry-after", HeaderValue::from_static("7"));
                response.headers_mut().insert(
                    "x-ratelimit-remaining-requests",
                    HeaderValue::from_static("42"),
                );
                response
                    .headers_mut()
                    .insert("x-request-id", HeaderValue::from_static("upstream-request"));
                response
                    .headers_mut()
                    .insert("set-cookie", HeaderValue::from_static("session=secret"));
                response
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let state = GatewayState::new([RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::OpenAi,
        "local-header-key".to_owned(),
        format!("http://{address}/v1"),
        SecretString::from("upstream-header-key"),
        [24; 32],
    )])
    .unwrap_or_else(|error| panic!("gateway state: {error}"));

    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(header::AUTHORIZATION, "Bearer local-header-key")
                .header(header::CONTENT_TYPE, "application/json")
                .header("openai-project", "project-fixture")
                .header("idempotency-key", "idempotency-fixture")
                .header("cookie", "local-cookie=secret")
                .body(axum::body::Body::from(
                    r#"{"model":"test","messages":[{"role":"user","content":"hello"}]}"#,
                ))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));

    assert_eq!(
        response.headers().get("retry-after"),
        Some(&HeaderValue::from_static("7"))
    );
    assert_eq!(
        response.headers().get("x-ratelimit-remaining-requests"),
        Some(&HeaderValue::from_static("42"))
    );
    assert_eq!(
        response.headers().get("x-request-id"),
        Some(&HeaderValue::from_static("upstream-request"))
    );
    assert!(response.headers().get("set-cookie").is_none());
    let captured = captured.lock().unwrap_or_else(|_| panic!("capture lock"));
    let captured = captured
        .as_ref()
        .unwrap_or_else(|| panic!("headers missing"));
    assert_eq!(
        captured.get(header::AUTHORIZATION),
        Some(&HeaderValue::from_static("Bearer upstream-header-key"))
    );
    assert_eq!(
        captured.get("openai-project"),
        Some(&HeaderValue::from_static("project-fixture"))
    );
    assert_eq!(
        captured.get("idempotency-key"),
        Some(&HeaderValue::from_static("idempotency-fixture"))
    );
    assert!(captured.get("cookie").is_none());
}

#[tokio::test]
async fn gateway_preserves_anthropic_version_and_beta_headers() {
    let captured = Arc::new(Mutex::new(None::<HeaderMap>));
    let handler_capture = Arc::clone(&captured);
    let mock = axum::Router::new().route(
        "/v1/messages",
        post(move |headers: HeaderMap| {
            let handler_capture = Arc::clone(&handler_capture);
            async move {
                *handler_capture
                    .lock()
                    .unwrap_or_else(|_| panic!("capture lock")) = Some(headers);
                (
                    [(header::CONTENT_TYPE, "application/json")],
                    r#"{"id":"msg_mock","type":"message","role":"assistant","content":[{"type":"text","text":"ok"}],"model":"mock","stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#,
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("mock bind: {error}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("mock address: {error}"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock).await;
    });
    let state = GatewayState::new([RuntimeProfile::new(
        Uuid::nil(),
        GatewayProtocol::Anthropic,
        "local-anthropic-header-key".to_owned(),
        format!("http://{address}"),
        SecretString::from("upstream-anthropic-key"),
        [25; 32],
    )])
    .unwrap_or_else(|error| panic!("gateway state: {error}"));

    let response = gateway_router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("x-api-key", "local-anthropic-header-key")
                .header(header::CONTENT_TYPE, "application/json")
                .header("anthropic-version", "2024-01-01")
                .header("anthropic-beta", "tools-2024-04-04")
                .body(axum::body::Body::from(
                    r#"{"model":"test","max_tokens":8,"messages":[{"role":"user","content":"hello"}]}"#,
                ))
                .unwrap_or_else(|error| panic!("request: {error}")),
        )
        .await
        .unwrap_or_else(|error| panic!("gateway response: {error}"));

    assert_eq!(response.status(), StatusCode::OK);
    let captured = captured.lock().unwrap_or_else(|_| panic!("capture lock"));
    let captured = captured
        .as_ref()
        .unwrap_or_else(|| panic!("headers missing"));
    assert_eq!(
        captured.get("anthropic-version"),
        Some(&HeaderValue::from_static("2024-01-01"))
    );
    assert_eq!(
        captured.get("anthropic-beta"),
        Some(&HeaderValue::from_static("tools-2024-04-04"))
    );
    assert_eq!(
        captured.get("x-api-key"),
        Some(&HeaderValue::from_static("upstream-anthropic-key"))
    );
}
