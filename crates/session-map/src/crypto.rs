use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::RngCore;
use uuid::Uuid;

use crate::MapError;

pub(crate) const NONCE_LENGTH: usize = 12;

pub(crate) fn encrypt(
    key: &[u8; 32],
    profile_id: Uuid,
    token: &str,
    kind: &str,
    plaintext: &[u8],
) -> Result<([u8; NONCE_LENGTH], Vec<u8>), MapError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| MapError::Crypto)?;
    let mut nonce_bytes = [0_u8; NONCE_LENGTH];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: plaintext,
                aad: &associated_data(profile_id, token, kind),
            },
        )
        .map_err(|_| MapError::Crypto)?;
    Ok((nonce_bytes, ciphertext))
}

pub(crate) fn decrypt(
    key: &[u8; 32],
    profile_id: Uuid,
    token: &str,
    kind: &str,
    nonce_bytes: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, MapError> {
    let nonce: &[u8; NONCE_LENGTH] = nonce_bytes
        .try_into()
        .map_err(|_| MapError::InvalidRecord)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| MapError::Crypto)?;
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &associated_data(profile_id, token, kind),
            },
        )
        .map_err(|_| MapError::AuthenticationFailed)
}

fn associated_data(profile_id: Uuid, token: &str, kind: &str) -> Vec<u8> {
    let mut value = Vec::with_capacity(16 + token.len() + kind.len() + 2);
    value.extend_from_slice(profile_id.as_bytes());
    value.push(0);
    value.extend_from_slice(token.as_bytes());
    value.push(0);
    value.extend_from_slice(kind.as_bytes());
    value
}
