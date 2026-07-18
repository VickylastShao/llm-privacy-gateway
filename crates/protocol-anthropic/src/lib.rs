#![forbid(unsafe_code)]

mod messages;
mod sse;

pub use messages::{
    AnthropicProtocolError, restore_messages_response_raw, transform_messages_request,
    transform_messages_request_raw, transform_messages_request_raw_with_report,
    transform_messages_request_with_report,
};
pub use sse::AnthropicSseTransformer;
pub use sse::restore_anthropic_sse;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
