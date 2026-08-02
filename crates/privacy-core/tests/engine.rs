use privacy_core::{
    Detection, Detector, EntityKind, Policy, PolicyAction, PrivacyEngine, PrivacyError,
    StaticDictionary,
};

struct FixedDetector {
    findings: Vec<Detection>,
}

impl Detector for FixedDetector {
    fn name(&self) -> &'static str {
        "fixed"
    }

    fn detect(&self, _text: &str) -> Result<Vec<Detection>, PrivacyError> {
        Ok(self.findings.clone())
    }
}

fn detection(
    kind: EntityKind,
    start: usize,
    end: usize,
    confidence: u8,
    source: &'static str,
) -> Detection {
    match Detection::new(kind, start, end, confidence, source) {
        Ok(detection) => detection,
        Err(error) => panic!("test detection must be valid: {error}"),
    }
}

#[test]
fn strict_policy_blocks_credentials_that_overlap_custom_terms() {
    let text = "api_key=sk-test";
    let engine = PrivacyEngine::new(
        vec![Box::new(FixedDetector {
            findings: vec![
                detection(EntityKind::Custom("sk-test".into()), 8, 15, 100, "fixed"),
                detection(EntityKind::ApiKey, 0, 15, 100, "fixed"),
            ],
        })],
        StaticDictionary::empty(),
    );

    let inspection = match engine.inspect(text, &Policy::strict()) {
        Ok(inspection) => inspection,
        Err(error) => panic!("inspection must succeed: {error}"),
    };

    assert!(inspection.is_blocked());
    assert_eq!(inspection.decisions().len(), 1);
    assert_eq!(
        inspection.decisions()[0].detection().kind(),
        &EntityKind::ApiKey
    );
    assert_eq!(inspection.decisions()[0].action(), PolicyAction::Block);
}

#[test]
fn dictionary_finds_longest_exact_term_and_whitelist_suppresses_it() {
    let text = "send Alpha Project report";
    let dictionary = match StaticDictionary::new(["Alpha", "Alpha Project"]) {
        Ok(dictionary) => dictionary,
        Err(error) => panic!("dictionary must be valid: {error}"),
    };
    let engine = PrivacyEngine::new(Vec::new(), dictionary);

    let first = match engine.inspect(text, &Policy::strict()) {
        Ok(inspection) => inspection,
        Err(error) => panic!("inspection must succeed: {error}"),
    };
    assert_eq!(first.decisions().len(), 1);
    assert_eq!(
        &text[first.decisions()[0].detection().range()],
        "Alpha Project"
    );
    assert_eq!(first.decisions()[0].action(), PolicyAction::Pseudonymize);

    let whitelisted = Policy::strict().with_whitelist(["Alpha Project"]);
    let second = match engine.inspect(text, &whitelisted) {
        Ok(inspection) => inspection,
        Err(error) => panic!("inspection must succeed: {error}"),
    };
    assert!(second.decisions().is_empty());
}

#[test]
fn templates_apply_documented_actions() {
    assert_eq!(
        Policy::strict().action_for(&EntityKind::Email),
        PolicyAction::Pseudonymize
    );
    assert_eq!(
        Policy::balanced().action_for(&EntityKind::Ipv4),
        PolicyAction::Monitor
    );
    assert_eq!(
        Policy::balanced().action_for(&EntityKind::Phone),
        PolicyAction::Mask
    );
    assert_eq!(
        Policy::monitor().action_for(&EntityKind::ApiKey),
        PolicyAction::Block
    );
    assert_eq!(
        Policy::monitor().action_for(&EntityKind::Custom("X".into())),
        PolicyAction::Monitor
    );
}

#[test]
fn engine_rejects_detector_ranges_outside_the_inspected_text() {
    let engine = PrivacyEngine::new(
        vec![Box::new(FixedDetector {
            findings: vec![detection(EntityKind::Email, 0, 9, 100, "fixed")],
        })],
        StaticDictionary::empty(),
    );

    let result = engine.inspect("short", &Policy::strict());
    assert!(matches!(result, Err(PrivacyError::InvalidTextRange)));
}
