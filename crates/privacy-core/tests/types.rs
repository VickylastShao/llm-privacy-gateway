use std::collections::HashSet;

use privacy_core::{Detection, Detector, EntityKind, PrivacyError};

#[test]
fn rejects_reversed_range() {
    let result = Detection::new(EntityKind::Email, 8, 3, 50, "test");

    assert!(matches!(
        result,
        Err(PrivacyError::InvalidRange { start: 8, end: 3 })
    ));
}

#[test]
fn rejects_empty_range() {
    let result = Detection::new(EntityKind::Email, 4, 4, 50, "test");

    assert!(matches!(
        result,
        Err(PrivacyError::InvalidRange { start: 4, end: 4 })
    ));
}

#[test]
fn exposes_valid_detection_metadata() -> Result<(), PrivacyError> {
    let detection = Detection::new(EntityKind::Phone, 2, 13, 87, "fake-detector")?;

    assert_eq!(detection.kind(), &EntityKind::Phone);
    assert_eq!(detection.start(), 2);
    assert_eq!(detection.end(), 13);
    assert_eq!(detection.confidence(), 87);
    assert_eq!(detection.source(), "fake-detector");
    assert_eq!(detection.range(), 2..13);
    Ok(())
}

#[test]
fn accepts_confidence_bounds() {
    assert!(Detection::new(EntityKind::Ipv4, 0, 1, 0, "test").is_ok());
    assert!(Detection::new(EntityKind::Ipv6, 0, 1, 100, "test").is_ok());
}

#[test]
fn rejects_confidence_above_one_hundred() {
    let result = Detection::new(EntityKind::Email, 0, 1, 101, "test");

    assert!(matches!(
        result,
        Err(PrivacyError::InvalidConfidence { confidence: 101 })
    ));
}

#[test]
fn custom_entity_kind_supports_equality_and_hashing() {
    let first = EntityKind::Custom("employee-id".to_owned());
    let equal = EntityKind::Custom("employee-id".to_owned());
    let distinct = EntityKind::Custom("device-id".to_owned());
    let mut kinds = HashSet::new();

    assert_eq!(first, equal);
    assert_ne!(first, distinct);
    assert!(kinds.insert(first));
    assert!(!kinds.insert(equal));
    assert!(kinds.insert(distinct));
    assert_eq!(kinds.len(), 2);
}

struct FakeDetector;

impl Detector for FakeDetector {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        let start = text.find('@').unwrap_or(text.len());
        Detection::new(EntityKind::Email, start, text.len(), 100, self.name())
            .map(|detection| vec![detection])
    }
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn detector_contract_is_send_sync_and_returns_metadata_only() -> Result<(), PrivacyError> {
    assert_send_sync::<FakeDetector>();
    let sensitive_text = "prefix@private.example";
    let detections = FakeDetector.detect(sensitive_text)?;

    assert_eq!(detections.len(), 1);
    let Some(detection) = detections.first() else {
        panic!("fake detector returned no detection");
    };
    assert_eq!(detection.kind(), &EntityKind::Email);
    assert_eq!(detection.range(), 6..sensitive_text.len());
    assert_eq!(detection.source(), "fake");
    Ok(())
}
