use std::sync::Arc;

use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{StreamExt, stream};
use secrecy::ExposeSecret;
use tower::{ServiceBuilder, limit::ConcurrencyLimitLayer};
use tower_http::{
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
};

use crate::{GatewayConfig, GatewayError, GatewayProtocol, GatewayState, authenticate};

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

pub fn router() -> Router {
    apply_layers(Router::new().route("/healthz", get(|| async { StatusCode::NO_CONTENT })))
}
pub fn gateway_router(state: Arc<GatewayState>) -> Router {
    Router::new()
        .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
        .route("/v1/chat/completions", post(openai_chat))
        .route("/v1/responses", post(openai_responses))
        .route("/v1/messages", post(anthropic_messages))
        .route("/v1/messages/count_tokens", post(anthropic_count_tokens))
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(ConcurrencyLimitLayer::new(64))
                .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(PropagateRequestIdLayer::x_request_id()),
        )
}
fn apply_layers(router: Router) -> Router {
    router.layer(
        ServiceBuilder::new()
            .layer(ConcurrencyLimitLayer::new(64))
            .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
            .layer(PropagateRequestIdLayer::x_request_id()),
    )
}

pub async fn start(config: GatewayConfig) -> Result<(), GatewayError> {
    let listener = tokio::net::TcpListener::bind(config.bind())
        .await
        .map_err(|_| GatewayError::Listener)?;
    axum::serve(listener, router())
        .await
        .map_err(|_| GatewayError::Listener)
}
pub async fn start_with_state(
    config: GatewayConfig,
    state: Arc<GatewayState>,
) -> Result<(), GatewayError> {
    let listener = tokio::net::TcpListener::bind(config.bind())
        .await
        .map_err(|_| GatewayError::Listener)?;
    axum::serve(listener, gateway_router(state))
        .await
        .map_err(|_| GatewayError::Listener)
}

async fn openai_chat(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    forward(
        state,
        GatewayProtocol::OpenAi,
        "/v1/chat/completions",
        headers,
        body,
        |body, state, profile| {
            let mut value = serde_json::from_slice(body).map_err(|_| ForwardError::InvalidJson)?;
            protocol_openai::transform_chat_request(
                &mut value,
                &state.engine,
                &profile.policy,
                &mut *profile.mappings.lock().map_err(|_| ForwardError::Blocked)?,
            )
            .map_err(|_| ForwardError::Blocked)?;
            Ok(OutboundBody::Json(value))
        },
    )
    .await
}
async fn openai_responses(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    forward(
        state,
        GatewayProtocol::OpenAi,
        "/v1/responses",
        headers,
        body,
        |body, state, profile| {
            let mut value = serde_json::from_slice(body).map_err(|_| ForwardError::InvalidJson)?;
            protocol_openai::transform_responses_request(
                &mut value,
                &state.engine,
                &profile.policy,
                &mut *profile.mappings.lock().map_err(|_| ForwardError::Blocked)?,
            )
            .map_err(|_| ForwardError::Blocked)?;
            Ok(OutboundBody::Json(value))
        },
    )
    .await
}
async fn anthropic_messages(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    forward(
        state,
        GatewayProtocol::Anthropic,
        "/v1/messages",
        headers,
        body,
        |body, state, profile| {
            let input = std::str::from_utf8(body).map_err(|_| ForwardError::InvalidJson)?;
            let transformed = protocol_anthropic::transform_messages_request_raw(
                input,
                &state.engine,
                &profile.policy,
                &mut *profile.mappings.lock().map_err(|_| ForwardError::Blocked)?,
            )
            .map_err(|error| match error {
                protocol_anthropic::AnthropicProtocolError::Privacy => ForwardError::Blocked,
                protocol_anthropic::AnthropicProtocolError::Json => ForwardError::InvalidJson,
            })?;
            Ok(OutboundBody::Raw(transformed))
        },
    )
    .await
}
async fn anthropic_count_tokens(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    forward(
        state,
        GatewayProtocol::Anthropic,
        "/v1/messages/count_tokens",
        headers,
        body,
        |body, state, profile| {
            let input = std::str::from_utf8(body).map_err(|_| ForwardError::InvalidJson)?;
            let transformed = protocol_anthropic::transform_messages_request_raw(
                input,
                &state.engine,
                &profile.policy,
                &mut *profile.mappings.lock().map_err(|_| ForwardError::Blocked)?,
            )
            .map_err(|error| match error {
                protocol_anthropic::AnthropicProtocolError::Privacy => ForwardError::Blocked,
                protocol_anthropic::AnthropicProtocolError::Json => ForwardError::InvalidJson,
            })?;
            Ok(OutboundBody::Raw(transformed))
        },
    )
    .await
}

enum OutboundBody {
    Json(serde_json::Value),
    Raw(String),
}

enum ForwardError {
    InvalidJson,
    Blocked,
}

