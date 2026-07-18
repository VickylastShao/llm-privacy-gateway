use std::{sync::Arc, thread, time::Duration};

use activity_service::ActivityService;
use activity_store::{ProfileInput, Protocol, Repository};
use gateway::{ActivityEvent, ActivitySink};
use privacy_core::{
    CredentialsDetector, Policy, PrivacyEngine, RequestPrivacyReport, StaticDictionary,
};
use session_map::{MemoryBackend, PseudonymMap};
use tempfile::NamedTempFile;
use uuid::Uuid;

fn report_for(secret: &str) -> RequestPrivacyReport {
    let engine = PrivacyEngine::new(
        vec![Box::new(CredentialsDetector)],
        StaticDictionary::empty(),
    );
    let mut map = PseudonymMap::new(Uuid::nil(), [31; 32], MemoryBackend::default());
    let mut report = RequestPrivacyReport::default();
    let _ = engine.transform_reported(secret, &Policy::strict(), &mut map, &mut report);
    report
}

fn event(profile_id: Uuid, secret: &str) -> ActivityEvent {
    ActivityEvent {
        id: Uuid::new_v4(),
        occurred_at_ms: 1_900_000_000_000,
        profile_id,
        endpoint: "/v1/chat/completions".to_owned(),
        model: "deepseek-chat".to_owned(),
        status: 403,
        latency_ms: 2,
        request_id: "request-1".to_owned(),
        report: report_for(secret),
    }
}

#[test]
fn session_preview_is_immediate_but_is_not_restored_from_sqlite() {
    const SECRET: &str = "api_key=sk-test-abcdefghijklmnopqrstuvwxyz";
    let file = NamedTempFile::new().unwrap_or_else(|error| panic!("temp file: {error}"));
    let profile_id = Uuid::nil();
    {
        let repository =
            Repository::open(file.path()).unwrap_or_else(|error| panic!("repository: {error}"));
        repository
            .save_profile(&ProfileInput {
                id: profile_id,
                name: "DeepSeek".to_owned(),
                protocol: Protocol::OpenAi,
                upstream_url: "https://example.test/v1".to_owned(),
                upstream_secret_ref: "upstream/test".to_owned(),
                local_secret_ref: "local/test".to_owned(),
                local_key_digest: [1; 32],
                policy: "strict".to_owned(),
            })
            .unwrap_or_else(|error| panic!("profile: {error}"));
    }
    let service = ActivityService::open(file.path())
        .unwrap_or_else(|error| panic!("activity service: {error}"));

    assert!(service.publish(event(profile_id, SECRET)));
    let immediate = service
        .query(10, 1_900_000_000_001)
        .unwrap_or_else(|error| panic!("query: {error}"));
    assert_eq!(immediate.activities.len(), 1);
    assert!(immediate.activities[0].preview_available);
    assert_eq!(immediate.activities[0].previews.len(), 1);
    assert!(!format!("{immediate:?}").contains(SECRET));

    for _ in 0..50 {
        if immediate_persisted(&service) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(immediate_persisted(&service));
    drop(service);

    let reopened = ActivityService::open(file.path())
        .unwrap_or_else(|error| panic!("reopen activity service: {error}"));
    let history = reopened
        .query(10, 1_900_000_000_002)
        .unwrap_or_else(|error| panic!("history: {error}"));
    assert_eq!(history.activities.len(), 1);
    assert!(!history.activities[0].preview_available);
    assert!(history.activities[0].previews.is_empty());
}

fn immediate_persisted(service: &Arc<ActivityService>) -> bool {
    service.persisted_count().is_ok_and(|count| count == 1)
}

#[test]
fn session_preview_count_excludes_category_only_events() {
    let file = NamedTempFile::new().unwrap_or_else(|error| panic!("temp file: {error}"));
    let service = ActivityService::open(file.path())
        .unwrap_or_else(|error| panic!("activity service: {error}"));

    assert!(service.publish(event(
        Uuid::nil(),
        "api_key=sk-test-abcdefghijklmnopqrstuvwxyz"
    )));
    assert!(service.publish(event(Uuid::nil(), "password=super-secret-value")));

    let activity = service
        .query(10, 1_900_000_000_001)
        .unwrap_or_else(|error| panic!("query: {error}"));
    assert_eq!(activity.activities.len(), 2);
    assert_eq!(activity.session_preview_count, 1);
}
