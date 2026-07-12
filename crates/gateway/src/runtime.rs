use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use privacy_core::{
    BankCardDetector, ChinaIdDetector, CredentialsDetector, NetworkDetector, Policy, PrivacyEngine,
    StaticDictionary, UnifiedSocialCreditCodeDetector,
};
use secrecy::SecretString;
use session_map::{MemoryBackend, PseudonymMap};
use uuid::Uuid;

use crate::{AuthProfile, GatewayProtocol, local_key_digest};

pub struct RuntimeProfile {
    pub id: Uuid,
    pub protocol: GatewayProtocol,
    pub local_key: String,
    pub upstream_base_url: String,
    pub upstream_key: SecretString,
    pub policy: Policy,
    pub mappings: Mutex<PseudonymMap<MemoryBackend>>,
}

impl RuntimeProfile {
    pub fn new(
        id: Uuid,
        protocol: GatewayProtocol,
        local_key: String,
        upstream_base_url: String,
        upstream_key: SecretString,
        mapping_key: [u8; 32],
    ) -> Self {
        Self {
            id,
            protocol,
            local_key,
            upstream_base_url,
            upstream_key,
            policy: Policy::strict(),
            mappings: Mutex::new(PseudonymMap::new(id, mapping_key, MemoryBackend::default())),
        }
    }
    pub fn auth_profile(&self) -> AuthProfile {
        AuthProfile {
            id: self.id,
            protocol: self.protocol,
            local_key_digest: local_key_digest(&self.local_key),
        }
    }
}

pub struct GatewayState {
    pub client: reqwest::Client,
    pub profiles: HashMap<Uuid, Arc<RuntimeProfile>>,
    pub engine: PrivacyEngine,
}
impl GatewayState {
    pub fn new(
        profiles: impl IntoIterator<Item = RuntimeProfile>,
    ) -> Result<Arc<Self>, reqwest::Error> {
        let profiles = profiles
            .into_iter()
            .map(|profile| (profile.id, Arc::new(profile)))
            .collect();
        Ok(Arc::new(Self {
            client: crate::upstream_client()?,
            profiles,
            engine: PrivacyEngine::new(
                vec![
                    Box::new(CredentialsDetector),
                    Box::new(ChinaIdDetector),
                    Box::new(UnifiedSocialCreditCodeDetector),
                    Box::new(BankCardDetector),
                    Box::new(NetworkDetector),
                ],
                StaticDictionary::empty(),
            ),
        }))
    }
    pub fn auth_profiles(&self) -> Vec<AuthProfile> {
        self.profiles
            .values()
            .map(|profile| profile.auth_profile())
            .collect()
    }
}
