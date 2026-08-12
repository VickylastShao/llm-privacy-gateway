use privacy_core::{
    Detection, Detector, EntityKind, Policy, PrivacyEngine, PrivacyError, RequestPrivacyReport,
    StaticDictionary,
};
use session_map::{MemoryBackend, PseudonymMap};
use uuid::Uuid;

struct ExactDetector {
    matches: Vec<(EntityKind, &'static str)>,
}

impl Detector for ExactDetector {
    fn name(&self) -> &'static str {
        "exact"
    }

    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        self.matches
            .iter()
            .filter_map(|(kind, value)| {
                text.find(value).map(|start| {
                    Detection::new(kind.clone(), start, start + value.len(), 100, "exact")
                })
            })
            .collect()
    }
}

fn engine(matches: Vec<(EntityKind, &'static str)>) -> PrivacyEngine {
    PrivacyEngine::new(
        vec![Box::new(ExactDetector { matches })],
        StaticDictionary::empty(),
    )
}

#[test]
fn blocked_transform_keeps_a_safe_report_without_original_credentials() {
    let secret = "sk-test-abcdefghijklmnopqrstuvwxyz";
    let detector = engine(vec![(EntityKind::ApiKey, secret)]);
    let mut map = PseudonymMap::new(Uuid::nil(), [7; 32], MemoryBackend::default());
    let mut report = RequestPrivacyReport::default();

    let result = detector.transform_reported(
        &format!("api_key={secret}"),
        &Policy::strict(),
        &mut map,
        &mut report,
    );

    assert!(matches!(result, Err(PrivacyError::Blocked)));
    assert_eq!(report.category_counts().get("api_key"), Some(&1));
    assert_eq!(report.actions(), &["block"]);
    let encoded = match serde_json::to_string(&report) {
        Ok(encoded) => encoded,
        Err(error) => panic!("safe report must serialize: {error}"),
    };
    assert!(!encoded.contains(secret));
    assert!(encoded.contains("sk-"));
    assert!(encoded.contains("xyz"));
}

#[test]
fn report_applies_type_specific_masking_and_omits_high_risk_previews() {
    let values = [
        (EntityKind::ChinaResidentId, "11010519491231002X"),
        (EntityKind::BankCard, "4111111111111111"),
        (EntityKind::Ipv4, "10.17.2.35"),
        (EntityKind::Password, "CorrectHorseBatteryStaple"),
        (EntityKind::PrivateKey, "PRIVATEKEYCANARY"),
    ];
    let text = values
        .iter()
        .map(|(_, value)| *value)
        .collect::<Vec<_>>()
        .join(" ");
    let detector = engine(values.to_vec());
    let mut map = PseudonymMap::new(Uuid::nil(), [8; 32], MemoryBackend::default());
    let mut report = RequestPrivacyReport::default();

    let result = detector.transform_reported(&text, &Policy::monitor(), &mut map, &mut report);

    assert!(matches!(result, Err(PrivacyError::Blocked)));
    let previews = report.previews();
    assert!(
        previews
            .iter()
            .any(|item| item.display() == "11**************2X")
    );
    assert!(previews.iter().any(|item| item.display() == "****1111"));
    assert!(previews.iter().any(|item| item.display() == "10.*.*.35"));
    assert!(!previews.iter().any(|item| item.kind() == "password"));
    assert!(!previews.iter().any(|item| item.kind() == "private_key"));
    let encoded = match serde_json::to_string(&report) {
        Ok(encoded) => encoded,
        Err(error) => panic!("safe report must serialize: {error}"),
    };
    for (_, original) in values {
        assert!(!encoded.contains(original));
    }
}

#[test]
fn report_deduplicates_and_limits_previews_per_kind() {
    let values = ["10.1.1.1", "10.2.2.2", "10.3.3.3", "10.4.4.4", "10.4.4.4"];
    let text = values.join(" ");
    let detector = engine(
        values
            .iter()
            .map(|value| (EntityKind::Ipv4, *value))
            .collect(),
    );
    let mut map = PseudonymMap::new(Uuid::nil(), [9; 32], MemoryBackend::default());
    let mut report = RequestPrivacyReport::default();

    let result = detector.transform_reported(&text, &Policy::monitor(), &mut map, &mut report);

    assert!(result.is_ok());
    assert_eq!(report.previews().len(), 3);
}

#[test]
fn report_tracks_uninspected_content_without_recording_payloads() {
    let mut report = RequestPrivacyReport::default();
    report.record_uninspected_content("openai_responses_content:input_image");
    report.record_uninspected_content("openai_responses_content:input_image");
    report.record_uninspected_content("openai_responses_content:input_file");

    assert!(report.has_uninspected_content());
    assert_eq!(
        report.category_counts().get("uninspected_content"),
        Some(&3)
    );
    assert_eq!(report.actions(), &["warn"]);
    assert_eq!(report.previews().len(), 2);
}
