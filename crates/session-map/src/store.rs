use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::{MapError, crypto, token_for};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedRecord {
    token: String,
    kind: String,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

impl EncryptedRecord {
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    pub fn for_test(token: &str, kind: &str, nonce: Vec<u8>, ciphertext: Vec<u8>) -> Self {
        Self {
            token: token.to_owned(),
            kind: kind.to_owned(),
            nonce,
            ciphertext,
        }
    }

    pub fn tamper_for_test(&mut self) {
        if let Some(first) = self.ciphertext.first_mut() {
            *first ^= 1;
        }
    }
}

pub trait MappingBackend {
    fn get(&self, token: &str) -> Result<Option<EncryptedRecord>, MapError>;
    fn put(&mut self, record: EncryptedRecord) -> Result<(), MapError>;
    fn remove(&mut self, token: &str) -> Result<(), MapError>;
    fn clear(&mut self) -> Result<(), MapError>;
}

#[derive(Debug, Default)]
pub struct MemoryBackend {
    records: HashMap<String, EncryptedRecord>,
}

impl MemoryBackend {
    pub fn get(&self, token: &str) -> Result<Option<EncryptedRecord>, MapError> {
        MappingBackend::get(self, token)
    }

    pub fn put(&mut self, record: EncryptedRecord) -> Result<(), MapError> {
        MappingBackend::put(self, record)
    }
}

impl MappingBackend for MemoryBackend {
    fn get(&self, token: &str) -> Result<Option<EncryptedRecord>, MapError> {
        Ok(self.records.get(token).cloned())
    }

    fn put(&mut self, record: EncryptedRecord) -> Result<(), MapError> {
        self.records.insert(record.token.clone(), record);
        Ok(())
    }

    fn remove(&mut self, token: &str) -> Result<(), MapError> {
        self.records.remove(token);
        Ok(())
    }

    fn clear(&mut self) -> Result<(), MapError> {
        self.records.clear();
        Ok(())
    }
}

pub trait Pseudonymizer {
    fn pseudonymize(&mut self, kind: &str, value: &str) -> Result<String, MapError>;
}

pub struct PseudonymMap<B> {
    profile_id: Uuid,
    key: [u8; 32],
    backend: B,
    tokens: HashSet<String>,
}

impl<B: MappingBackend> PseudonymMap<B> {
    pub fn new(profile_id: Uuid, key: [u8; 32], backend: B) -> Self {
        Self {
            profile_id,
            key,
            backend,
            tokens: HashSet::new(),
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn pseudonymize(&mut self, kind: &str, value: &str) -> Result<String, MapError> {
        let token = token_for(&self.key, kind, value)?;
        if self.backend.get(&token)?.is_none() {
            let (nonce, ciphertext) =
                crypto::encrypt(&self.key, self.profile_id, &token, kind, value.as_bytes())?;
            self.backend.put(EncryptedRecord {
                token: token.clone(),
                kind: kind.to_owned(),
                nonce: nonce.to_vec(),
                ciphertext,
            })?;
        }
        self.tokens.insert(token.clone());
        Ok(token)
    }

    pub fn restore_text(&self, text: &str) -> Result<String, MapError> {
        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;
        while let Some(relative_start) = text[cursor..].find("__LPG_") {
            let start = cursor + relative_start;
            output.push_str(&text[cursor..start]);
            let after_prefix = start + "__LPG_".len();
            let Some(relative_end) = text[after_prefix..].find("__") else {
                output.push_str(&text[start..]);
                return Ok(output);
            };
            let end = after_prefix + relative_end + 2;
            let candidate = &text[start..end];
            match self.backend.get(candidate)? {
                Some(record) => {
                    let plaintext = crypto::decrypt(
                        &self.key,
                        self.profile_id,
                        record.token(),
                        record.kind(),
                        &record.nonce,
                        record.ciphertext(),
                    )?;
                    let value =
                        String::from_utf8(plaintext).map_err(|_| MapError::InvalidRecord)?;
                    output.push_str(&value);
                }
                None => output.push_str(candidate),
            }
            cursor = end;
        }
        output.push_str(&text[cursor..]);
        Ok(output)
    }

    pub fn clear(&mut self) -> Result<(), MapError> {
        self.backend.clear()?;
        self.tokens.clear();
        Ok(())
    }
}

impl<B: MappingBackend> Pseudonymizer for PseudonymMap<B> {
    fn pseudonymize(&mut self, kind: &str, value: &str) -> Result<String, MapError> {
        Self::pseudonymize(self, kind, value)
    }
}
