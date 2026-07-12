#![forbid(unsafe_code)]

mod chat;
mod error;
mod responses;
mod sse;
mod stream_restore;

pub use chat::transform_chat_request;
pub use error::OpenAiProtocolError;
pub use responses::transform_responses_request;
pub use sse::{SseDecoder, SseEvent};
pub use stream_restore::StreamRestorer;
pub use stream_restore::restore_openai_sse;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
