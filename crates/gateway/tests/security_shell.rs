use axum::{
    extract::State,
    http::{HeaderValue, Request, StatusCode, header},
    routing::post,
};
use gateway::{
    AuthProfile, GatewayConfig, GatewayError, GatewayProtocol, GatewayState, RuntimeProfile,
    authenticate, gateway_router, local_key_digest, router,
};
use http_body_util::BodyExt;
use secrecy::SecretString;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use uuid::Uuid;

type Captures = Arc<Mutex<Vec<(String, String)>>>;

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
