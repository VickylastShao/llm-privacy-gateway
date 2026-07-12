use activity_store::{
    Activity, ProfileInput, Protocol, Repository, StoreError, validate_upstream_url,
};
use std::collections::BTreeMap;
use uuid::Uuid;
#[test]
fn profiles_keep_only_refs_and_activity_metadata() {
    let repo = Repository::open_in_memory().unwrap_or_else(|e| panic!("open: {e}"));
    let id = Uuid::nil();
    repo.save_profile(&ProfileInput {
        id,
        name: "DeepSeek".into(),
        protocol: Protocol::OpenAi,
        upstream_url: "https://api.deepseek.com/v1".into(),
        upstream_secret_ref: format!("upstream/{id}"),
        local_secret_ref: format!("local/{id}"),
        local_key_digest: [1; 32],
        policy: "strict".into(),
    })
    .unwrap_or_else(|e| panic!("save: {e}"));
    repo.record_activity(&Activity {
        profile_id: id,
        endpoint: "/v1/chat/completions".into(),
        model: "deepseek-chat".into(),
        status: 200,
        latency_ms: 1,
        category_counts: BTreeMap::new(),
        actions: vec!["Pseudonymize".into()],
        request_id: "r1".into(),
    })
    .unwrap_or_else(|e| panic!("activity: {e}"));
    assert_eq!(
        repo.profile(id)
            .unwrap_or_else(|e| panic!("load: {e}"))
            .unwrap_or_else(|| panic!("missing"))
            .protocol,
        Protocol::OpenAi
    );
    let export = repo
        .debug_export()
        .unwrap_or_else(|e| panic!("export: {e}"));
    assert!(!export.contains("CANARY_SECRET"));
    assert!(export.contains(&format!("upstream/{id}")));
}
#[test]
fn url_policy_is_https_or_loopback_only() {
    assert!(validate_upstream_url("https://api.deepseek.com/v1").is_ok());
    assert!(validate_upstream_url("http://127.0.0.1:11434/v1").is_ok());
    assert!(matches!(
        validate_upstream_url("http://api.deepseek.com/v1"),
        Err(StoreError::InvalidUpstreamUrl)
    ));
    assert!(matches!(
        validate_upstream_url("https://key@example.test"),
        Err(StoreError::InvalidUpstreamUrl)
    ));
}
