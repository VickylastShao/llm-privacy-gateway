#![forbid(unsafe_code)]

mod messages;

pub use messages::transform_messages_request;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
