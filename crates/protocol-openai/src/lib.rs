#![forbid(unsafe_code)]

mod chat;
mod error;
mod response;
mod responses;
mod sse;
mod stream_restore;

pub use chat::{transform_chat_request, transform_chat_request_with_report};
pub use error::OpenAiProtocolError;
pub use response::{restore_chat_response, restore_responses_response};
pub use responses::{transform_responses_request, transform_responses_request_with_report};
pub use sse::{SseDecoder, SseEvent};
pub use stream_restore::OpenAiSseTransformer;
pub use stream_restore::StreamRestorer;
pub use stream_restore::restore_openai_sse;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
