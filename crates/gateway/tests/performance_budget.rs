use std::time::{Duration, Instant};

use privacy_core::{Policy, PrivacyEngine, StaticDictionary};
use protocol_openai::OpenAiSseTransformer;
use session_map::{MemoryBackend, PseudonymMap};
use uuid::Uuid;

fn percentile_95(mut values: Vec<Duration>) -> Duration {
    values.sort_unstable();
    values[(values.len() * 95).div_ceil(100).saturating_sub(1)]
}

#[test]
fn scan_overhead_for_100kb_text_stays_within_release_budget() {
    let dictionary = StaticDictionary::new(
        (0..10_000)
            .map(|index| format!("内部项目{index:05}"))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|error| panic!("dictionary: {error}"));
    let engine = PrivacyEngine::new(Vec::new(), dictionary);
    let text = "火电机组运行工况 mixed-language telemetry baseline. ".repeat(2_000);
    assert!(text.len() >= 100_000);
    let mut samples = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        engine
            .inspect(&text, &Policy::strict())
            .unwrap_or_else(|error| panic!("inspection: {error}"));
        samples.push(started.elapsed());
    }
    assert!(
        percentile_95(samples) <= Duration::from_millis(20),
        "100KB scan P95 exceeded the 20ms release budget"
    );
}

#[test]
fn ordinary_sse_first_event_overhead_stays_within_release_budget() {
    let mut map = PseudonymMap::new(Uuid::nil(), [20; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let event = format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{token}\"}}}}]}}\n\n");
    let mut samples = Vec::new();
    for _ in 0..50 {
        let mut transformer = OpenAiSseTransformer::default();
        let started = Instant::now();
        let output = transformer
            .push_bytes(event.as_bytes(), &map)
            .unwrap_or_else(|error| panic!("stream: {error}"));
        assert!(!output.is_empty());
        samples.push(started.elapsed());
    }
    assert!(
        percentile_95(samples) <= Duration::from_millis(30),
        "ordinary SSE first-event P95 exceeded the 30ms release budget"
    );
}
