#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, params};
use session_map::{EncryptedRecord, MapError, MappingBackend};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database operation failed")]
    Database,
    #[error("upstream URL is not allowed")]
    InvalidUpstreamUrl,
    #[error("stored profile is invalid")]
    InvalidProfile,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAi,
    Anthropic,
}
impl Protocol {
    fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }
    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "openai" => Ok(Self::OpenAi),
            "anthropic" => Ok(Self::Anthropic),
            _ => Err(StoreError::InvalidProfile),
        }
    }
}
#[derive(Debug, Clone)]
pub struct ProfileInput {
    pub id: Uuid,
    pub name: String,
    pub protocol: Protocol,
    pub upstream_url: String,
    pub upstream_secret_ref: String,
    pub local_secret_ref: String,
    pub local_key_digest: [u8; 32],
    pub policy: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub protocol: Protocol,
    pub upstream_url: String,
    pub upstream_secret_ref: String,
    pub local_secret_ref: String,
    pub local_key_digest: [u8; 32],
    pub policy: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub id: Uuid,
    pub occurred_at_ms: i64,
    pub profile_id: Uuid,
    pub endpoint: String,
    pub model: String,
    pub status: u16,
    pub latency_ms: u64,
    pub category_counts: BTreeMap<String, u64>,
    pub actions: Vec<String>,
    pub request_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRecord {
    pub id: Uuid,
    pub occurred_at_ms: i64,
    pub profile_id: Uuid,
    pub model: String,
    pub reported: bool,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageTotal {
    pub profile_id: Uuid,
    pub request_count: u64,
    pub metered_request_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_tokens: u64,
}
pub struct Repository {
    connection: Connection,
}

#[derive(Clone)]
pub struct RepositoryMappingBackend {
    repository: Arc<Mutex<Repository>>,
    profile_id: Uuid,
}

impl RepositoryMappingBackend {
    #[must_use]
    pub const fn new(repository: Arc<Mutex<Repository>>, profile_id: Uuid) -> Self {
        Self {
            repository,
            profile_id,
        }
    }
}

impl MappingBackend for RepositoryMappingBackend {
    fn get(&self, token: &str) -> Result<Option<EncryptedRecord>, MapError> {
        self.repository
            .lock()
            .map_err(|_| MapError::Backend)?
            .mapping_record(self.profile_id, token)
            .map_err(|_| MapError::Backend)
    }

    fn put(&mut self, record: EncryptedRecord) -> Result<(), MapError> {
        self.repository
            .lock()
            .map_err(|_| MapError::Backend)?
            .save_mapping_record(self.profile_id, &record)
            .map_err(|_| MapError::Backend)
    }

    fn remove(&mut self, token: &str) -> Result<(), MapError> {
        self.repository
            .lock()
            .map_err(|_| MapError::Backend)?
            .delete_mapping_record(self.profile_id, token)
            .map_err(|_| MapError::Backend)
    }

    fn clear(&mut self) -> Result<(), MapError> {
        self.repository
            .lock()
            .map_err(|_| MapError::Backend)?
            .clear_mapping_records(self.profile_id)
            .map_err(|_| MapError::Backend)
    }
}

impl Repository {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path).map_err(|_| StoreError::Database)?;
        Self::initialize(connection)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory().map_err(|_| StoreError::Database)?;
        Self::initialize(connection)
    }

