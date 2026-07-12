use session_map::{EncryptedRecord, MapError, MemoryBackend, PseudonymMap, token_for};
use uuid::Uuid;

const PROFILE_KEY: [u8; 32] = [7; 32];

fn new_map() -> PseudonymMap<MemoryBackend> {
    PseudonymMap::new(Uuid::nil(), PROFILE_KEY, MemoryBackend::default())
}

#[test]
fn tokens_are_stable_profile_isolated_and_have_eighty_bits() {
    let first = match token_for(&PROFILE_KEY, "email", "alice@example.test") {
        Ok(token) => token,
        Err(error) => panic!("token generation must work: {error}"),
    };
    let repeated = match token_for(&PROFILE_KEY, "email", "alice@example.test") {
        Ok(token) => token,
        Err(error) => panic!("token generation must work: {error}"),
    };
    let other_key = [8; 32];
    let isolated = match token_for(&other_key, "email", "alice@example.test") {
        Ok(token) => token,
        Err(error) => panic!("token generation must work: {error}"),
    };
    assert_eq!(first, repeated);
    assert_ne!(first, isolated);
    assert!(first.starts_with("__LPG_EMAIL_"));
    assert!(first.ends_with("__"));
    assert!(first.len() >= "__LPG_EMAIL_".len() + 16 + 2);
}

#[test]
fn pseudonymization_encrypts_and_restores_text() {
    let mut map = new_map();
    let token = match map.pseudonymize("email", "alice@example.test") {
        Ok(token) => token,
        Err(error) => panic!("pseudonymization must work: {error}"),
    };
    let record = match map.backend().get(&token) {
        Ok(Some(record)) => record,
        Ok(None) => panic!("encrypted record must exist"),
        Err(error) => panic!("backend lookup must work: {error}"),
    };
    assert!(
        !record
            .ciphertext()
            .windows("alice@example.test".len())
            .any(|part| part == b"alice@example.test")
    );
    let restored = match map.restore_text(&format!("contact {token}")) {
        Ok(text) => text,
        Err(error) => panic!("restoration must work: {error}"),
    };
    assert_eq!(restored, "contact alice@example.test");
}

#[test]
fn tampering_and_deletion_are_detected() {
    let mut map = new_map();
    let token = match map.pseudonymize("project", "Falcon") {
        Ok(token) => token,
        Err(error) => panic!("pseudonymization must work: {error}"),
    };
    let mut record = match map.backend().get(&token) {
        Ok(Some(record)) => record,
        Ok(None) => panic!("encrypted record must exist"),
        Err(error) => panic!("backend lookup must work: {error}"),
    };
    record.tamper_for_test();
    match map.backend_mut().put(record) {
        Ok(()) => {}
        Err(error) => panic!("backend replacement must work: {error}"),
    }
    assert!(matches!(
        map.restore_text(&token),
        Err(MapError::AuthenticationFailed)
    ));

    let mut clean = new_map();
    let clean_token = match clean.pseudonymize("project", "Falcon") {
        Ok(token) => token,
        Err(error) => panic!("pseudonymization must work: {error}"),
    };
    match clean.clear() {
        Ok(()) => {}
        Err(error) => panic!("clear must work: {error}"),
    }
    assert!(matches!(clean.backend().get(&clean_token), Ok(None)));
}

#[test]
fn encrypted_record_is_not_plaintext_container() {
    let record = EncryptedRecord::for_test("token", "kind", vec![0; 12], b"ciphertext".to_vec());
    assert_eq!(record.token(), "token");
    assert_eq!(record.kind(), "kind");
}
