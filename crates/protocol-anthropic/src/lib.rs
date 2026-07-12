#![forbid(unsafe_code)]

mod messages;
mod sse;

pub use messages::transform_messages_request;
pub use sse::restore_anthropic_sse;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
