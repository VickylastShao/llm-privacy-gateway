use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use tower::{ServiceBuilder, limit::ConcurrencyLimitLayer};
use tower_http::{
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
};

use crate::{GatewayConfig, GatewayError};

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

pub fn router() -> Router {
    Router::new()
        .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
        .route(
            "/echo",
            post(|request: Request<Body>| async move {
                (StatusCode::NO_CONTENT, request.into_response())
            }),
        )
        .layer(
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

trait IntoResponseRequest {
    fn into_response(self) -> axum::response::Response;
}
impl IntoResponseRequest for Request<Body> {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::NO_CONTENT).into_response()
    }
}
