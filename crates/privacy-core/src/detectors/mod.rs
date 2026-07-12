mod bank_card;
mod china_id;
mod credentials;
mod network;
mod uscc;

pub use bank_card::BankCardDetector;
pub use china_id::ChinaIdDetector;
pub use credentials::CredentialsDetector;
pub use network::NetworkDetector;
pub use uscc::UnifiedSocialCreditCodeDetector;

use crate::{Detection, EntityKind, PrivacyError};

fn add_detection(
    detections: &mut Vec<Detection>,
    kind: EntityKind,
    start: usize,
    end: usize,
    confidence: u8,
    source: &'static str,
) -> Result<(), PrivacyError> {
    detections.push(Detection::new(kind, start, end, confidence, source)?);
    Ok(())
}

fn sort_and_dedup(detections: &mut Vec<Detection>) {
    detections.sort_by_key(|detection| detection.start());
    let mut end = 0;
    detections.retain(|detection| {
        let keep = detection.start() >= end;
        if keep {
            end = detection.end();
        }
        keep
    });
}
