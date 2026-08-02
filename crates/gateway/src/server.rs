use std::{
    future::Future,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, HeaderName, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::{StreamExt, stream};
use privacy_core::{Policy, PrivacyEngine, RequestPrivacyReport};
use secrecy::ExposeSecret;
use tower::{ServiceBuilder, limit::ConcurrencyLimitLayer};
use tower_http::{
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
};

use crate::{GatewayConfig, GatewayError, GatewayProtocol, GatewayState, authenticate};

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const MAX_ACTIVITY_METADATA_CHARS: usize = 128;

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
        .route("/v1/models", get(models))
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
    let listener = bind_listener(config).await?;
    axum::serve(listener, router())
        .await
        .map_err(|_| GatewayError::Listener)
}
pub async fn start_with_state(
    config: GatewayConfig,
    state: Arc<GatewayState>,
) -> Result<(), GatewayError> {
    let listener = bind_listener(config).await?;
    axum::serve(listener, gateway_router(state))
        .await
        .map_err(|_| GatewayError::Listener)
}

pub async fn start_with_shutdown<F>(
    config: GatewayConfig,
    state: Arc<GatewayState>,
    shutdown: F,
) -> Result<(), GatewayError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let listener = bind_listener(config).await?;
    serve_with_listener_and_shutdown(listener, state, shutdown).await
}

pub async fn bind_listener(config: GatewayConfig) -> Result<tokio::net::TcpListener, GatewayError> {
    tokio::net::TcpListener::bind(config.bind())
        .await
        .map_err(|_| GatewayError::Listener)
}

