use axum::http::{HeaderValue, Request, StatusCode, header};
use gateway::{
    AuthProfile, GatewayConfig, GatewayError, GatewayProtocol, authenticate, local_key_digest,
    router,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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
