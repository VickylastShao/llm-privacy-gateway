#![forbid(unsafe_code)]

mod activity;
mod auth;
mod config;
mod error;
mod runtime;
mod server;
mod upstream;

pub use activity::{ActivityEvent, ActivitySink, NoopActivitySink};
pub use auth::{AuthProfile, GatewayProtocol, authenticate, local_key_digest};
pub use config::GatewayConfig;
pub use error::GatewayError;
pub use runtime::{GatewayState, RuntimeProfile};
pub use server::gateway_router;
pub use server::{
    bind_listener, router, serve_with_listener_and_shutdown, start, start_with_shutdown,
    start_with_state,
};
pub use upstream::upstream_client;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
