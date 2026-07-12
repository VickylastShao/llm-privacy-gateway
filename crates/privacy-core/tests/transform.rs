use privacy_core::{
    Detection, Detector, EntityKind, Policy, PrivacyEngine, PrivacyError, StaticDictionary,
};
use session_map::{MemoryBackend, PseudonymMap};
use uuid::Uuid;

struct FixedDetector;

impl Detector for FixedDetector {
    fn name(&self) -> &'static str {
        "fixed"
    }

    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        let key = "sk-test-abcdefghijklmnopqrstuvwxyz";
        let start = match text.find(key) {
            Some(start) => start,
            None => return Ok(Vec::new()),
        };
        let end = start + key.len();
        Detection::new(EntityKind::ApiKey, start, end, 100, "fixed")
            .map(|detection| vec![detection])
    }
}

#[test]
fn blocked_decisions_do_not_write_mapping_records() {
    let engine = PrivacyEngine::new(vec![Box::new(FixedDetector)], StaticDictionary::empty());
    let mut map = PseudonymMap::new(Uuid::nil(), [3; 32], MemoryBackend::default());
    let result = engine.transform(
        "key=sk-test-abcdefghijklmnopqrstuvwxyz",
        &Policy::strict(),
        &mut map,
    );
    assert!(matches!(result, Err(PrivacyError::Blocked)));
    assert_eq!(map.len(), 0);
}

#[test]
fn pseudonymized_text_round_trips_through_the_mapping() {
    let engine = PrivacyEngine::new(
        Vec::new(),
        match StaticDictionary::new(["Alpha Project"]) {
            Ok(dictionary) => dictionary,
            Err(error) => panic!("dictionary must be valid: {error}"),
        },
    );
    let mut map = PseudonymMap::new(Uuid::nil(), [4; 32], MemoryBackend::default());
    let transformed = match engine.transform("review Alpha Project", &Policy::strict(), &mut map) {
        Ok(result) => result,
        Err(error) => panic!("transform must work: {error}"),
    };
    assert_ne!(transformed.outbound_text(), "review Alpha Project");
    let restored = match map.restore_text(transformed.outbound_text()) {
        Ok(text) => text,
        Err(error) => panic!("restore must work: {error}"),
    };
    assert_eq!(restored, "review Alpha Project");
}
