#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use session_map::EncryptedRecord;
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
#[derive(Debug, Clone)]
pub struct Activity {
    pub profile_id: Uuid,
    pub endpoint: String,
    pub model: String,
    pub status: u16,
    pub latency_ms: u64,
    pub category_counts: BTreeMap<String, u64>,
    pub actions: Vec<String>,
    pub request_id: String,
}
pub struct Repository {
    connection: Connection,
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

    fn initialize(connection: Connection) -> Result<Self, StoreError> {
        connection.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY); CREATE TABLE IF NOT EXISTS profiles(id TEXT PRIMARY KEY,name TEXT NOT NULL,protocol TEXT NOT NULL,upstream_url TEXT NOT NULL,upstream_secret_ref TEXT NOT NULL,local_secret_ref TEXT NOT NULL,local_key_digest BLOB NOT NULL,policy TEXT NOT NULL); CREATE TABLE IF NOT EXISTS policy_rules(profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,entity_kind TEXT NOT NULL,action TEXT NOT NULL,PRIMARY KEY(profile_id,entity_kind)); CREATE TABLE IF NOT EXISTS pseudonym_records(profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,token TEXT NOT NULL,kind TEXT NOT NULL,nonce BLOB NOT NULL,ciphertext BLOB NOT NULL,PRIMARY KEY(profile_id,token)); CREATE TABLE IF NOT EXISTS activities(id INTEGER PRIMARY KEY,profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,endpoint TEXT NOT NULL,model TEXT NOT NULL,status INTEGER NOT NULL,latency_ms INTEGER NOT NULL,category_counts TEXT NOT NULL,actions TEXT NOT NULL,request_id TEXT NOT NULL); INSERT OR IGNORE INTO schema_migrations VALUES(1);").map_err(|_| StoreError::Database)?;
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
    pub fn record_activity(&self, activity: &Activity) -> Result<(), StoreError> {
        let counts =
            serde_json::to_string(&activity.category_counts).map_err(|_| StoreError::Database)?;
        let actions = serde_json::to_string(&activity.actions).map_err(|_| StoreError::Database)?;
        self.connection.execute("INSERT INTO activities(profile_id,endpoint,model,status,latency_ms,category_counts,actions,request_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![activity.profile_id.to_string(),activity.endpoint,activity.model,activity.status,activity.latency_ms,counts,actions,activity.request_id]).map_err(|_|StoreError::Database)?;
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
