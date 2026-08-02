use activity_store::{
    Activity, ProfileInput, Protocol, Repository, StoreError, validate_upstream_url,
};
use session_map::{MemoryBackend, PseudonymMap};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tempfile::NamedTempFile;
use uuid::Uuid;

#[test]
fn initialization_removes_the_unused_policy_rules_table() {
    let file = NamedTempFile::new().unwrap_or_else(|error| panic!("temp file: {error}"));
    {
        let connection = rusqlite::Connection::open(file.path())
            .unwrap_or_else(|error| panic!("legacy sqlite: {error}"));
        connection
            .execute_batch("CREATE TABLE policy_rules(entity_kind TEXT NOT NULL);")
            .unwrap_or_else(|error| panic!("legacy table: {error}"));
    }
    {
        let _repository =
            Repository::open(file.path()).unwrap_or_else(|error| panic!("repository: {error}"));
    }
    let connection =
        rusqlite::Connection::open(file.path()).unwrap_or_else(|error| panic!("sqlite: {error}"));
    let count: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='policy_rules'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|error| panic!("table query: {error}"));
    assert_eq!(count, 0);
}

#[test]
fn failed_schema_migration_preserves_the_legacy_table_and_rows() {
    let file = NamedTempFile::new().unwrap_or_else(|error| panic!("temp file: {error}"));
    {
        let connection = rusqlite::Connection::open(file.path())
            .unwrap_or_else(|error| panic!("legacy sqlite: {error}"));
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY); \
                 INSERT INTO schema_migrations VALUES(1); \
                 CREATE TABLE activities(legacy_value TEXT NOT NULL); \
                 INSERT INTO activities VALUES('preserve-me'); \
                 CREATE TRIGGER fail_version_two BEFORE INSERT ON schema_migrations \
                 WHEN NEW.version=2 BEGIN SELECT RAISE(ABORT,'migration failure'); END;",
            )
            .unwrap_or_else(|error| panic!("legacy fixture: {error}"));
    }

    assert!(matches!(
        Repository::open(file.path()),
        Err(StoreError::Database)
    ));

    let connection = rusqlite::Connection::open(file.path())
        .unwrap_or_else(|error| panic!("reopen legacy sqlite: {error}"));
    let columns: Vec<String> = connection
        .prepare("SELECT name FROM pragma_table_info('activities') ORDER BY cid")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_else(|error| panic!("legacy columns: {error}"));
    let value: String = connection
        .query_row("SELECT legacy_value FROM activities", [], |row| row.get(0))
        .unwrap_or_else(|error| panic!("legacy row: {error}"));
    assert_eq!(columns, ["legacy_value"]);
    assert_eq!(value, "preserve-me");
}

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
        id: Uuid::new_v4(),
        occurred_at_ms: 1_900_000_000_000,
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
fn profiles_can_be_listed_and_deleted_without_exposing_secret_values() {
    let repo = Repository::open_in_memory().unwrap_or_else(|error| panic!("open: {error}"));
    let first = Uuid::nil();
    let second = Uuid::new_v4();
    for (id, name) in [(first, "first"), (second, "second")] {
        repo.save_profile(&ProfileInput {
            id,
            name: name.to_owned(),
            protocol: Protocol::OpenAi,
            upstream_url: "https://example.test/v1".to_owned(),
            upstream_secret_ref: format!("upstream/{id}"),
            local_secret_ref: format!("local/{id}"),
            local_key_digest: [7; 32],
            policy: "strict".to_owned(),
        })
        .unwrap_or_else(|error| panic!("save: {error}"));
    }
    let profiles = repo
        .profiles()
        .unwrap_or_else(|error| panic!("list: {error}"));
    assert_eq!(profiles.len(), 2);
    assert!(
        profiles
            .iter()
            .all(|profile| profile.name != "CANARY_SECRET")
    );
    assert!(
        repo.delete_profile(first)
            .unwrap_or_else(|error| panic!("delete: {error}"))
    );
    assert!(
        repo.profile(first)
            .unwrap_or_else(|error| panic!("load: {error}"))
            .is_none()
    );
    assert!(
        !repo
            .delete_profile(first)
            .unwrap_or_else(|error| panic!("delete: {error}"))
    );
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

#[test]
fn encrypted_mapping_records_restore_without_plaintext_storage() {
    let repository = Repository::open_in_memory().unwrap_or_else(|error| panic!("open: {error}"));
    let id = Uuid::nil();
    repository
        .save_profile(&ProfileInput {
            id,
            name: "test".into(),
            protocol: Protocol::OpenAi,
            upstream_url: "https://example.test/v1".into(),
            upstream_secret_ref: "upstream/test".into(),
            local_secret_ref: "local/test".into(),
            local_key_digest: [2; 32],
            policy: "strict".into(),
        })
        .unwrap_or_else(|error| panic!("save: {error}"));
    let mut map = PseudonymMap::new(id, [3; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "canary@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let record = map
        .backend()
        .get(&token)
        .unwrap_or_else(|error| panic!("get: {error}"))
        .unwrap_or_else(|| panic!("record missing"));
    repository
        .save_mapping_record(id, &record)
        .unwrap_or_else(|error| panic!("store: {error}"));
    let loaded = repository
        .mapping_records(id)
        .unwrap_or_else(|error| panic!("load: {error}"));
    let restored = PseudonymMap::new(id, [3; 32], MemoryBackend::from_records(loaded))
        .restore_text(&token)
        .unwrap_or_else(|error| panic!("restore: {error}"));
    assert_eq!(restored, "canary@example.test");
    assert!(
        !repository
            .debug_export()
            .unwrap_or_else(|error| panic!("export: {error}"))
            .contains("canary@example.test")
    );
}

#[test]
fn repository_mapping_backend_restores_after_runtime_restart() {
    let repository = Arc::new(Mutex::new(
        Repository::open_in_memory().unwrap_or_else(|error| panic!("open: {error}")),
    ));
    let id = Uuid::new_v4();
    repository
        .lock()
        .unwrap_or_else(|_| panic!("repository lock"))
        .save_profile(&ProfileInput {
            id,
            name: "restart".into(),
            protocol: Protocol::OpenAi,
            upstream_url: "https://example.test/v1".into(),
            upstream_secret_ref: "upstream/restart".into(),
            local_secret_ref: "local/restart".into(),
            local_key_digest: [4; 32],
            policy: "strict".into(),
        })
        .unwrap_or_else(|error| panic!("save: {error}"));

    let token = {
        let backend = activity_store::RepositoryMappingBackend::new(Arc::clone(&repository), id);
        let mut mappings = PseudonymMap::new(id, [9; 32], backend);
        mappings
            .pseudonymize("email", "restart@example.test")
            .unwrap_or_else(|error| panic!("pseudonymize: {error}"))
    };

    let backend = activity_store::RepositoryMappingBackend::new(Arc::clone(&repository), id);
    let mappings = PseudonymMap::new(id, [9; 32], backend);
    assert_eq!(
        mappings
            .restore_text(&token)
            .unwrap_or_else(|error| panic!("restore: {error}")),
        "restart@example.test"
    );
}

#[test]
fn activities_are_newest_first_limited_and_pruned_after_seven_days() {
    let repository = Repository::open_in_memory().unwrap_or_else(|error| panic!("open: {error}"));
    let profile = Uuid::nil();
    repository
        .save_profile(&ProfileInput {
            id: profile,
            name: "activity".into(),
            protocol: Protocol::OpenAi,
            upstream_url: "https://example.test/v1".into(),
            upstream_secret_ref: "upstream/activity".into(),
            local_secret_ref: "local/activity".into(),
            local_key_digest: [5; 32],
            policy: "strict".into(),
        })
        .unwrap_or_else(|error| panic!("save: {error}"));
    let now_ms = 1_900_000_000_000_i64;
    for (offset, request_id) in [
        (-(7 * 24 * 60 * 60 * 1_000) - 1, "expired"),
        (-2_000, "older"),
        (-1_000, "newer"),
    ] {
        repository
            .record_activity(&Activity {
                id: Uuid::new_v4(),
                occurred_at_ms: now_ms + offset,
                profile_id: profile,
                endpoint: "/v1/chat/completions".into(),
                model: "test".into(),
                status: 200,
                latency_ms: 1,
                category_counts: BTreeMap::from([("ipv4".to_owned(), 1)]),
                actions: vec!["pseudonymize".into()],
                request_id: request_id.into(),
            })
            .unwrap_or_else(|error| panic!("activity: {error}"));
    }

    repository
        .prune_activities(now_ms)
        .unwrap_or_else(|error| panic!("prune: {error}"));
    let activities = repository
        .activities(1)
        .unwrap_or_else(|error| panic!("list: {error}"));

    assert_eq!(activities.len(), 1);
    assert_eq!(activities[0].request_id, "newer");
    assert_eq!(
        repository
            .activity_count()
            .unwrap_or_else(|error| panic!("count: {error}")),
        2
    );
}
