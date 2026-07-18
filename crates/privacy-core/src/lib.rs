#![forbid(unsafe_code)]

mod detector;
mod detectors;
mod dictionary;
mod engine;
mod overlap;
mod policy;
mod report;
mod types;

pub use detector::Detector;
pub use detectors::{
    BankCardDetector, ChinaIdDetector, CredentialsDetector, NetworkDetector,
    UnifiedSocialCreditCodeDetector,
};
pub use dictionary::StaticDictionary;
pub use engine::{Decision, Inspection, PrivacyEngine, TransformResult};
pub use policy::{Policy, PolicyAction};
pub use report::{PrivacyPreview, RequestPrivacyReport};
pub use types::{Detection, EntityKind, PrivacyError};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
