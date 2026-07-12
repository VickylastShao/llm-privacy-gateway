#![forbid(unsafe_code)]

mod detector;
mod detectors;
mod types;

pub use detector::Detector;
pub use detectors::{
    BankCardDetector, ChinaIdDetector, CredentialsDetector, NetworkDetector,
    UnifiedSocialCreditCodeDetector,
};
pub use types::{Detection, EntityKind, PrivacyError};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
