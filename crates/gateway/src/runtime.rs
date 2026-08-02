use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use privacy_core::{
    BankCardDetector, ChinaIdDetector, CredentialsDetector, NetworkDetector, PhoneDetector, Policy,
    PrivacyEngine, StaticDictionary, UnifiedSocialCreditCodeDetector,
};
use secrecy::SecretString;
use session_map::{MappingBackend, MemoryBackend, PseudonymMap};
use uuid::Uuid;

use crate::{
    ActivitySink, AuthProfile, GatewayProtocol, NoopActivitySink, NoopUsageSink, UsageSink,
    local_key_digest,
};

pub struct RuntimeProfile {
    pub id: Uuid,
    pub protocol: GatewayProtocol,
    pub local_key: String,
    pub upstream_base_url: String,
    pub upstream_key: SecretString,
    pub policy: Policy,
    pub engine: PrivacyEngine,
    pub mappings: Mutex<PseudonymMap<Box<dyn MappingBackend + Send>>>,
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
        Self::with_mapping_backend(
            id,
            protocol,
            local_key,
            upstream_base_url,
            upstream_key,
            mapping_key,
            MemoryBackend::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_mapping_backend<B>(
        id: Uuid,
        protocol: GatewayProtocol,
        local_key: String,
        upstream_base_url: String,
        upstream_key: SecretString,
        mapping_key: [u8; 32],
        backend: B,
    ) -> Self
    where
        B: MappingBackend + Send + 'static,
    {
        Self::with_mapping_backend_and_dictionary(
            id,
            protocol,
            local_key,
            upstream_base_url,
            upstream_key,
            mapping_key,
            backend,
            StaticDictionary::empty(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_mapping_backend_and_dictionary<B>(
        id: Uuid,
        protocol: GatewayProtocol,
        local_key: String,
        upstream_base_url: String,
        upstream_key: SecretString,
        mapping_key: [u8; 32],
        backend: B,
        dictionary: StaticDictionary,
    ) -> Self
    where
        B: MappingBackend + Send + 'static,
    {
        Self {
            id,
            protocol,
            local_key,
            upstream_base_url,
            upstream_key,
            policy: Policy::strict(),
            engine: PrivacyEngine::new(
                vec![
                    Box::new(CredentialsDetector),
                    Box::new(ChinaIdDetector),
                    Box::new(UnifiedSocialCreditCodeDetector),
                    Box::new(BankCardDetector),
                    Box::new(PhoneDetector),
                    Box::new(NetworkDetector),
                ],
                dictionary,
            ),
            mappings: Mutex::new(PseudonymMap::new(id, mapping_key, Box::new(backend))),
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
    pub activity_sink: Arc<dyn ActivitySink>,
    pub usage_sink: Arc<dyn UsageSink>,
}
impl GatewayState {
    pub fn new(
        profiles: impl IntoIterator<Item = RuntimeProfile>,
    ) -> Result<Arc<Self>, reqwest::Error> {
        Self::new_with_sinks(
            profiles,
            Arc::new(NoopActivitySink),
            Arc::new(NoopUsageSink),
        )
    }

    pub fn new_with_sink(
        profiles: impl IntoIterator<Item = RuntimeProfile>,
        activity_sink: Arc<dyn ActivitySink>,
    ) -> Result<Arc<Self>, reqwest::Error> {
        Self::new_with_sinks(profiles, activity_sink, Arc::new(NoopUsageSink))
    }

    pub fn new_with_sinks(
        profiles: impl IntoIterator<Item = RuntimeProfile>,
        activity_sink: Arc<dyn ActivitySink>,
        usage_sink: Arc<dyn UsageSink>,
    ) -> Result<Arc<Self>, reqwest::Error> {
        let profiles = profiles
            .into_iter()
            .map(|profile| (profile.id, Arc::new(profile)))
            .collect();
        Ok(Arc::new(Self {
            client: crate::upstream_client()?,
            profiles,
            activity_sink,
            usage_sink,
        }))
    }
    pub fn auth_profiles(&self) -> Vec<AuthProfile> {
        self.profiles
            .values()
            .map(|profile| profile.auth_profile())
            .collect()
    }
}