pub async fn serve_with_listener_and_shutdown<F>(
    listener: tokio::net::TcpListener,
    state: Arc<GatewayState>,
    shutdown: F,
) -> Result<(), GatewayError>
where
    F: Future<Output = ()> + Send + 'static,
{
    axum::serve(listener, gateway_router(state))
        .with_graceful_shutdown(shutdown)
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
        ResponseAdapter::OpenAiChat,
        "/chat/completions",
        headers,
        body,
        |body, _state, profile, report| {
            let mut value = serde_json::from_slice(body).map_err(|_| ForwardError::InvalidJson)?;
            protocol_openai::transform_chat_request_with_report(
                &mut value,
                &profile.engine,
                &profile.policy,
                &mut *profile.mappings.lock().map_err(|_| ForwardError::Blocked)?,
                report,
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
        ResponseAdapter::OpenAiResponses,
        "/responses",
        headers,
        body,
        |body, _state, profile, report| {
            let mut value = serde_json::from_slice(body).map_err(|_| ForwardError::InvalidJson)?;
            protocol_openai::transform_responses_request_with_report(
                &mut value,
                &profile.engine,
                &profile.policy,
                &mut *profile.mappings.lock().map_err(|_| ForwardError::Blocked)?,
                report,
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
        ResponseAdapter::AnthropicMessages,
        "/v1/messages",
        headers,
        body,
        |body, _state, profile, report| {
            let input = std::str::from_utf8(body).map_err(|_| ForwardError::InvalidJson)?;
            let transformed = protocol_anthropic::transform_messages_request_raw_with_report(
                input,
                &profile.engine,
                &profile.policy,
                &mut *profile.mappings.lock().map_err(|_| ForwardError::Blocked)?,
                report,
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
        ResponseAdapter::None,
        "/v1/messages/count_tokens",
        headers,
        body,
        |body, _state, profile, report| {
            let input = std::str::from_utf8(body).map_err(|_| ForwardError::InvalidJson)?;
            let transformed = protocol_anthropic::transform_messages_request_raw_with_report(
                input,
                &profile.engine,
                &profile.policy,
                &mut *profile.mappings.lock().map_err(|_| ForwardError::Blocked)?,
                report,
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

async fn models(State(state): State<Arc<GatewayState>>, headers: HeaderMap) -> Response {
    let auth_profiles = state.auth_profiles();
    let protocol = [GatewayProtocol::OpenAi, GatewayProtocol::Anthropic]
        .into_iter()
        .find_map(|protocol| {
            authenticate(&headers, protocol, &auth_profiles)
                .ok()
                .map(|id| (protocol, id))
        });
    let Some((protocol, profile_id)) = protocol else {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    let Some(profile) = state.profiles.get(&profile_id) else {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    let path = match protocol {
        GatewayProtocol::OpenAi => "/models",
        GatewayProtocol::Anthropic => "/v1/models",
    };
    let url = format!(
        "{}{}",
        profile.upstream_base_url.trim_end_matches('/'),
        path
    );
    let request = copy_safe_request_headers(state.client.get(url), &headers);
    let request = match protocol {
        GatewayProtocol::OpenAi => request.bearer_auth(profile.upstream_key.expose_secret()),
        GatewayProtocol::Anthropic => request
            .header("x-api-key", profile.upstream_key.expose_secret())
            .header(
                "anthropic-version",
                headers
                    .get("anthropic-version")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("2023-06-01"),
            ),
    };
    match request.send().await {
        Ok(response) => {
            let status = response.status();
            let response_headers = safe_response_headers(response.headers());
            match response.bytes().await {
                Ok(bytes) => {
                    let mut result = Response::new(Body::from(bytes));
                    *result.status_mut() = status;
                    apply_response_headers(&mut result, &response_headers);
                    result
                }
                Err(_) => (StatusCode::BAD_GATEWAY, "upstream response failed").into_response(),
            }
        }
        Err(_) => (StatusCode::BAD_GATEWAY, "upstream request failed").into_response(),
    }
}

enum OutboundBody {
    Json(serde_json::Value),
    Raw(String),
}

enum ForwardError {
    InvalidJson,
    Blocked,
}

#[derive(Clone, Copy)]
enum ResponseAdapter {
    None,
    OpenAiChat,
    OpenAiResponses,
    AnthropicMessages,
}

async fn forward<F>(
    state: Arc<GatewayState>,
    protocol: GatewayProtocol,
    response_adapter: ResponseAdapter,
    path: &str,
    headers: HeaderMap,
    body: Bytes,
    transform: F,
) -> Response
where
    F: FnOnce(
        &[u8],
        &GatewayState,
        &crate::RuntimeProfile,
        &mut RequestPrivacyReport,
    ) -> Result<OutboundBody, ForwardError>,
{
    let auth_profiles = state.auth_profiles();
    let Ok(profile_id) = authenticate(&headers, protocol, &auth_profiles) else {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    let Some(profile) = state.profiles.get(&profile_id) else {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    };
    let started = Instant::now();
    let occurred_at_ms = now_ms();
    let usage_id = uuid::Uuid::new_v4();
    let request_id = request_id(&headers, &profile.engine);
    let model = request_model(&body, &profile.engine);
    let mut report = RequestPrivacyReport::default();
    let outbound = match transform(&body, &state, profile, &mut report) {
        Ok(outbound) => outbound,
        Err(ForwardError::InvalidJson) => {
            return (StatusCode::BAD_REQUEST, "invalid JSON").into_response();
        }
        Err(ForwardError::Blocked) => {
            publish_activity(
                &state,
                profile_id,
                path,
                model.clone(),
                StatusCode::FORBIDDEN,
                started,
                request_id,
                report,
                occurred_at_ms,
            );
            return (StatusCode::FORBIDDEN, "privacy policy blocked request").into_response();
        }
    };
    let url = format!(
        "{}{}",
        profile.upstream_base_url.trim_end_matches('/'),
        path
    );
    let request = copy_safe_request_headers(
        state
            .client
            .post(url)
            .header("content-type", "application/json"),
        &headers,
    );
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
            publish_request_usage(
                &state,
                usage_id,
                occurred_at_ms,
                profile_id,
                model.clone(),
                None,
            );
            publish_activity(
                &state,
                profile_id,
                path,
                model.clone(),
                status,
                started,
                request_id,
                report,
                occurred_at_ms,
            );
            let response_headers = safe_response_headers(response.headers());
            let is_sse = response_headers
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("text/event-stream"));
            if is_sse && protocol == GatewayProtocol::OpenAi {
                let profile = Arc::clone(profile);
                let usage_sink = Arc::clone(&state.usage_sink);
                let stream_usage_id = usage_id;
                let usage_profile_id = profile_id;
                let usage_model = model.clone();
                let usage = Arc::new(std::sync::Mutex::new(UsageAccumulator::new(protocol)));
                let observing = Arc::clone(&usage);
                let transformer = Arc::new(std::sync::Mutex::new(
                    protocol_openai::OpenAiSseTransformer::default(),
                ));
                let transforming = Arc::clone(&transformer);
                let transformed = response.bytes_stream().map(move |chunk| {
                    let bytes =
                        chunk.map_err(|_| std::io::Error::other("upstream response failed"))?;
                    if let Ok(mut usage) = observing.lock() {
                        usage.push(&bytes);
                    }
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
                let final_usage = Arc::clone(&usage);
                let trailing = stream::once(async move {
                    let mut transformer = final_transformer
                        .lock()
                        .map_err(|_| std::io::Error::other("SSE transformer unavailable"))?;
                    let restored = transformer
                        .finish()
                        .map(Bytes::from)
                        .map_err(|_| std::io::Error::other("SSE restoration failed"))?;
                    if let Ok(mut usage) = final_usage.lock()
                        && let Some(usage) = usage.finish()
                    {
                        let _ = usage_sink.publish_usage(crate::UsageEvent {
                            id: stream_usage_id,
                            occurred_at_ms,
                            profile_id: usage_profile_id,
                            model: usage_model,
                            reported: true,
                            usage,
                        });
                    }
                    Ok::<Bytes, std::io::Error>(restored)
                });
                let mut result = Response::new(Body::from_stream(transformed.chain(trailing)));
                *result.status_mut() = status;
                apply_response_headers(&mut result, &response_headers);
                return result;
            }
            if is_sse && protocol == GatewayProtocol::Anthropic {
                let profile = Arc::clone(profile);
                let usage_sink = Arc::clone(&state.usage_sink);
                let stream_usage_id = usage_id;
                let usage_profile_id = profile_id;
                let usage_model = model.clone();
                let usage = Arc::new(std::sync::Mutex::new(UsageAccumulator::new(protocol)));
                let observing = Arc::clone(&usage);
                let transformer = Arc::new(std::sync::Mutex::new(
                    protocol_anthropic::AnthropicSseTransformer::default(),
                ));
                let transforming = Arc::clone(&transformer);
                let transformed = response.bytes_stream().map(move |chunk| {
                    let bytes =
                        chunk.map_err(|_| std::io::Error::other("upstream response failed"))?;
                    if let Ok(mut usage) = observing.lock() {
                        usage.push(&bytes);
                    }
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
                let final_usage = Arc::clone(&usage);
                let trailing = stream::once(async move {
                    let mut transformer = final_transformer
                        .lock()
                        .map_err(|_| std::io::Error::other("SSE transformer unavailable"))?;
                    let restored = transformer
                        .finish()
                        .map(Bytes::from)
                        .map_err(|_| std::io::Error::other("SSE restoration failed"))?;
                    if let Ok(mut usage) = final_usage.lock()
                        && let Some(usage) = usage.finish()
                    {
                        let _ = usage_sink.publish_usage(crate::UsageEvent {
                            id: stream_usage_id,
                            occurred_at_ms,
                            profile_id: usage_profile_id,
                            model: usage_model,
                            reported: true,
                            usage,
                        });
                    }
                    Ok::<Bytes, std::io::Error>(restored)
                });
                let mut result = Response::new(Body::from_stream(transformed.chain(trailing)));
                *result.status_mut() = status;
                apply_response_headers(&mut result, &response_headers);
                return result;
            }
            match response.bytes().await {
                Ok(mut bytes) => {
                    if let Some(usage) = usage_from_json(&bytes, protocol) {
                        let _ = state.usage_sink.publish_usage(crate::UsageEvent {
                            id: usage_id,
                            occurred_at_ms,
                            profile_id,
                            model: model.clone(),
                            reported: true,
                            usage,
                        });
                    }
                    if matches!(
                        response_adapter,
                        ResponseAdapter::OpenAiChat | ResponseAdapter::OpenAiResponses
                    ) {
                        let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&bytes)
                        else {
                            let mut result = Response::new(Body::from(bytes));
                            *result.status_mut() = status;
                            apply_response_headers(&mut result, &response_headers);
                            return result;
                        };
                        let Ok(mapping) = profile.mappings.lock() else {
                            return (StatusCode::INTERNAL_SERVER_ERROR, "mapping unavailable")
                                .into_response();
                        };
                        let restored = match response_adapter {
                            ResponseAdapter::OpenAiChat => {
                                protocol_openai::restore_chat_response(&mut value, &mapping)
                            }
                            ResponseAdapter::OpenAiResponses => {
                                protocol_openai::restore_responses_response(&mut value, &mapping)
                            }
                            ResponseAdapter::None | ResponseAdapter::AnthropicMessages => Ok(()),
                        };
                        if restored.is_err() {
                            return (
                                StatusCode::BAD_GATEWAY,
                                "upstream response restoration failed",
                            )
                                .into_response();
                        }
                        let Ok(serialized) = serde_json::to_vec(&value) else {
                            return (
                                StatusCode::BAD_GATEWAY,
                                "upstream response restoration failed",
                            )
                                .into_response();
                        };
                        bytes = Bytes::from(serialized);
                    }
                    if matches!(response_adapter, ResponseAdapter::AnthropicMessages) {
                        let Ok(body) = std::str::from_utf8(&bytes) else {
                            let mut result = Response::new(Body::from(bytes));
                            *result.status_mut() = status;
                            apply_response_headers(&mut result, &response_headers);
                            return result;
                        };
                        let Ok(mapping) = profile.mappings.lock() else {
                            return (StatusCode::INTERNAL_SERVER_ERROR, "mapping unavailable")
                                .into_response();
                        };
                        let Ok(restored) =
                            protocol_anthropic::restore_messages_response_raw(body, &mapping)
                        else {
                            return (
                                StatusCode::BAD_GATEWAY,
                                "upstream response restoration failed",
                            )
                                .into_response();
                        };
                        bytes = Bytes::from(restored);
                    }
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
                    apply_response_headers(&mut result, &response_headers);
                    result
                }
                Err(_) => (StatusCode::BAD_GATEWAY, "upstream response failed").into_response(),
            }
        }
        Err(_) => {
            publish_request_usage(
                &state,
                usage_id,
                occurred_at_ms,
                profile_id,
                model.clone(),
                None,
            );
            publish_activity(
                &state,
                profile_id,
                path,
                model.clone(),
                StatusCode::BAD_GATEWAY,
                started,
                request_id,
                report,
                occurred_at_ms,
            );
            (StatusCode::BAD_GATEWAY, "upstream request failed").into_response()
        }
    }
}

fn publish_request_usage(
    state: &GatewayState,
    id: uuid::Uuid,
    occurred_at_ms: i64,
    profile_id: uuid::Uuid,
    model: String,
    usage: Option<crate::TokenUsage>,
) {
    let _ = state.usage_sink.publish_usage(crate::UsageEvent {
        id,
        occurred_at_ms,
        profile_id,
        model,
        reported: usage.is_some(),
        usage: usage.unwrap_or_default(),
    });
}

const MAX_USAGE_SSE_EVENT_BYTES: usize = 256 * 1024;

struct UsageAccumulator {
    protocol: GatewayProtocol,
    pending: Vec<u8>,
    usage: crate::TokenUsage,
}

impl UsageAccumulator {
    fn new(protocol: GatewayProtocol) -> Self {
        Self {
            protocol,
            pending: Vec::new(),
            usage: crate::TokenUsage::default(),
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        if self.pending.len().saturating_add(bytes.len()) > MAX_USAGE_SSE_EVENT_BYTES {
            self.pending.clear();
            return;
        }
        self.pending.extend_from_slice(bytes);
        while let Some(end) = sse_event_end(&self.pending) {
            let event = self.pending.drain(..end).collect::<Vec<_>>();
            self.observe_event(&event);
        }
    }

    fn observe_event(&mut self, event: &[u8]) {
        let Ok(text) = std::str::from_utf8(event) else {
            return;
        };
        let data = text
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        if let Some(usage) = usage_from_json(data.as_bytes(), self.protocol) {
            self.usage.input_tokens = self.usage.input_tokens.max(usage.input_tokens);
            self.usage.output_tokens = self.usage.output_tokens.max(usage.output_tokens);
            self.usage.total_tokens = self.usage.total_tokens.max(usage.total_tokens);
            self.usage.cached_tokens = self.usage.cached_tokens.max(usage.cached_tokens);
        }
    }

    fn finish(&mut self) -> Option<crate::TokenUsage> {
        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            self.observe_event(&pending);
        }
        if self.protocol == GatewayProtocol::Anthropic {
            self.usage.total_tokens = self
                .usage
                .input_tokens
                .saturating_add(self.usage.output_tokens)
                .saturating_add(self.usage.cached_tokens);
        } else if self.usage.total_tokens == 0 {
            self.usage.total_tokens = self
                .usage
                .input_tokens
                .saturating_add(self.usage.output_tokens);
        }
        (!self.usage.is_empty()).then_some(self.usage)
    }
}

fn sse_event_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| index + 2)
        .or_else(|| {
            bytes
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
        })
}

fn usage_from_json(bytes: &[u8], protocol: GatewayProtocol) -> Option<crate::TokenUsage> {
    let value = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    let usage = value.get("usage").or_else(|| {
        value
            .get("response")
            .and_then(|response| response.get("usage"))
    })?;
    let input_tokens = json_u64(usage, "prompt_tokens")
        .or_else(|| json_u64(usage, "input_tokens"))
        .unwrap_or(0);
    let output_tokens = json_u64(usage, "completion_tokens")
        .or_else(|| json_u64(usage, "output_tokens"))
        .unwrap_or(0);
    let cached_tokens = usage
        .get("prompt_tokens_details")
        .and_then(|details| json_u64(details, "cached_tokens"))
        .or_else(|| {
            usage
                .get("input_tokens_details")
                .and_then(|details| json_u64(details, "cached_tokens"))
        })
        .or_else(|| json_u64(usage, "cached_tokens"))
        .or_else(|| json_u64(usage, "cache_read_input_tokens"))
        .unwrap_or(0);
    let cache_creation = json_u64(usage, "cache_creation_input_tokens").unwrap_or(0);
    let total_tokens = json_u64(usage, "total_tokens").unwrap_or_else(|| {
        input_tokens.saturating_add(output_tokens).saturating_add(
            if protocol == GatewayProtocol::Anthropic {
                cached_tokens.saturating_add(cache_creation)
            } else {
                0
            },
        )
    });
    let result = crate::TokenUsage {
        input_tokens,
        output_tokens,
        total_tokens,
        cached_tokens: cached_tokens.saturating_add(cache_creation),
    };
    (!result.is_empty()).then_some(result)
}

fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(serde_json::Value::as_u64)
}

fn copy_safe_request_headers(
    mut request: reqwest::RequestBuilder,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    for name in [
        "accept",
        "anthropic-beta",
        "idempotency-key",
        "openai-organization",
        "openai-project",
    ] {
        if let Some(value) = headers.get(name) {
            request = request.header(name, value);
        }
    }
    request
}

fn safe_response_headers(headers: &HeaderMap) -> HeaderMap {
    let mut safe = HeaderMap::new();
    for (name, value) in headers {
        if safe_response_header(name) {
            safe.append(name.clone(), value.clone());
        }
    }
    safe
}

fn safe_response_header(name: &HeaderName) -> bool {
    let name = name.as_str();
    matches!(
        name,
        "content-type" | "retry-after" | "request-id" | "x-request-id" | "openai-processing-ms"
    ) || name.starts_with("x-ratelimit-")
        || name.starts_with("ratelimit-")
}

fn apply_response_headers(response: &mut Response, headers: &HeaderMap) {
    for (name, value) in headers {
        response.headers_mut().append(name.clone(), value.clone());
    }
}

#[allow(clippy::too_many_arguments)]
fn publish_activity(
    state: &GatewayState,
    profile_id: uuid::Uuid,
    endpoint: &str,
    model: String,
    status: StatusCode,
    started: Instant,
    request_id: String,
    report: RequestPrivacyReport,
    occurred_at_ms: i64,
) {
    if report.is_empty() {
        return;
    }
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let _ = state.activity_sink.publish(crate::ActivityEvent {
        id: uuid::Uuid::new_v4(),
        occurred_at_ms,
        profile_id,
        endpoint: endpoint.to_owned(),
        model,
        status: status.as_u16(),
        latency_ms,
        request_id,
        report,
    });
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn request_id(headers: &HeaderMap, engine: &PrivacyEngine) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| safe_activity_metadata(value, engine, request_id_character))
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

fn request_model(body: &[u8], engine: &PrivacyEngine) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("model")
                .and_then(serde_json::Value::as_str)
                .filter(|value| safe_activity_metadata(value, engine, model_character))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default()
}

fn safe_activity_metadata(
    value: &str,
    engine: &PrivacyEngine,
    allowed_character: fn(char) -> bool,
) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_ACTIVITY_METADATA_CHARS
        && value.chars().all(allowed_character)
        && engine
            .inspect(value, &Policy::monitor())
            .is_ok_and(|inspection| inspection.decisions().is_empty())
}

fn request_id_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
}

fn model_character(character: char) -> bool {
    request_id_character(character) || matches!(character, '/' | '@' | '+')
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    #[test]
    fn reads_chat_completions_usage_and_cached_tokens() {
        let body = br#"{"usage":{"prompt_tokens":120,"completion_tokens":30,"total_tokens":150,"prompt_tokens_details":{"cached_tokens":80}}}"#;
        let usage = match usage_from_json(body, GatewayProtocol::OpenAi) {
            Some(usage) => usage,
            None => panic!("usage missing"),
        };
        assert_eq!(usage.input_tokens, 120);
        assert_eq!(usage.output_tokens, 30);
        assert_eq!(usage.total_tokens, 150);
        assert_eq!(usage.cached_tokens, 80);
    }

    #[test]
    fn reads_nested_responses_api_usage() {
        let body = br#"{"type":"response.completed","response":{"usage":{"input_tokens":42,"output_tokens":8,"total_tokens":50,"input_tokens_details":{"cached_tokens":12}}}}"#;
        let usage = match usage_from_json(body, GatewayProtocol::OpenAi) {
            Some(usage) => usage,
            None => panic!("usage missing"),
        };
        assert_eq!(usage.total_tokens, 50);
        assert_eq!(usage.cached_tokens, 12);
    }

    #[test]
    fn combines_anthropic_usage_across_split_sse_chunks() {
        let mut accumulator = UsageAccumulator::new(GatewayProtocol::Anthropic);
        accumulator.push(
            b"event: message_start\ndata: {\"usage\":{\"input_tokens\":10,\"cache_read_input_tokens\":4}}\n\n",
        );
        accumulator.push(b"event: message_delta\ndata: {\"usage\":{\"out");
        accumulator.push(b"put_tokens\":7}}\n\n");
        let usage = match accumulator.finish() {
            Some(usage) => usage,
            None => panic!("usage missing"),
        };
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 7);
        assert_eq!(usage.cached_tokens, 4);
        assert_eq!(usage.total_tokens, 21);
    }
}
