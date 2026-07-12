use base32::{Alphabet, encode};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::MapError;

type HmacSha256 = Hmac<Sha256>;

/// Produces a stable, opaque token scoped to a profile secret.
pub fn token_for(key: &[u8; 32], kind: &str, value: &str) -> Result<String, MapError> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| MapError::Crypto)?;
    mac.update(kind.as_bytes());
    mac.update(&[0]);
    mac.update(value.as_bytes());
    let digest = mac.finalize().into_bytes();
    let encoded = encode(Alphabet::Rfc4648 { padding: false }, &digest[..10]);
    Ok(format!("__LPG_{}_{}__", kind.to_ascii_uppercase(), encoded))
}
