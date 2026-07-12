#![forbid(unsafe_code)]

mod auth;
mod config;
mod error;
mod server;
mod upstream;

pub use auth::{AuthProfile, GatewayProtocol, authenticate, local_key_digest};
pub use config::GatewayConfig;
pub use error::GatewayError;
pub use server::{router, start};
pub use upstream::upstream_client;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
