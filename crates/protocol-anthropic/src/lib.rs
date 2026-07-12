#![forbid(unsafe_code)]

mod messages;
mod sse;

pub use messages::{
    AnthropicProtocolError, transform_messages_request, transform_messages_request_raw,
};
pub use sse::AnthropicSseTransformer;
pub use sse::restore_anthropic_sse;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