    fn initialize(mut connection: Connection) -> Result<Self, StoreError> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|_| StoreError::Database)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(|_| StoreError::Database)?;
        let transaction = connection.transaction().map_err(|_| StoreError::Database)?;
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY); CREATE TABLE IF NOT EXISTS profiles(id TEXT PRIMARY KEY,name TEXT NOT NULL,protocol TEXT NOT NULL,upstream_url TEXT NOT NULL,upstream_secret_ref TEXT NOT NULL,local_secret_ref TEXT NOT NULL,local_key_digest BLOB NOT NULL,policy TEXT NOT NULL); CREATE TABLE IF NOT EXISTS pseudonym_records(profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,token TEXT NOT NULL,kind TEXT NOT NULL,nonce BLOB NOT NULL,ciphertext BLOB NOT NULL,PRIMARY KEY(profile_id,token)); INSERT OR IGNORE INTO schema_migrations VALUES(1);").map_err(|_| StoreError::Database)?;
        let has_timestamp = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('activities') WHERE name='occurred_at_ms')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| StoreError::Database)?;
        if !has_timestamp {
            transaction
                .execute_batch("DROP TABLE IF EXISTS activities; CREATE TABLE activities(id TEXT PRIMARY KEY,occurred_at_ms INTEGER NOT NULL,profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,endpoint TEXT NOT NULL,model TEXT NOT NULL,status INTEGER NOT NULL,latency_ms INTEGER NOT NULL,category_counts TEXT NOT NULL,actions TEXT NOT NULL,request_id TEXT NOT NULL); INSERT OR IGNORE INTO schema_migrations VALUES(2);")
                .map_err(|_| StoreError::Database)?;
        }
        transaction
            .execute_batch("CREATE INDEX IF NOT EXISTS activities_occurred_at_idx ON activities(occurred_at_ms DESC,id DESC); CREATE TABLE IF NOT EXISTS usage_records(id TEXT PRIMARY KEY,occurred_at_ms INTEGER NOT NULL,profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,model TEXT NOT NULL,input_tokens INTEGER NOT NULL,output_tokens INTEGER NOT NULL,total_tokens INTEGER NOT NULL,cached_tokens INTEGER NOT NULL,reported INTEGER NOT NULL DEFAULT 1); CREATE INDEX IF NOT EXISTS usage_records_time_idx ON usage_records(occurred_at_ms DESC,id DESC); DROP TABLE IF EXISTS policy_rules; INSERT OR IGNORE INTO schema_migrations VALUES(2); INSERT OR IGNORE INTO schema_migrations VALUES(3); INSERT OR IGNORE INTO schema_migrations VALUES(4);")
            .map_err(|_| StoreError::Database)?;
        let has_reported = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('usage_records') WHERE name='reported')",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| StoreError::Database)?;
        if !has_reported {
            transaction
                .execute_batch("ALTER TABLE usage_records ADD COLUMN reported INTEGER NOT NULL DEFAULT 1; INSERT OR IGNORE INTO schema_migrations VALUES(5);")
                .map_err(|_| StoreError::Database)?;
        } else {
            transaction
                .execute("INSERT OR IGNORE INTO schema_migrations VALUES(5)", [])
                .map_err(|_| StoreError::Database)?;
        }
        transaction.commit().map_err(|_| StoreError::Database)?;
        Ok(Self { connection })
    }
    pub fn save_profile(&self, input: &ProfileInput) -> Result<(), StoreError> {
        validate_upstream_url(&input.upstream_url)?;
        self.connection.execute("INSERT INTO profiles VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET name=excluded.name,protocol=excluded.protocol,upstream_url=excluded.upstream_url,upstream_secret_ref=excluded.upstream_secret_ref,local_secret_ref=excluded.local_secret_ref,local_key_digest=excluded.local_key_digest,policy=excluded.policy", params![input.id.to_string(),input.name,input.protocol.as_str(),input.upstream_url,input.upstream_secret_ref,input.local_secret_ref,input.local_key_digest.as_slice(),input.policy]).map_err(|_| StoreError::Database)?;
        Ok(())
    }
    pub fn profile(&self, id: Uuid) -> Result<Option<Profile>, StoreError> {
        self.connection.query_row("SELECT id,name,protocol,upstream_url,upstream_secret_ref,local_secret_ref,local_key_digest,policy FROM profiles WHERE id=?1", [id.to_string()], |row| { let raw_id:String=row.get(0)?; let protocol:String=row.get(2)?; let digest:Vec<u8>=row.get(6)?; let parsed_id=Uuid::parse_str(&raw_id).map_err(|_| rusqlite::Error::InvalidQuery)?; let digest:[u8;32]=digest.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?; Ok(Profile{id:parsed_id,name:row.get(1)?,protocol:Protocol::parse(&protocol).map_err(|_| rusqlite::Error::InvalidQuery)?,upstream_url:row.get(3)?,upstream_secret_ref:row.get(4)?,local_secret_ref:row.get(5)?,local_key_digest:digest,policy:row.get(7)?}) }).optional().map_err(|_| StoreError::Database)
    }
    pub fn profiles(&self) -> Result<Vec<Profile>, StoreError> {
        let mut statement = self.connection.prepare("SELECT id,name,protocol,upstream_url,upstream_secret_ref,local_secret_ref,local_key_digest,policy FROM profiles ORDER BY name,id").map_err(|_| StoreError::Database)?;
        let rows = statement
            .query_map([], |row| {
                let raw_id: String = row.get(0)?;
                let protocol: String = row.get(2)?;
                let digest: Vec<u8> = row.get(6)?;
                let parsed_id =
                    Uuid::parse_str(&raw_id).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let digest: [u8; 32] = digest
                    .try_into()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                Ok(Profile {
                    id: parsed_id,
                    name: row.get(1)?,
                    protocol: Protocol::parse(&protocol)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    upstream_url: row.get(3)?,
                    upstream_secret_ref: row.get(4)?,
                    local_secret_ref: row.get(5)?,
                    local_key_digest: digest,
                    policy: row.get(7)?,
                })
            })
            .map_err(|_| StoreError::Database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)
    }
    pub fn delete_profile(&self, id: Uuid) -> Result<bool, StoreError> {
        let changed = self
            .connection
            .execute("DELETE FROM profiles WHERE id=?1", [id.to_string()])
            .map_err(|_| StoreError::Database)?;
        Ok(changed == 1)
    }
    pub fn record_activity(&self, activity: &Activity) -> Result<(), StoreError> {
        let counts =
            serde_json::to_string(&activity.category_counts).map_err(|_| StoreError::Database)?;
        let actions = serde_json::to_string(&activity.actions).map_err(|_| StoreError::Database)?;
        self.connection.execute("INSERT INTO activities(id,occurred_at_ms,profile_id,endpoint,model,status,latency_ms,category_counts,actions,request_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![activity.id.to_string(),activity.occurred_at_ms,activity.profile_id.to_string(),activity.endpoint,activity.model,activity.status,activity.latency_ms,counts,actions,activity.request_id]).map_err(|_|StoreError::Database)?;
        Ok(())
    }

    pub fn activities(&self, limit: usize) -> Result<Vec<Activity>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit.min(100)).map_err(|_| StoreError::Database)?;
        let mut statement = self.connection.prepare("SELECT id,occurred_at_ms,profile_id,endpoint,model,status,latency_ms,category_counts,actions,request_id FROM activities ORDER BY occurred_at_ms DESC,id DESC LIMIT ?1").map_err(|_| StoreError::Database)?;
        let rows = statement
            .query_map([limit], |row| {
                let raw_id: String = row.get(0)?;
                let raw_profile_id: String = row.get(2)?;
                let status: i64 = row.get(5)?;
                let latency_ms: i64 = row.get(6)?;
                let category_counts: String = row.get(7)?;
                let actions: String = row.get(8)?;
                Ok(Activity {
                    id: Uuid::parse_str(&raw_id).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    occurred_at_ms: row.get(1)?,
                    profile_id: Uuid::parse_str(&raw_profile_id)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    endpoint: row.get(3)?,
                    model: row.get(4)?,
                    status: u16::try_from(status).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    latency_ms: u64::try_from(latency_ms)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    category_counts: serde_json::from_str(&category_counts)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    actions: serde_json::from_str(&actions)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    request_id: row.get(9)?,
                })
            })
            .map_err(|_| StoreError::Database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)
    }

    pub fn activity_count(&self) -> Result<u64, StoreError> {
        self.connection
            .query_row("SELECT COUNT(*) FROM activities", [], |row| {
                row.get::<_, u64>(0)
            })
            .map_err(|_| StoreError::Database)
    }

    pub fn record_usage(&self, usage: &UsageRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO usage_records(id,occurred_at_ms,profile_id,model,input_tokens,output_tokens,total_tokens,cached_tokens,reported) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET input_tokens=CASE WHEN excluded.reported=1 THEN excluded.input_tokens ELSE usage_records.input_tokens END,output_tokens=CASE WHEN excluded.reported=1 THEN excluded.output_tokens ELSE usage_records.output_tokens END,total_tokens=CASE WHEN excluded.reported=1 THEN excluded.total_tokens ELSE usage_records.total_tokens END,cached_tokens=CASE WHEN excluded.reported=1 THEN excluded.cached_tokens ELSE usage_records.cached_tokens END,reported=MAX(usage_records.reported,excluded.reported)",
            params![
                usage.id.to_string(),
                usage.occurred_at_ms,
                usage.profile_id.to_string(),
                usage.model,
                usage.input_tokens,
                usage.output_tokens,
                usage.total_tokens,
                usage.cached_tokens,
                usage.reported,
            ],
        ).map_err(|_| StoreError::Database)?;
        Ok(())
    }

    pub fn usage_totals(&self, since_ms: i64) -> Result<Vec<UsageTotal>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT profile_id,COUNT(*),COALESCE(SUM(reported),0),COALESCE(SUM(input_tokens),0),COALESCE(SUM(output_tokens),0),COALESCE(SUM(total_tokens),0),COALESCE(SUM(cached_tokens),0) FROM usage_records WHERE occurred_at_ms>=?1 GROUP BY profile_id ORDER BY profile_id",
        ).map_err(|_| StoreError::Database)?;
        let rows = statement
            .query_map([since_ms], |row| {
                let profile_id: String = row.get(0)?;
                Ok(UsageTotal {
                    profile_id: Uuid::parse_str(&profile_id)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    request_count: row.get(1)?,
                    metered_request_count: row.get(2)?,
                    input_tokens: row.get(3)?,
                    output_tokens: row.get(4)?,
                    total_tokens: row.get(5)?,
                    cached_tokens: row.get(6)?,
                })
            })
            .map_err(|_| StoreError::Database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)
    }

    pub fn activity_ids(&self) -> Result<Vec<Uuid>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM activities")
            .map_err(|_| StoreError::Database)?;
        let rows = statement
            .query_map([], |row| {
                let raw_id: String = row.get(0)?;
                Uuid::parse_str(&raw_id).map_err(|_| rusqlite::Error::InvalidQuery)
            })
            .map_err(|_| StoreError::Database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)
    }

    pub fn prune_activities(&self, now_ms: i64) -> Result<(), StoreError> {
        const RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1_000;
        self.connection
            .execute(
                "DELETE FROM activities WHERE occurred_at_ms < ?1",
                [now_ms.saturating_sub(RETENTION_MS)],
            )
            .map_err(|_| StoreError::Database)?;
        self.connection
            .execute(
                "DELETE FROM usage_records WHERE occurred_at_ms < ?1",
                [now_ms.saturating_sub(RETENTION_MS)],
            )
            .map_err(|_| StoreError::Database)?;
        self.connection
            .execute("DELETE FROM activities WHERE id IN (SELECT id FROM activities ORDER BY occurred_at_ms DESC,id DESC LIMIT -1 OFFSET 10000)", [])
            .map_err(|_| StoreError::Database)?;
        Ok(())
    }

    pub fn save_mapping_record(
        &self,
        profile_id: Uuid,
        record: &EncryptedRecord,
    ) -> Result<(), StoreError> {
        self.connection.execute("INSERT INTO pseudonym_records(profile_id,token,kind,nonce,ciphertext) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(profile_id,token) DO UPDATE SET kind=excluded.kind,nonce=excluded.nonce,ciphertext=excluded.ciphertext", params![profile_id.to_string(), record.token(), record.kind(), record.nonce(), record.ciphertext()]).map_err(|_| StoreError::Database)?;
        Ok(())
    }

    pub fn mapping_records(&self, profile_id: Uuid) -> Result<Vec<EncryptedRecord>, StoreError> {
        let mut statement = self.connection.prepare("SELECT token,kind,nonce,ciphertext FROM pseudonym_records WHERE profile_id=?1 ORDER BY token").map_err(|_| StoreError::Database)?;
        let rows = statement
            .query_map([profile_id.to_string()], |row| {
                let token: String = row.get(0)?;
                let kind: String = row.get(1)?;
                let nonce: Vec<u8> = row.get(2)?;
                let ciphertext: Vec<u8> = row.get(3)?;
                EncryptedRecord::from_encrypted_parts(token, kind, nonce, ciphertext)
                    .map_err(|_| rusqlite::Error::InvalidQuery)
            })
            .map_err(|_| StoreError::Database)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| StoreError::Database)
    }

    pub fn mapping_record(
        &self,
        profile_id: Uuid,
        token: &str,
    ) -> Result<Option<EncryptedRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT token,kind,nonce,ciphertext FROM pseudonym_records WHERE profile_id=?1 AND token=?2",
                params![profile_id.to_string(), token],
                |row| {
                    EncryptedRecord::from_encrypted_parts(
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                    )
                    .map_err(|_| rusqlite::Error::InvalidQuery)
                },
            )
            .optional()
            .map_err(|_| StoreError::Database)
    }

    pub fn delete_mapping_record(&self, profile_id: Uuid, token: &str) -> Result<(), StoreError> {
        self.connection
            .execute(
                "DELETE FROM pseudonym_records WHERE profile_id=?1 AND token=?2",
                params![profile_id.to_string(), token],
            )
            .map_err(|_| StoreError::Database)?;
        Ok(())
    }

    pub fn clear_mapping_records(&self, profile_id: Uuid) -> Result<(), StoreError> {
        self.connection
            .execute(
                "DELETE FROM pseudonym_records WHERE profile_id=?1",
                [profile_id.to_string()],
            )
            .map_err(|_| StoreError::Database)?;
        Ok(())
    }
    pub fn debug_export(&self) -> Result<String, StoreError> {
        self.connection.query_row("SELECT group_concat(name || ':' || upstream_secret_ref || ':' || local_secret_ref) FROM profiles",[],|row|row.get::<_,Option<String>>(0)).map(|value|value.unwrap_or_default()).map_err(|_|StoreError::Database)
    }
}
pub fn validate_upstream_url(value: &str) -> Result<Url, StoreError> {
    let url = Url::parse(value).map_err(|_| StoreError::InvalidUpstreamUrl)?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(StoreError::InvalidUpstreamUrl);
    }
    match url.scheme() {
        "https" => Ok(url),
        "http"
            if matches!(
                url.host_str(),
                Some("localhost") | Some("127.0.0.1") | Some("::1")
            ) =>
        {
            Ok(url)
        }
        _ => Err(StoreError::InvalidUpstreamUrl),
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    #[test]
    fn aggregates_usage_by_profile_and_time_window() {
        let repository = match Repository::open_in_memory() {
            Ok(repository) => repository,
            Err(error) => panic!("open repository: {error}"),
        };
        let profile_id = Uuid::new_v4();
        let saved = repository.save_profile(&ProfileInput {
            id: profile_id,
            name: "fixture".to_owned(),
            protocol: Protocol::OpenAi,
            upstream_url: "https://api.example.test/v1".to_owned(),
            upstream_secret_ref: "upstream".to_owned(),
            local_secret_ref: "local".to_owned(),
            local_key_digest: [0; 32],
            policy: "strict".to_owned(),
        });
        assert!(saved.is_ok());
        for (occurred_at_ms, input, output, cached, reported) in
            [(1_000, 10, 4, 2, true), (2_000, 0, 0, 0, false)]
        {
            let recorded = repository.record_usage(&UsageRecord {
                id: Uuid::new_v4(),
                occurred_at_ms,
                profile_id,
                model: "fixture-model".to_owned(),
                reported,
                input_tokens: input,
                output_tokens: output,
                total_tokens: input + output,
                cached_tokens: cached,
            });
            assert!(recorded.is_ok());
        }

        let metered_id = Uuid::new_v4();
        assert!(
            repository
                .record_usage(&UsageRecord {
                    id: metered_id,
                    occurred_at_ms: 3_000,
                    profile_id,
                    model: "fixture-model".to_owned(),
                    reported: false,
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    cached_tokens: 0,
                })
                .is_ok()
        );
        assert!(
            repository
                .record_usage(&UsageRecord {
                    id: metered_id,
                    occurred_at_ms: 3_000,
                    profile_id,
                    model: "fixture-model".to_owned(),
                    reported: true,
                    input_tokens: 20,
                    output_tokens: 6,
                    total_tokens: 26,
                    cached_tokens: 3,
                })
                .is_ok()
        );

        let totals = match repository.usage_totals(1_500) {
            Ok(totals) => totals,
            Err(error) => panic!("query totals: {error}"),
        };
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].request_count, 2);
        assert_eq!(totals[0].metered_request_count, 1);
        assert_eq!(totals[0].input_tokens, 20);
        assert_eq!(totals[0].output_tokens, 6);
        assert_eq!(totals[0].total_tokens, 26);
        assert_eq!(totals[0].cached_tokens, 3);
    }
}
