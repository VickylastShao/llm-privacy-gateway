use std::{collections::BTreeMap, fs};

use activity_store::{Activity, ProfileInput, Protocol, Repository};
use session_map::{MemoryBackend, PseudonymMap};
use tempfile::NamedTempFile;
use uuid::Uuid;

#[test]
fn plaintext_canaries_are_absent_from_persisted_sqlite_database() {
    const PROMPT: &str = "LPG_CANARY_PROMPT_9fd7a1";
    const KEY: &str = "LPG_CANARY_KEY_5c2e8b";
    let file = NamedTempFile::new().unwrap_or_else(|error| panic!("temp file: {error}"));
    let profile = Uuid::nil();
    {
        let repository =
            Repository::open(file.path()).unwrap_or_else(|error| panic!("repository: {error}"));
        repository
            .save_profile(&ProfileInput {
                id: profile,
                name: "canary".to_owned(),
                protocol: Protocol::OpenAi,
                upstream_url: "https://example.test/v1".to_owned(),
                upstream_secret_ref: "upstream/canary".to_owned(),
                local_secret_ref: "local/canary".to_owned(),
                local_key_digest: [3; 32],
                policy: "strict".to_owned(),
            })
            .unwrap_or_else(|error| panic!("profile: {error}"));
        repository
            .record_activity(&Activity {
                id: Uuid::new_v4(),
                occurred_at_ms: 1_900_000_000_000,
                profile_id: profile,
                endpoint: "/v1/chat/completions".to_owned(),
                model: "test".to_owned(),
                status: 200,
                latency_ms: 1,
                category_counts: BTreeMap::from([("Email".to_owned(), 1)]),
                actions: vec!["Pseudonymize".to_owned()],
                request_id: "request-canary".to_owned(),
            })
            .unwrap_or_else(|error| panic!("activity: {error}"));
        let mut mappings = PseudonymMap::new(profile, [4; 32], MemoryBackend::default());
        let token = mappings
            .pseudonymize("custom", PROMPT)
            .unwrap_or_else(|error| panic!("mapping: {error}"));
        let record = mappings
            .backend()
            .get(&token)
            .unwrap_or_else(|error| panic!("mapping: {error}"))
            .unwrap_or_else(|| panic!("record missing"));
        repository
            .save_mapping_record(profile, &record)
            .unwrap_or_else(|error| panic!("mapping record: {error}"));
        assert_ne!(KEY, PROMPT);
    }
    let bytes = fs::read(file.path()).unwrap_or_else(|error| panic!("database bytes: {error}"));
    let contents = String::from_utf8_lossy(&bytes);
    assert!(!contents.contains(PROMPT));
    assert!(!contents.contains(KEY));
}