async fn forward<F>(
    state: Arc<GatewayState>,
    protocol: GatewayProtocol,
    path: &str,
    headers: HeaderMap,
    body: Bytes,
    transform: F,
) -> Response
where
    F: FnOnce(&[u8], &GatewayState, &crate::RuntimeProfile) -> Result<OutboundBody, ForwardError>,
{
    let auth_profiles = state.auth_profiles();
    let Ok(profile_id) = authenticate(&headers, protocol, &auth_profiles) else {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    let Some(profile) = state.profiles.get(&profile_id) else {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    let outbound = match transform(&body, &state, profile) {
        Ok(outbound) => outbound,
        Err(ForwardError::InvalidJson) => {
            return (StatusCode::BAD_REQUEST, "invalid JSON").into_response();
        }
        Err(ForwardError::Blocked) => {
            return (StatusCode::FORBIDDEN, "privacy policy blocked request").into_response();
        }
    };
    let url = format!(
        "{}{}",
        profile.upstream_base_url.trim_end_matches('/'),
        path
    );
    let request = state
        .client
        .post(url)
        .header("content-type", "application/json");
    let mut request = match outbound {
        OutboundBody::Json(value) => request.json(&value),
        OutboundBody::Raw(body) => request.body(body),
    };
    match protocol {
        GatewayProtocol::OpenAi => {
            request = request.bearer_auth(profile.upstream_key.expose_secret())
        }
        GatewayProtocol::Anthropic => {
            request = request
                .header("x-api-key", profile.upstream_key.expose_secret())
                .header(
                    "anthropic-version",
                    headers
                        .get("anthropic-version")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("2023-06-01"),
                );
        }
    }
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let content_type = response.headers().get("content-type").cloned();
            let is_sse = content_type
                .as_ref()
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("text/event-stream"));
            if is_sse && protocol == GatewayProtocol::OpenAi {
                let profile = Arc::clone(profile);
                let transformer = Arc::new(std::sync::Mutex::new(
                    protocol_openai::OpenAiSseTransformer::default(),
                ));
                let transforming = Arc::clone(&transformer);
                let transformed = response.bytes_stream().map(move |chunk| {
                    let bytes =
                        chunk.map_err(|_| std::io::Error::other("upstream response failed"))?;
                    let mut transformer = transforming
                        .lock()
                        .map_err(|_| std::io::Error::other("SSE transformer unavailable"))?;
                    let mapping = profile
                        .mappings
                        .lock()
                        .map_err(|_| std::io::Error::other("mapping unavailable"))?;
                    transformer
                        .push_bytes(&bytes, &mapping)
                        .map(Bytes::from)
                        .map_err(|_| std::io::Error::other("SSE restoration failed"))
                });
                let final_transformer = Arc::clone(&transformer);
                let trailing = stream::once(async move {
                    let mut transformer = final_transformer
                        .lock()
                        .map_err(|_| std::io::Error::other("SSE transformer unavailable"))?;
                    transformer
                        .finish()
                        .map(Bytes::from)
                        .map_err(|_| std::io::Error::other("SSE restoration failed"))
                });
                let mut result = Response::new(Body::from_stream(transformed.chain(trailing)));
                *result.status_mut() = status;
                if let Some(content_type) = content_type {
                    result.headers_mut().insert("content-type", content_type);
                }
                return result;
            }
            if is_sse && protocol == GatewayProtocol::Anthropic {
                let profile = Arc::clone(profile);
                let transformer = Arc::new(std::sync::Mutex::new(
                    protocol_anthropic::AnthropicSseTransformer::default(),
                ));
                let transforming = Arc::clone(&transformer);
                let transformed = response.bytes_stream().map(move |chunk| {
                    let bytes =
                        chunk.map_err(|_| std::io::Error::other("upstream response failed"))?;
                    let mut transformer = transforming
                        .lock()
                        .map_err(|_| std::io::Error::other("SSE transformer unavailable"))?;
                    let mapping = profile
                        .mappings
                        .lock()
                        .map_err(|_| std::io::Error::other("mapping unavailable"))?;
                    transformer
                        .push_bytes(&bytes, &mapping)
                        .map(Bytes::from)
                        .map_err(|_| std::io::Error::other("SSE restoration failed"))
                });
                let final_transformer = Arc::clone(&transformer);
                let trailing = stream::once(async move {
                    let mut transformer = final_transformer
                        .lock()
                        .map_err(|_| std::io::Error::other("SSE transformer unavailable"))?;
                    transformer
                        .finish()
                        .map(Bytes::from)
                        .map_err(|_| std::io::Error::other("SSE restoration failed"))
                });
                let mut result = Response::new(Body::from_stream(transformed.chain(trailing)));
                *result.status_mut() = status;
                if let Some(content_type) = content_type {
                    result.headers_mut().insert("content-type", content_type);
                }
                return result;
            }
            match response.bytes().await {
                Ok(mut bytes) => {
                    if is_sse && protocol == GatewayProtocol::Anthropic {
                        let Ok(text) = std::str::from_utf8(&bytes) else {
                            return (StatusCode::BAD_GATEWAY, "invalid upstream SSE")
                                .into_response();
                        };
                        let Ok(mapping) = profile.mappings.lock() else {
                            return (StatusCode::INTERNAL_SERVER_ERROR, "mapping unavailable")
                                .into_response();
                        };
                        let Ok(restored) =
                            protocol_anthropic::restore_anthropic_sse(text, &mapping)
                        else {
                            return (StatusCode::BAD_GATEWAY, "upstream SSE restoration failed")
                                .into_response();
                        };
                        bytes = Bytes::from(restored);
                    }
                    let mut result = Response::new(Body::from(bytes));
                    *result.status_mut() = status;
                    if let Some(content_type) = content_type {
                        result.headers_mut().insert("content-type", content_type);
                    }
                    result
                }
                Err(_) => (StatusCode::BAD_GATEWAY, "upstream response failed").into_response(),
            }
        }
        Err(_) => (StatusCode::BAD_GATEWAY, "upstream request failed").into_response(),
    }
}
