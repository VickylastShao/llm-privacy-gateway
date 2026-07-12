use secrecy::{ExposeSecret, SecretString};
use secret_store::{MemorySecretStore, SecretKind, SecretStore, service_account};
use uuid::Uuid;

#[test]
fn memory_store_and_account_names_follow_contract() {
    let id = Uuid::nil();
    assert_eq!(
        service_account(SecretKind::Upstream, id).1,
        format!("upstream/{id}")
    );
    assert_eq!(
        service_account(SecretKind::Local, id).1,
        format!("local/{id}")
    );
    assert_eq!(
        service_account(SecretKind::Mapping, id).1,
        format!("mapping/{id}")
    );
    let store = MemorySecretStore::default();
    store
        .put(
            SecretKind::Upstream,
            id,
            SecretString::from("synthetic-secret"),
        )
        .unwrap_or_else(|error| panic!("put must work: {error}"));
    assert_eq!(
        store
            .get(SecretKind::Upstream, id)
            .unwrap_or_else(|error| panic!("get must work: {error}"))
            .as_ref()
            .map(ExposeSecret::expose_secret),
        Some("synthetic-secret")
    );
    store
        .delete(SecretKind::Upstream, id)
        .unwrap_or_else(|error| panic!("delete must work: {error}"));
    assert!(
        store
            .get(SecretKind::Upstream, id)
            .unwrap_or_else(|error| panic!("get must work: {error}"))
            .is_none()
    );
}
