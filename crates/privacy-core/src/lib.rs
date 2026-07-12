#![forbid(unsafe_code)]

mod detector;
mod types;

pub use detector::Detector;
pub use types::{Detection, EntityKind, PrivacyError};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
