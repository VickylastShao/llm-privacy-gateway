#![forbid(unsafe_code)]

use std::{collections::HashMap, sync::Mutex};

use secrecy::SecretString;
use thiserror::Error;
use uuid::Uuid;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SERVICE_NAME: &str = "llm-privacy-gateway";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    Upstream,
    Local,
    Mapping,
    Dictionary,
}

pub fn service_account(kind: SecretKind, profile_id: Uuid) -> (String, String) {
    let prefix = match kind {
        SecretKind::Upstream => "upstream",
        SecretKind::Local => "local",
        SecretKind::Mapping => "mapping",
        SecretKind::Dictionary => "dictionary",
    };
    (SERVICE_NAME.to_owned(), format!("{prefix}/{profile_id}"))
}

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("secret storage operation failed")]
    Storage,
}

pub trait SecretStore: Send + Sync {
    fn put(
        &self,
        kind: SecretKind,
        profile_id: Uuid,
        value: SecretString,
    ) -> Result<(), SecretStoreError>;
    fn get(
        &self,
        kind: SecretKind,
        profile_id: Uuid,
    ) -> Result<Option<SecretString>, SecretStoreError>;
    fn delete(&self, kind: SecretKind, profile_id: Uuid) -> Result<(), SecretStoreError>;
}

#[derive(Debug, Default)]
pub struct MemorySecretStore {
    secrets: Mutex<HashMap<(SecretKind, Uuid), SecretString>>,
}

impl SecretStore for MemorySecretStore {
    fn put(
        &self,
        kind: SecretKind,
        profile_id: Uuid,
        value: SecretString,
    ) -> Result<(), SecretStoreError> {
        self.secrets
            .lock()
            .map_err(|_| SecretStoreError::Storage)?
            .insert((kind, profile_id), value);
        Ok(())
    }
    fn get(
        &self,
        kind: SecretKind,
        profile_id: Uuid,
    ) -> Result<Option<SecretString>, SecretStoreError> {
        Ok(self
            .secrets
            .lock()
            .map_err(|_| SecretStoreError::Storage)?
            .get(&(kind, profile_id))
            .cloned())
    }
    fn delete(&self, kind: SecretKind, profile_id: Uuid) -> Result<(), SecretStoreError> {
        self.secrets
            .lock()
            .map_err(|_| SecretStoreError::Storage)?
            .remove(&(kind, profile_id));
        Ok(())
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Default)]
pub struct WindowsCredentialStore;

#[cfg(target_os = "windows")]
impl SecretStore for WindowsCredentialStore {
    fn put(
        &self,
        kind: SecretKind,
        profile_id: Uuid,
        value: SecretString,
    ) -> Result<(), SecretStoreError> {
        use secrecy::ExposeSecret;
        let (service, account) = service_account(kind, profile_id);
        let entry =
            keyring::Entry::new(&service, &account).map_err(|_| SecretStoreError::Storage)?;
        entry
            .set_password(value.expose_secret())
            .map_err(|_| SecretStoreError::Storage)
    }
    fn get(
        &self,
        kind: SecretKind,
        profile_id: Uuid,
    ) -> Result<Option<SecretString>, SecretStoreError> {
        let (service, account) = service_account(kind, profile_id);
        let entry =
            keyring::Entry::new(&service, &account).map_err(|_| SecretStoreError::Storage)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(SecretString::from(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(SecretStoreError::Storage),
        }
    }
    fn delete(&self, kind: SecretKind, profile_id: Uuid) -> Result<(), SecretStoreError> {
        let (service, account) = service_account(kind, profile_id);
        let entry =
            keyring::Entry::new(&service, &account).map_err(|_| SecretStoreError::Storage)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(SecretStoreError::Storage),
        }
    }
}
