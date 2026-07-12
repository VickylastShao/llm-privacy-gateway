use http::HeaderMap;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::GatewayError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayProtocol {
    OpenAi,
    Anthropic,
}

#[derive(Debug, Clone)]
pub struct AuthProfile {
    pub id: Uuid,
    pub protocol: GatewayProtocol,
    pub local_key_digest: [u8; 32],
}

pub fn local_key_digest(key: &str) -> [u8; 32] {
    Sha256::digest(key.as_bytes()).into()
}

pub fn authenticate(
    headers: &HeaderMap,
    expected_protocol: GatewayProtocol,
    profiles: &[AuthProfile],
) -> Result<Uuid, GatewayError> {
    let supplied = match expected_protocol {
        GatewayProtocol::OpenAi => headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer ")),
        GatewayProtocol::Anthropic => headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok()),
    }
    .ok_or(GatewayError::Unauthorized)?;
    let digest = local_key_digest(supplied);
    let mut matching_protocol = None;
    let mut matching_other_protocol = false;
    for profile in profiles {
        if digest.ct_eq(&profile.local_key_digest).into() {
            if profile.protocol == expected_protocol {
                matching_protocol = Some(profile.id);
            } else {
                matching_other_protocol = true;
            }
        }
    }
    matching_protocol.ok_or(if matching_other_protocol {
        GatewayError::ProtocolMismatch
    } else {
        GatewayError::Unauthorized
    })
}
