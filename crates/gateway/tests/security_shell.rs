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

#[test]
fn listener_refuses_non_loopback_addresses() {
    assert!(matches!(
        GatewayConfig::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 17891)),
        Err(GatewayError::NonLoopbackBind)
    ));
    assert!(GatewayConfig::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 17891)).is_ok());
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
        format!("http://{address}"),
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
        format!("http://{address}"),
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
