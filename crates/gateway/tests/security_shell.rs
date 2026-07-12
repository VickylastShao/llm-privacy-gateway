use axum::{
    extract::State,
    http::{HeaderValue, Request, StatusCode, header},
    routing::post,
};
use gateway::{
    AuthProfile, GatewayConfig, GatewayError, GatewayProtocol, GatewayState, RuntimeProfile,
    authenticate, gateway_router, local_key_digest, router,
};
use secrecy::SecretString;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use uuid::Uuid;

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
