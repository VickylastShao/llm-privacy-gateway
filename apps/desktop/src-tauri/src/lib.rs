#![forbid(unsafe_code)]

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use activity_service::{ActivityQuery, ActivityService, UsageQuery};
use activity_store::{
    ProfileInput, Protocol, Repository, RepositoryMappingBackend, validate_upstream_url,
};
use gateway::{
    GatewayConfig, GatewayProtocol, GatewayState, RuntimeProfile, bind_listener, local_key_digest,
    serve_with_listener_and_shutdown,
};
use privacy_core::StaticDictionary;
use rand::{TryRngCore, rngs::OsRng};
use rusqlite::{Connection, MAIN_DB, OpenFlags};
use secrecy::{ExposeSecret, SecretString};
#[cfg(any(not(target_os = "windows"), test))]
use secret_store::MemorySecretStore;
use secret_store::{SecretKind, SecretStore, service_account};
use serde::{Deserialize, Serialize};
use tauri::{
    Manager, State, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tokio::sync::oneshot;
use uuid::Uuid;

const LOCAL_GATEWAY_PORT: u16 = 17_891;
const MAX_CUSTOM_TERMS: usize = 100;
const MAX_CUSTOM_TERM_CHARS: usize = 256;
const MAX_CUSTOM_DICTIONARY_BYTES: usize = 1_800;
const DISCOVERY_TTL_MS: i64 = 5 * 60 * 1_000;
const LEGACY_APP_IDENTIFIER: &str = "com.local.llmprivacygateway";
const PROFILE_DATABASE_NAME: &str = "profiles.sqlite";

#[derive(Debug, Clone, Serialize)]
struct GatewayStatus {
    running: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileSummary {
    id: String,
    name: String,
    protocol: String,
    upstream_url: String,
    policy: String,
    custom_term_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct CreatedProfile {
    profile: ProfileSummary,
    local_base_url: String,
    local_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileDraft {
    name: String,
    protocol: String,
    upstream_url: String,
    #[serde(default)]
    upstream_key: String,
    #[serde(default)]
    discovery_id: Option<String>,
    policy: String,
    #[serde(default)]
    custom_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct EditableProfile {
    id: String,
    name: String,
    provider: String,
    protocol: String,
    upstream_url: String,
    policy: String,
    custom_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DiscoveredProfile {
    id: String,
    provider: String,
    name: String,
    protocol: String,
    upstream_url: String,
    key_preview: String,
    source: String,
    key_variable: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url_variable: Option<String>,
}

#[derive(Debug, Clone)]
struct DiscoveredSecret {
    key: SecretString,
    protocol: String,
    upstream_url: String,
    expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
struct BalanceSummary {
    supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remaining: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    used: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    refreshed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct ConnectionTestSummary {
    success: bool,
    status: u16,
    latency_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct StartupIssue {
    profile_id: String,
    profile_name: String,
    reason: String,
}

#[derive(Debug)]
struct DesktopProfile {
    id: Uuid,
    summary: ProfileSummary,
    upstream_key: SecretString,
    local_key: String,
    mapping_key: [u8; 32],
    custom_terms: Vec<String>,
}

#[derive(Debug)]
struct RunningGateway {
    generation: u64,
    shutdown: oneshot::Sender<()>,
}

#[derive(Debug, Default)]
struct GatewayControl {
    next_generation: u64,
    starting: bool,
    running: Option<RunningGateway>,
}

struct GatewayStartupGuard {
    control: Arc<Mutex<GatewayControl>>,
    active: bool,
}

impl Drop for GatewayStartupGuard {
    fn drop(&mut self) {
        if self.active
            && let Ok(mut control) = self.control.lock()
        {
            control.starting = false;
        }
    }
}

struct DesktopState {
    gateway: Arc<Mutex<GatewayControl>>,
    profiles: Arc<Mutex<Vec<DesktopProfile>>>,
    startup_issues: Mutex<Vec<StartupIssue>>,
    repository: Arc<Mutex<Repository>>,
    activity: Arc<ActivityService>,
    secrets: Arc<dyn SecretStore>,
    discoveries: Arc<Mutex<HashMap<String, DiscoveredSecret>>>,
}

impl DesktopState {
    fn open(database_path: &std::path::Path) -> Result<Self, String> {
        let activity =
            ActivityService::open(database_path).map_err(|_| "activity database unavailable")?;
        let repository =
            Repository::open(database_path).map_err(|_| "profile database unavailable")?;
        let secrets = default_secret_store();
        let (profiles, startup_issues) = load_desktop_profiles(&repository, secrets.as_ref())?;
        Ok(Self {
            gateway: Arc::new(Mutex::new(GatewayControl::default())),
            profiles: Arc::new(Mutex::new(profiles)),
            startup_issues: Mutex::new(startup_issues),
            repository: Arc::new(Mutex::new(repository)),
            activity,
            secrets,
            discoveries: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

fn load_desktop_profiles(
    repository: &Repository,
    secrets: &dyn SecretStore,
) -> Result<(Vec<DesktopProfile>, Vec<StartupIssue>), String> {
    let mut profiles = Vec::new();
    let mut startup_issues = Vec::new();
    for stored in repository
        .profiles()
        .map_err(|_| "profile database unavailable")?
    {
        let upstream = secrets
            .get(SecretKind::Upstream, stored.id)
            .map_err(|_| "credential store unavailable")?;
        let local = secrets
            .get(SecretKind::Local, stored.id)
            .map_err(|_| "credential store unavailable")?;
        let mapping = secrets
            .get(SecretKind::Mapping, stored.id)
            .map_err(|_| "credential store unavailable")?;
        let dictionary = secrets
            .get(SecretKind::Dictionary, stored.id)
            .map_err(|_| "credential store unavailable")?;
        let (Some(upstream_key), Some(local_key), Some(mapping_key)) = (upstream, local, mapping)
        else {
            startup_issues.push(StartupIssue {
                profile_id: stored.id.to_string(),
                profile_name: stored.name,
                reason: "missing_credentials".to_owned(),
            });
            continue;
        };
        let Ok(mapping_key) = decode_mapping_key(mapping_key.expose_secret()) else {
            startup_issues.push(StartupIssue {
                profile_id: stored.id.to_string(),
                profile_name: stored.name,
                reason: "invalid_mapping_key".to_owned(),
            });
            continue;
        };
        let custom_terms = match dictionary {
            Some(value) => match decode_custom_terms(value.expose_secret()) {
                Ok(terms) => terms,
                Err(_) => {
                    startup_issues.push(StartupIssue {
                        profile_id: stored.id.to_string(),
                        profile_name: stored.name,
                        reason: "invalid_dictionary".to_owned(),
                    });
                    continue;
                }
            },
            None => Vec::new(),
        };
        profiles.push(DesktopProfile {
            id: stored.id,
            summary: ProfileSummary {
                id: stored.id.to_string(),
                name: stored.name,
                protocol: match stored.protocol {
                    Protocol::OpenAi => "openai".to_owned(),
                    Protocol::Anthropic => "anthropic".to_owned(),
                },
                upstream_url: stored.upstream_url,
                policy: stored.policy,
                custom_term_count: custom_terms.len(),
            },
            upstream_key,
            local_key: local_key.expose_secret().to_owned(),
            mapping_key,
            custom_terms,
        });
    }
    Ok((profiles, startup_issues))
}

fn default_secret_store() -> Arc<dyn SecretStore> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(secret_store::WindowsCredentialStore)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Arc::new(MemorySecretStore::default())
    }
}

#[tauri::command]
fn gateway_status(state: State<'_, DesktopState>) -> Result<GatewayStatus, String> {
    let control = state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?;
    Ok(GatewayStatus {
        running: control.starting || control.running.is_some(),
    })
}

#[tauri::command]
async fn start_gateway(state: State<'_, DesktopState>) -> Result<GatewayStatus, String> {
    let gateway_control = Arc::clone(&state.gateway);
    {
        let mut control = gateway_control
            .lock()
            .map_err(|_| "gateway state unavailable")?;
        if control.running.is_some() {
            return Ok(GatewayStatus { running: true });
        }
        if control.starting {
            return Err("gateway startup is already in progress".to_owned());
        }
        control.starting = true;
    }
    let mut startup_guard = GatewayStartupGuard {
        control: Arc::clone(&gateway_control),
        active: true,
    };
    let runtime_profiles = {
        let profiles = state
            .profiles
            .lock()
            .map_err(|_| "profile state unavailable")?;
        if profiles.is_empty() {
            return Err("create a profile before starting the gateway".to_owned());
        }
        let mut runtime_profiles = Vec::with_capacity(profiles.len());
        for profile in profiles.iter() {
            let protocol = protocol_from_str(&profile.summary.protocol)?;
            let dictionary = StaticDictionary::new(&profile.custom_terms)
                .map_err(|_| "custom dictionary unavailable")?;
            let mut runtime = RuntimeProfile::with_mapping_backend_and_dictionary(
                profile.id,
                protocol,
                profile.local_key.clone(),
                profile.summary.upstream_url.clone(),
                profile.upstream_key.clone(),
                profile.mapping_key,
                RepositoryMappingBackend::new(Arc::clone(&state.repository), profile.id),
                dictionary,
            );
            runtime.policy = policy_from_str(&profile.summary.policy)?;
            runtime_profiles.push(runtime);
        }
        runtime_profiles
    };
    let runtime = GatewayState::new_with_sinks(
        runtime_profiles,
        state.activity.clone(),
        state.activity.clone(),
    )
    .map_err(|_| "gateway setup failed")?;
    let config = GatewayConfig::new(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        LOCAL_GATEWAY_PORT,
    ))
    .map_err(|_| "gateway must use loopback")?;
    let listener = bind_listener(config)
        .await
        .map_err(|_| "local gateway port is unavailable")?;
    let (shutdown, receiver) = oneshot::channel();
    let mut control = state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?;
    control.starting = false;
    control.next_generation = control.next_generation.wrapping_add(1);
    let generation = control.next_generation;
    control.running = Some(RunningGateway {
        generation,
        shutdown,
    });
    startup_guard.active = false;
    drop(control);
    let control = Arc::clone(&state.gateway);
    tauri::async_runtime::spawn(async move {
        let _ = serve_with_listener_and_shutdown(listener, runtime, async move {
            let _ = receiver.await;
        })
        .await;
        if let Ok(mut control) = control.lock()
            && control
                .running
                .as_ref()
                .is_some_and(|running| running.generation == generation)
        {
            control.running = None;
        }
    });
    Ok(GatewayStatus { running: true })
}

#[tauri::command]
fn stop_gateway(state: State<'_, DesktopState>) -> Result<GatewayStatus, String> {
    let mut control = state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?;
    if control.starting {
        return Err("gateway startup is in progress".to_owned());
    }
    let shutdown = control.running.take().map(|running| running.shutdown);
    drop(control);
    if let Some(shutdown) = shutdown {
        let _ = shutdown.send(());
    }
    Ok(GatewayStatus { running: false })
}

#[tauri::command]
fn list_profiles(state: State<'_, DesktopState>) -> Result<Vec<ProfileSummary>, String> {
    Ok(state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?
        .iter()
        .map(|profile| profile.summary.clone())
        .collect())
}

#[tauri::command]
fn list_startup_issues(state: State<'_, DesktopState>) -> Result<Vec<StartupIssue>, String> {
    Ok(state
        .startup_issues
        .lock()
        .map_err(|_| "startup recovery state unavailable")?
        .clone())
}

#[tauri::command]
fn remove_unavailable_profile(id: String, state: State<'_, DesktopState>) -> Result<(), String> {
    remove_unavailable_profile_with_state(id, state.inner())
}

fn remove_unavailable_profile_with_state(id: String, state: &DesktopState) -> Result<(), String> {
    let gateway = state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?;
    if gateway.starting || gateway.running.is_some() {
        return Err("stop the gateway before changing profiles".to_owned());
    }
    let profile_id = Uuid::parse_str(&id).map_err(|_| "startup issue does not exist")?;
    let mut issues = state
        .startup_issues
        .lock()
        .map_err(|_| "startup recovery state unavailable")?;
    let issue_index = issues
        .iter()
        .position(|issue| issue.profile_id == id)
        .ok_or_else(|| "startup issue does not exist".to_owned())?;
    let mut existing_secrets = Vec::new();
    for kind in PROFILE_SECRET_KINDS {
        if let Some(value) = state
            .secrets
            .get(kind, profile_id)
            .map_err(|_| "credential store unavailable")?
        {
            existing_secrets.push((kind, value));
        }
    }
    if !delete_profile_secrets(state.secrets.as_ref(), profile_id) {
        let restored =
            restore_profile_secrets(state.secrets.as_ref(), profile_id, &existing_secrets);
        return Err(if restored {
            "credential removal failed".to_owned()
        } else {
            "credential removal failed and rollback was incomplete".to_owned()
        });
    }
    let deleted = state
        .repository
        .lock()
        .map_err(|_| "profile database unavailable".to_owned())
        .and_then(|repository| {
            repository
                .delete_profile(profile_id)
                .map_err(|_| "profile persistence failed".to_owned())
        });
    if !matches!(deleted, Ok(true)) {
        let restored =
            restore_profile_secrets(state.secrets.as_ref(), profile_id, &existing_secrets);
        return Err(if restored {
            "profile persistence failed".to_owned()
        } else {
            "profile persistence failed and credential rollback was incomplete".to_owned()
        });
    }
    issues.remove(issue_index);
    drop(gateway);
    Ok(())
}

#[tauri::command]
fn get_profile_for_edit(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<EditableProfile, String> {
    let profiles = state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?;
    let profile = profiles
        .iter()
        .find(|profile| profile.summary.id == id)
        .ok_or_else(|| "profile does not exist".to_owned())?;
    let (provider, _) = infer_provider(
        &profile.summary.upstream_url,
        &profile.summary.protocol,
        "Imported connection",
    );
    Ok(EditableProfile {
        id,
        name: profile.summary.name.clone(),
        provider: provider.to_owned(),
        protocol: profile.summary.protocol.clone(),
        upstream_url: profile.summary.upstream_url.clone(),
        policy: profile.summary.policy.clone(),
        custom_terms: profile.custom_terms.clone(),
    })
}

#[tauri::command]
fn create_profile(
    draft: ProfileDraft,
    state: State<'_, DesktopState>,
) -> Result<CreatedProfile, String> {
    create_profile_with_state(draft, state.inner())
}

fn create_profile_with_state(
    mut draft: ProfileDraft,
    state: &DesktopState,
) -> Result<CreatedProfile, String> {
    let discovery_id = draft.discovery_id.take();
    if draft.upstream_key.trim().is_empty()
        && let Some(discovery_id) = discovery_id.as_deref()
    {
        let mut discoveries = state
            .discoveries
            .lock()
            .map_err(|_| "environment discovery unavailable")?;
        purge_expired_discoveries(&mut discoveries, current_time_ms());
        let discovered = discoveries
            .get(discovery_id)
            .cloned()
            .ok_or_else(|| "discovered credential expired; scan again".to_owned())?;
        if draft.protocol != discovered.protocol || draft.upstream_url != discovered.upstream_url {
            return Err(
                "imported credential is bound to its discovered protocol and URL; enter the key manually to change them"
                    .to_owned(),
            );
        }
        draft.upstream_key = discovered.key.expose_secret().to_owned();
    }
    if draft.name.trim().is_empty() || draft.upstream_key.trim().is_empty() {
        return Err("profile name and upstream API key are required".to_owned());
    }
    let gateway_protocol = protocol_from_str(&draft.protocol)?;
    policy_from_str(&draft.policy)?;
    validate_upstream_url(&draft.upstream_url)
        .map_err(|_| "upstream URL must be HTTPS or loopback HTTP".to_owned())?;
    let _gateway_lifecycle = state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?;
    if _gateway_lifecycle.starting || _gateway_lifecycle.running.is_some() {
        return Err("stop the gateway before changing profiles".to_owned());
    }
    let id = Uuid::new_v4();
    let local_key = format!("lpg_local_{id}");
    let mut mapping_key = [0_u8; 32];
    OsRng
        .try_fill_bytes(&mut mapping_key)
        .map_err(|_| "secure random generation failed")?;
    let local_base_url = local_base_url(&gateway_protocol);
    let custom_terms = normalize_custom_terms(draft.custom_terms)?;
    let summary = ProfileSummary {
        id: id.to_string(),
        name: draft.name,
        protocol: draft.protocol,
        upstream_url: draft.upstream_url,
        policy: draft.policy,
        custom_term_count: custom_terms.len(),
    };
    let mapping_secret = encode_mapping_key(mapping_key);
    let secrets = profile_secret_values(
        &draft.upstream_key,
        &local_key,
        &mapping_secret,
        &encode_custom_terms(&custom_terms)?,
    );
    let mut profiles = state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?;
    let repository = state
        .repository
        .lock()
        .map_err(|_| "profile database unavailable")?;
    put_new_profile_secrets(state.secrets.as_ref(), id, &secrets)?;
    let protocol = match gateway_protocol {
        GatewayProtocol::OpenAi => Protocol::OpenAi,
        GatewayProtocol::Anthropic => Protocol::Anthropic,
    };
    let (_, upstream_secret_ref) = service_account(SecretKind::Upstream, id);
    let (_, local_secret_ref) = service_account(SecretKind::Local, id);
    if repository
        .save_profile(&ProfileInput {
            id,
            name: summary.name.clone(),
            protocol,
            upstream_url: summary.upstream_url.clone(),
            upstream_secret_ref,
            local_secret_ref,
            local_key_digest: local_key_digest(&local_key),
            policy: summary.policy.clone(),
        })
        .is_err()
    {
        let cleaned = delete_profile_secrets(state.secrets.as_ref(), id);
        return Err(if cleaned {
            "profile persistence failed".to_owned()
        } else {
            "profile persistence failed and credential cleanup was incomplete".to_owned()
        });
    }
    profiles.push(DesktopProfile {
        id,
        summary: summary.clone(),
        upstream_key: SecretString::from(draft.upstream_key),
        local_key: local_key.clone(),
        mapping_key,
        custom_terms,
    });
    if let Ok(mut discoveries) = state.discoveries.lock() {
        discoveries.clear();
    }
    Ok(CreatedProfile {
        profile: summary,
        local_base_url,
        local_key,
    })
}

#[tauri::command]
fn update_profile(
    id: String,
    draft: ProfileDraft,
    state: State<'_, DesktopState>,
) -> Result<ProfileSummary, String> {
    update_profile_with_state(id, draft, state.inner())
}

fn update_profile_with_state(
    id: String,
    draft: ProfileDraft,
    state: &DesktopState,
) -> Result<ProfileSummary, String> {
    if draft.name.trim().is_empty() {
        return Err("profile name is required".to_owned());
    }
    let gateway_protocol = protocol_from_str(&draft.protocol)?;
    policy_from_str(&draft.policy)?;
    validate_upstream_url(&draft.upstream_url)
        .map_err(|_| "upstream URL must be HTTPS or loopback HTTP".to_owned())?;
    let _gateway_lifecycle = state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?;
    if _gateway_lifecycle.starting || _gateway_lifecycle.running.is_some() {
        return Err("stop the gateway before changing profiles".to_owned());
    }
    let id = Uuid::parse_str(&id).map_err(|_| "profile does not exist")?;
    let custom_terms = normalize_custom_terms(draft.custom_terms)?;
    let encoded_terms = encode_custom_terms(&custom_terms)?;
    let mut profiles = state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.id == id)
        .ok_or_else(|| "profile does not exist".to_owned())?;
    if !same_upstream_origin(&profile.summary.upstream_url, &draft.upstream_url)
        && draft.upstream_key.trim().is_empty()
    {
        return Err("new upstream API key required when changing upstream origin".to_owned());
    }
    let old_upstream_key = profile.upstream_key.clone();
    let old_dictionary = encode_custom_terms(&profile.custom_terms)?;
    let upstream_key = if draft.upstream_key.trim().is_empty() {
        old_upstream_key.clone()
    } else {
        SecretString::from(draft.upstream_key)
    };
    let old_values = [
        (SecretKind::Upstream, old_upstream_key),
        (SecretKind::Dictionary, SecretString::from(old_dictionary)),
    ];
    let new_values = [
        (SecretKind::Upstream, upstream_key.clone()),
        (SecretKind::Dictionary, SecretString::from(encoded_terms)),
    ];
    if !put_replacement_secrets(state.secrets.as_ref(), id, &new_values) {
        let restored = restore_profile_secrets(state.secrets.as_ref(), id, &old_values);
        return Err(if restored {
            "credential storage failed".to_owned()
        } else {
            "credential storage failed and rollback was incomplete".to_owned()
        });
    }
    let summary = ProfileSummary {
        id: id.to_string(),
        name: draft.name,
        protocol: draft.protocol,
        upstream_url: draft.upstream_url,
        policy: draft.policy,
        custom_term_count: custom_terms.len(),
    };
    let protocol = match gateway_protocol {
        GatewayProtocol::OpenAi => Protocol::OpenAi,
        GatewayProtocol::Anthropic => Protocol::Anthropic,
    };
    let (_, upstream_secret_ref) = service_account(SecretKind::Upstream, id);
    let (_, local_secret_ref) = service_account(SecretKind::Local, id);
    let persisted = state
        .repository
        .lock()
        .map_err(|_| "profile database unavailable".to_owned())
        .and_then(|repository| {
            repository
                .save_profile(&ProfileInput {
                    id,
                    name: summary.name.clone(),
                    protocol,
                    upstream_url: summary.upstream_url.clone(),
                    upstream_secret_ref,
                    local_secret_ref,
                    local_key_digest: local_key_digest(&profile.local_key),
                    policy: summary.policy.clone(),
                })
                .map_err(|_| "profile persistence failed".to_owned())
        });
    if persisted.is_err() {
        let restored = restore_profile_secrets(state.secrets.as_ref(), id, &old_values);
        return Err(if restored {
            "profile persistence failed".to_owned()
        } else {
            "profile persistence failed and credential rollback was incomplete".to_owned()
        });
    }
    profile.summary.clone_from(&summary);
    profile.upstream_key = upstream_key;
    profile.custom_terms = custom_terms;
    Ok(summary)
}

#[derive(Clone, Copy)]
struct EnvironmentPreset {
    provider: &'static str,
    name: &'static str,
    protocol: &'static str,
    key_variables: &'static [&'static str],
    url_variables: &'static [&'static str],
    default_url: &'static str,
}

const ENVIRONMENT_PRESETS: &[EnvironmentPreset] = &[
    EnvironmentPreset {
        provider: "openai",
        name: "OpenAI",
        protocol: "openai",
        key_variables: &["OPENAI_API_KEY"],
        url_variables: &["OPENAI_BASE_URL", "OPENAI_API_BASE"],
        default_url: "https://api.openai.com/v1",
    },
    EnvironmentPreset {
        provider: "anthropic",
        name: "Anthropic",
        protocol: "anthropic",
        key_variables: &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"],
        url_variables: &["ANTHROPIC_BASE_URL"],
        default_url: "https://api.anthropic.com",
    },
    EnvironmentPreset {
        provider: "deepseek",
        name: "DeepSeek",
        protocol: "openai",
        key_variables: &["DEEPSEEK_API_KEY"],
        url_variables: &["DEEPSEEK_BASE_URL"],
        default_url: "https://api.deepseek.com",
    },
    EnvironmentPreset {
        provider: "gemini",
        name: "Google Gemini",
        protocol: "openai",
        key_variables: &["GEMINI_API_KEY"],
        url_variables: &["GEMINI_BASE_URL"],
        default_url: "https://generativelanguage.googleapis.com/v1beta/openai/",
    },
    EnvironmentPreset {
        provider: "openrouter",
        name: "OpenRouter",
        protocol: "openai",
        key_variables: &["OPENROUTER_API_KEY"],
        url_variables: &["OPENROUTER_BASE_URL"],
        default_url: "https://openrouter.ai/api/v1",
    },
    EnvironmentPreset {
        provider: "groq",
        name: "Groq",
        protocol: "openai",
        key_variables: &["GROQ_API_KEY"],
        url_variables: &["GROQ_BASE_URL"],
        default_url: "https://api.groq.com/openai/v1",
    },
    EnvironmentPreset {
        provider: "xai",
        name: "xAI",
        protocol: "openai",
        key_variables: &["XAI_API_KEY"],
        url_variables: &["XAI_BASE_URL"],
        default_url: "https://api.x.ai/v1",
    },
    EnvironmentPreset {
        provider: "mistral",
        name: "Mistral AI",
        protocol: "openai",
        key_variables: &["MISTRAL_API_KEY"],
        url_variables: &["MISTRAL_BASE_URL"],
        default_url: "https://api.mistral.ai/v1",
    },
    EnvironmentPreset {
        provider: "kimi",
        name: "Kimi",
        protocol: "openai",
        key_variables: &["MOONSHOT_API_KEY"],
        url_variables: &["MOONSHOT_BASE_URL"],
        default_url: "https://api.moonshot.cn/v1",
    },
    EnvironmentPreset {
        provider: "dashscope",
        name: "Alibaba Model Studio",
        protocol: "openai",
        key_variables: &["DASHSCOPE_API_KEY"],
        url_variables: &["DASHSCOPE_BASE_URL"],
        default_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    },
    EnvironmentPreset {
        provider: "siliconflow",
        name: "SiliconFlow",
        protocol: "openai",
        key_variables: &["SILICONFLOW_API_KEY"],
        url_variables: &["SILICONFLOW_BASE_URL"],
        default_url: "https://api.siliconflow.cn/v1",
    },
];

#[derive(Debug, Clone)]
struct EnvironmentValue {
    value: String,
    source: &'static str,
}

#[tauri::command]
fn scan_environment_profiles(
    state: State<'_, DesktopState>,
) -> Result<Vec<DiscoveredProfile>, String> {
    let mut candidates = Vec::new();
    let mut secrets = HashMap::new();
    for preset in ENVIRONMENT_PRESETS {
        let Some((key_variable, key)) = preset
            .key_variables
            .iter()
            .find_map(|name| environment_value(name).map(|value| (*name, value)))
        else {
            continue;
        };
        if key.value.trim().is_empty() {
            continue;
        }
        let url_value = preset
            .url_variables
            .iter()
            .find_map(|name| environment_value(name).map(|value| (*name, value)));
        let upstream_url = url_value.as_ref().map_or_else(
            || preset.default_url.to_owned(),
            |(_, value)| value.value.clone(),
        );
        if validate_upstream_url(&upstream_url).is_err() {
            continue;
        }
        let (provider, name) = infer_provider(&upstream_url, preset.provider, preset.name);
        let id = Uuid::new_v4().to_string();
        candidates.push(DiscoveredProfile {
            id: id.clone(),
            provider: provider.to_owned(),
            name: name.to_owned(),
            protocol: preset.protocol.to_owned(),
            upstream_url: upstream_url.clone(),
            key_preview: mask_secret(&key.value),
            source: key.source.to_owned(),
            key_variable: key_variable.to_owned(),
            url_variable: url_value.map(|(name, _)| name.to_owned()),
        });
        secrets.insert(
            id,
            DiscoveredSecret {
                key: SecretString::from(key.value),
                protocol: preset.protocol.to_owned(),
                upstream_url: upstream_url.clone(),
                expires_at_ms: current_time_ms().saturating_add(DISCOVERY_TTL_MS),
            },
        );
    }
    *state
        .discoveries
        .lock()
        .map_err(|_| "environment discovery unavailable")? = secrets;
    let discoveries = Arc::clone(&state.discoveries);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(
            u64::try_from(DISCOVERY_TTL_MS).unwrap_or(300_000),
        ))
        .await;
        if let Ok(mut discoveries) = discoveries.lock() {
            purge_expired_discoveries(&mut discoveries, current_time_ms());
        }
    });
    Ok(candidates)
}

fn purge_expired_discoveries(discoveries: &mut HashMap<String, DiscoveredSecret>, now_ms: i64) {
    discoveries.retain(|_, secret| secret.expires_at_ms > now_ms);
}

#[tauri::command]
fn clear_environment_discoveries(state: State<'_, DesktopState>) -> Result<(), String> {
    state
        .discoveries
        .lock()
        .map_err(|_| "environment discovery unavailable")?
        .clear();
    Ok(())
}

fn environment_value(name: &str) -> Option<EnvironmentValue> {
    std::env::var(name)
        .ok()
        .and_then(|value| non_empty_environment_value(value, "process"))
        .or_else(|| persistent_environment_value(name))
}

fn non_empty_environment_value(value: String, source: &'static str) -> Option<EnvironmentValue> {
    (!value.trim().is_empty()).then_some(EnvironmentValue { value, source })
}

#[cfg(target_os = "windows")]
fn persistent_environment_value(name: &str) -> Option<EnvironmentValue> {
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
    };
    let current_user = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Environment")
        .ok()
        .and_then(|key| key.get_value::<String, _>(name).ok())
        .and_then(|value| non_empty_environment_value(value, "user"));
    if current_user.is_some() {
        return current_user;
    }
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment")
        .ok()
        .and_then(|key| key.get_value::<String, _>(name).ok())
        .and_then(|value| non_empty_environment_value(value, "system"))
}

#[cfg(not(target_os = "windows"))]
fn persistent_environment_value(_name: &str) -> Option<EnvironmentValue> {
    None
}

fn infer_provider<'a>(url: &str, fallback: &'a str, fallback_name: &'a str) -> (&'a str, &'a str) {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|value| value.host_str().map(str::to_owned))
        .unwrap_or_default();
    match host.as_str() {
        "api.openai.com" => ("openai", "OpenAI"),
        "api.anthropic.com" => ("anthropic", "Anthropic"),
        "api.deepseek.com" => ("deepseek", "DeepSeek"),
        "generativelanguage.googleapis.com" => ("gemini", "Google Gemini"),
        "openrouter.ai" => ("openrouter", "OpenRouter"),
        "api.groq.com" => ("groq", "Groq"),
        "api.x.ai" => ("xai", "xAI"),
        "api.mistral.ai" => ("mistral", "Mistral AI"),
        "api.moonshot.cn" => ("kimi", "Kimi"),
        "dashscope.aliyuncs.com" => ("dashscope", "Alibaba Model Studio"),
        "api.siliconflow.cn" => ("siliconflow", "SiliconFlow"),
        _ if matches!(fallback, "openai" | "anthropic") => ("custom", "Imported connection"),
        _ => (fallback, fallback_name),
    }
}

fn mask_secret(value: &str) -> String {
    let prefix = value.chars().take(3).collect::<String>();
    let suffix = value
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if value.chars().count() <= 8 {
        "••••••••".to_owned()
    } else {
        format!("{prefix}••••••{suffix}")
    }
}

fn same_upstream_origin(left: &str, right: &str) -> bool {
    let Ok(left) = url::Url::parse(left) else {
        return false;
    };
    let Ok(right) = url::Url::parse(right) else {
        return false;
    };
    left.origin() == right.origin()
}

#[tauri::command]
fn delete_profile(id: String, state: State<'_, DesktopState>) -> Result<(), String> {
    delete_profile_with_state(id, state.inner())
}

fn delete_profile_with_state(id: String, state: &DesktopState) -> Result<(), String> {
    let _gateway_lifecycle = state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?;
    if _gateway_lifecycle.starting || _gateway_lifecycle.running.is_some() {
        return Err("stop the gateway before changing profiles".to_owned());
    }
    let profiles = &mut *state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?;
    let index = profiles
        .iter()
        .position(|profile| profile.summary.id == id)
        .ok_or_else(|| "profile does not exist".to_owned())?;
    let id = Uuid::parse_str(&id).map_err(|_| "profile does not exist")?;
    let profile = &profiles[index];
    let secret_values = profile_secret_values(
        profile.upstream_key.expose_secret(),
        &profile.local_key,
        &encode_mapping_key(profile.mapping_key),
        &encode_custom_terms(&profile.custom_terms)?,
    );
    if !delete_profile_secrets(state.secrets.as_ref(), id) {
        let restored = restore_profile_secrets(state.secrets.as_ref(), id, &secret_values);
        return Err(if restored {
            "credential removal failed".to_owned()
        } else {
            "credential removal failed and rollback was incomplete".to_owned()
        });
    }
    let deleted = state
        .repository
        .lock()
        .map_err(|_| "profile database unavailable".to_owned())
        .and_then(|repository| {
            repository
                .delete_profile(id)
                .map_err(|_| "profile persistence failed".to_owned())
        });
    if !matches!(deleted, Ok(true)) {
        let restored = restore_profile_secrets(state.secrets.as_ref(), id, &secret_values);
        return Err(if restored {
            "profile persistence failed".to_owned()
        } else {
            "profile persistence failed and credential rollback was incomplete".to_owned()
        });
    }
    profiles.remove(index);
    Ok(())
}

#[tauri::command]
fn rotate_local_key(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<serde_json::Value, String> {
    rotate_local_key_with_state(id, state.inner())
}

fn rotate_local_key_with_state(
    id: String,
    state: &DesktopState,
) -> Result<serde_json::Value, String> {
    let _gateway_lifecycle = state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?;
    if _gateway_lifecycle.starting || _gateway_lifecycle.running.is_some() {
        return Err("stop the gateway before rotating a local key".to_owned());
    }
    let mut profiles = state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.summary.id == id)
        .ok_or_else(|| "profile does not exist".to_owned())?;
    let old_local_key = profile.local_key.clone();
    let new_local_key = format!("lpg_local_{}", Uuid::new_v4());
    state
        .secrets
        .put(
            SecretKind::Local,
            profile.id,
            SecretString::from(new_local_key.clone()),
        )
        .map_err(|_| "credential storage failed")?;
    let protocol = match protocol_from_str(&profile.summary.protocol)? {
        GatewayProtocol::OpenAi => Protocol::OpenAi,
        GatewayProtocol::Anthropic => Protocol::Anthropic,
    };
    let (_, upstream_secret_ref) = service_account(SecretKind::Upstream, profile.id);
    let (_, local_secret_ref) = service_account(SecretKind::Local, profile.id);
    let persisted = state
        .repository
        .lock()
        .map_err(|_| "profile database unavailable".to_owned())
        .and_then(|repository| {
            repository
                .save_profile(&ProfileInput {
                    id: profile.id,
                    name: profile.summary.name.clone(),
                    protocol,
                    upstream_url: profile.summary.upstream_url.clone(),
                    upstream_secret_ref,
                    local_secret_ref,
                    local_key_digest: local_key_digest(&new_local_key),
                    policy: profile.summary.policy.clone(),
                })
                .map_err(|_| "profile persistence failed".to_owned())
        });
    if persisted.is_err() {
        let restored = state
            .secrets
            .put(
                SecretKind::Local,
                profile.id,
                SecretString::from(old_local_key),
            )
            .is_ok();
        return Err(if restored {
            "profile persistence failed".to_owned()
        } else {
            "profile persistence failed and local-key rollback was incomplete".to_owned()
        });
    }
    profile.local_key.clone_from(&new_local_key);
    Ok(serde_json::json!({"profile_id": id, "local_key": new_local_key}))
}

const PROFILE_SECRET_KINDS: [SecretKind; 4] = [
    SecretKind::Upstream,
    SecretKind::Local,
    SecretKind::Mapping,
    SecretKind::Dictionary,
];

fn profile_secret_values(
    upstream: &str,
    local: &str,
    mapping: &str,
    dictionary: &str,
) -> [(SecretKind, SecretString); 4] {
    [
        (
            SecretKind::Upstream,
            SecretString::from(upstream.to_owned()),
        ),
        (SecretKind::Local, SecretString::from(local.to_owned())),
        (SecretKind::Mapping, SecretString::from(mapping.to_owned())),
        (
            SecretKind::Dictionary,
            SecretString::from(dictionary.to_owned()),
        ),
    ]
}

fn put_new_profile_secrets(
    store: &dyn SecretStore,
    profile_id: Uuid,
    values: &[(SecretKind, SecretString)],
) -> Result<(), String> {
    let mut written = Vec::new();
    for (kind, value) in values {
        if store.put(*kind, profile_id, value.clone()).is_err() {
            let cleaned = written
                .into_iter()
                .map(|written_kind| store.delete(written_kind, profile_id).is_ok())
                .fold(true, |all_ok, deleted| all_ok & deleted);
            return Err(if cleaned {
                "credential storage failed".to_owned()
            } else {
                "credential storage failed and cleanup was incomplete".to_owned()
            });
        }
        written.push(*kind);
    }
    Ok(())
}

fn delete_profile_secrets(store: &dyn SecretStore, profile_id: Uuid) -> bool {
    PROFILE_SECRET_KINDS
        .into_iter()
        .map(|kind| store.delete(kind, profile_id).is_ok())
        .fold(true, |all_ok, deleted| all_ok & deleted)
}

fn restore_profile_secrets(
    store: &dyn SecretStore,
    profile_id: Uuid,
    values: &[(SecretKind, SecretString)],
) -> bool {
    values
        .iter()
        .map(|(kind, value)| store.put(*kind, profile_id, value.clone()).is_ok())
        .fold(true, |all_ok, restored| all_ok & restored)
}

fn put_replacement_secrets(
    store: &dyn SecretStore,
    profile_id: Uuid,
    values: &[(SecretKind, SecretString)],
) -> bool {
    values
        .iter()
        .map(|(kind, value)| store.put(*kind, profile_id, value.clone()).is_ok())
        .fold(true, |all_ok, stored| all_ok & stored)
}

#[tauri::command]
fn query_activity(limit: usize, state: State<'_, DesktopState>) -> Result<ActivityQuery, String> {
    state
        .activity
        .query(limit, current_time_ms())
        .map_err(|_| "activity database unavailable".to_owned())
}

#[tauri::command]
fn query_usage(state: State<'_, DesktopState>) -> Result<UsageQuery, String> {
    state
        .activity
        .query_usage(current_time_ms())
        .map_err(|_| "usage database unavailable".to_owned())
}

#[tauri::command]
async fn query_balance(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<BalanceSummary, String> {
    let (upstream_url, upstream_key) = {
        let profiles = state
            .profiles
            .lock()
            .map_err(|_| "profile state unavailable")?;
        let profile = profiles
            .iter()
            .find(|profile| profile.summary.id == id)
            .ok_or_else(|| "profile does not exist".to_owned())?;
        (
            profile.summary.upstream_url.clone(),
            profile.upstream_key.expose_secret().to_owned(),
        )
    };
    query_provider_balance(&upstream_url, &upstream_key).await
}

#[tauri::command]
async fn test_profile_connection(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<ConnectionTestSummary, String> {
    let (upstream_url, upstream_key, protocol) = {
        let profiles = state
            .profiles
            .lock()
            .map_err(|_| "profile state unavailable")?;
        let profile = profiles
            .iter()
            .find(|profile| profile.summary.id == id)
            .ok_or_else(|| "profile does not exist".to_owned())?;
        (
            profile.summary.upstream_url.clone(),
            profile.upstream_key.expose_secret().to_owned(),
            protocol_from_str(&profile.summary.protocol)?,
        )
    };
    test_upstream_connection(&upstream_url, &upstream_key, protocol).await
}

async fn test_upstream_connection(
    upstream_url: &str,
    upstream_key: &str,
    protocol: GatewayProtocol,
) -> Result<ConnectionTestSummary, String> {
    validate_upstream_url(upstream_url)
        .map_err(|_| "upstream URL must be HTTPS or loopback HTTP")?;
    let path = match protocol {
        GatewayProtocol::OpenAi => "/models",
        GatewayProtocol::Anthropic => "/v1/models",
    };
    let endpoint = format!("{}{}", upstream_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "connection test client unavailable")?;
    let request = client.get(endpoint).header("accept", "application/json");
    let request = match protocol {
        GatewayProtocol::OpenAi => request.bearer_auth(upstream_key),
        GatewayProtocol::Anthropic => request
            .header("x-api-key", upstream_key)
            .header("anthropic-version", "2023-06-01"),
    };
    let started = std::time::Instant::now();
    let response = request
        .send()
        .await
        .map_err(|_| "upstream connection failed")?;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    Ok(ConnectionTestSummary {
        success: response.status().is_success(),
        status: response.status().as_u16(),
        latency_ms,
    })
}

async fn query_provider_balance(
    upstream_url: &str,
    upstream_key: &str,
) -> Result<BalanceSummary, String> {
    let url = url::Url::parse(upstream_url).map_err(|_| "invalid upstream URL")?;
    let host = url.host_str().unwrap_or_default();
    let (provider, endpoint) = match host {
        "api.deepseek.com" => ("DeepSeek", "https://api.deepseek.com/user/balance"),
        "api.moonshot.cn" => ("Kimi", "https://api.moonshot.cn/v1/users/me/balance"),
        "api.siliconflow.cn" => ("SiliconFlow", "https://api.siliconflow.cn/v1/user/info"),
        "openrouter.ai" => ("OpenRouter", "https://openrouter.ai/api/v1/key"),
        _ => {
            return Ok(BalanceSummary {
                supported: false,
                provider: None,
                remaining: None,
                total: None,
                used: None,
                unit: None,
                detail: Some(
                    "provider does not expose a balance endpoint for regular API keys".to_owned(),
                ),
                refreshed_at_ms: None,
            });
        }
    };
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "balance client unavailable")?
        .get(endpoint)
        .bearer_auth(upstream_key)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|_| "balance endpoint unavailable")?;
    if !response.status().is_success() {
        return Err(format!(
            "balance query failed with HTTP {}",
            response.status()
        ));
    }
    let body = response
        .json::<serde_json::Value>()
        .await
        .map_err(|_| "balance response was invalid")?;
    parse_balance_response(provider, &body)
}

fn parse_balance_response(
    provider: &str,
    body: &serde_json::Value,
) -> Result<BalanceSummary, String> {
    let (remaining, total, used, unit, detail) = match provider {
        "DeepSeek" => {
            let info = body
                .get("balance_infos")
                .and_then(serde_json::Value::as_array)
                .and_then(|items| items.first())
                .ok_or_else(|| "balance response was invalid".to_owned())?;
            (
                json_number(info, "total_balance")
                    .ok_or_else(|| "balance response was invalid".to_owned())?
                    .into(),
                None,
                None,
                info.get("currency")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                None,
            )
        }
        "Kimi" => {
            let data = body.get("data").unwrap_or(body);
            (
                json_number(data, "available_balance")
                    .ok_or_else(|| "balance response was invalid".to_owned())?
                    .into(),
                None,
                None,
                Some("CNY".to_owned()),
                None,
            )
        }
        "SiliconFlow" => {
            let data = body.get("data").unwrap_or(body);
            (
                json_number(data, "totalBalance")
                    .ok_or_else(|| "balance response was invalid".to_owned())?
                    .into(),
                None,
                None,
                Some("CNY".to_owned()),
                None,
            )
        }
        "OpenRouter" => {
            let data = body.get("data").unwrap_or(body);
            let remaining = json_number(data, "limit_remaining");
            let total = json_number(data, "limit");
            let used = json_number(data, "usage")
                .ok_or_else(|| "balance response was invalid".to_owned())?
                .into();
            let detail = if total.is_none() {
                Some(
                    "This API key has no configured spending limit; showing recorded usage only."
                        .to_owned(),
                )
            } else {
                None
            };
            (remaining, total, used, Some("USD".to_owned()), detail)
        }
        _ => return Err("balance provider is unsupported".to_owned()),
    };
    Ok(BalanceSummary {
        supported: true,
        provider: Some(provider.to_owned()),
        remaining,
        total,
        used,
        unit,
        detail,
        refreshed_at_ms: Some(current_time_ms()),
    })
}

fn json_number(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|field| {
        field
            .as_f64()
            .or_else(|| field.as_str().and_then(|text| text.parse::<f64>().ok()))
    })
}

fn protocol_from_str(value: &str) -> Result<GatewayProtocol, String> {
    match value {
        "openai" => Ok(GatewayProtocol::OpenAi),
        "anthropic" => Ok(GatewayProtocol::Anthropic),
        _ => Err("unsupported protocol".to_owned()),
    }
}

fn policy_from_str(value: &str) -> Result<privacy_core::Policy, String> {
    match value {
        "strict" => Ok(privacy_core::Policy::strict()),
        "balanced" => Ok(privacy_core::Policy::balanced()),
        "monitor" => Ok(privacy_core::Policy::monitor()),
        _ => Err("unsupported policy".to_owned()),
    }
}

fn local_base_url(protocol: &GatewayProtocol) -> String {
    match protocol {
        GatewayProtocol::OpenAi => format!("http://127.0.0.1:{LOCAL_GATEWAY_PORT}/v1"),
        GatewayProtocol::Anthropic => format!("http://127.0.0.1:{LOCAL_GATEWAY_PORT}"),
    }
}

fn current_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn encode_mapping_key(key: [u8; 32]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn decode_mapping_key(value: &str) -> Result<[u8; 32], ()> {
    if value.len() != 64 {
        return Err(());
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| ())?;
    }
    Ok(output)
}

fn normalize_custom_terms(terms: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for term in terms {
        let term = term.trim();
        if term.is_empty() || normalized.iter().any(|existing| existing == term) {
            continue;
        }
        if term.chars().count() > MAX_CUSTOM_TERM_CHARS {
            return Err("custom dictionary is too large".to_owned());
        }
        normalized.push(term.to_owned());
    }
    if normalized.len() > MAX_CUSTOM_TERMS {
        return Err("custom dictionary is too large".to_owned());
    }
    let encoded = encode_custom_terms(&normalized)?;
    if encoded.len() > MAX_CUSTOM_DICTIONARY_BYTES {
        return Err("custom dictionary is too large".to_owned());
    }
    Ok(normalized)
}

fn encode_custom_terms(terms: &[String]) -> Result<String, String> {
    serde_json::to_string(terms).map_err(|_| "custom dictionary unavailable".to_owned())
}

fn decode_custom_terms(value: &str) -> Result<Vec<String>, String> {
    let terms = serde_json::from_str(value).map_err(|_| "custom dictionary unavailable")?;
    normalize_custom_terms(terms)
}

fn migrate_legacy_database(
    legacy_directory: &Path,
    current_directory: &Path,
) -> Result<bool, String> {
    if legacy_directory == current_directory {
        return Ok(false);
    }
    let source = legacy_directory.join(PROFILE_DATABASE_NAME);
    let destination = current_directory.join(PROFILE_DATABASE_NAME);
    if destination.exists() || !source.exists() {
        return Ok(false);
    }

    std::fs::create_dir_all(current_directory)
        .map_err(|_| "application data migration directory unavailable".to_owned())?;
    let temporary = current_directory.join(".profiles.sqlite.migrating");
    if temporary.exists() {
        std::fs::remove_file(&temporary)
            .map_err(|_| "stale application data migration could not be cleared".to_owned())?;
    }

    let migration = (|| {
        let source_connection = Connection::open_with_flags(
            &source,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| "legacy application database could not be opened".to_owned())?;
        source_connection
            .backup(MAIN_DB, &temporary, None)
            .map_err(|_| "legacy application database could not be copied".to_owned())?;
        drop(source_connection);

        let copied = Connection::open_with_flags(
            &temporary,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| "migrated application database could not be opened".to_owned())?;
        let integrity = copied
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .map_err(|_| "migrated application database could not be verified".to_owned())?;
        drop(copied);
        if integrity != "ok" {
            return Err("migrated application database failed integrity verification".to_owned());
        }
        std::fs::OpenOptions::new()
            .write(true)
            .open(&temporary)
            .and_then(|file| file.sync_all())
            .map_err(|_| "migrated application database could not be synchronized".to_owned())?;
        std::fs::rename(&temporary, &destination)
            .map_err(|_| "migrated application database could not be activated".to_owned())?;
        Ok(true)
    })();
    if migration.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    migration
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_root = app
                .path()
                .data_dir()
                .map_err(|_| std::io::Error::other("application data root unavailable"))?;
            let directory = app
                .path()
                .app_data_dir()
                .map_err(|_| std::io::Error::other("application data directory unavailable"))?;
            migrate_legacy_database(&data_root.join(LEGACY_APP_IDENTIFIER), &directory)
                .map_err(std::io::Error::other)?;
            std::fs::create_dir_all(&directory)?;
            app.manage(
                DesktopState::open(&directory.join("profiles.sqlite"))
                    .map_err(std::io::Error::other)?,
            );
            let show = MenuItem::with_id(app, "show", "显示 / Show", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "退出 / Exit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &exit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .tooltip("LLM Privacy Gateway")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "exit" => {
                        let _ = stop_gateway(app.state());
                        app.exit(0);
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let gateway_active = window
                    .app_handle()
                    .state::<DesktopState>()
                    .gateway
                    .lock()
                    .is_ok_and(|control| control.starting || control.running.is_some());
                if gateway_active {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            gateway_status,
            start_gateway,
            stop_gateway,
            list_profiles,
            list_startup_issues,
            remove_unavailable_profile,
            get_profile_for_edit,
            create_profile,
            update_profile,
            delete_profile,
            rotate_local_key,
            query_activity,
            scan_environment_profiles,
            clear_environment_discoveries,
            query_usage,
            query_balance,
            test_profile_connection
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("Tauri startup failed: {error}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use secret_store::SecretStoreError;

    #[derive(Debug, Default)]
    struct FaultySecretStore {
        secrets: Mutex<HashMap<(SecretKind, Uuid), SecretString>>,
        fail_put: Mutex<Option<SecretKind>>,
        fail_delete: Mutex<Option<SecretKind>>,
    }

    impl FaultySecretStore {
        fn fail_next_put(&self, kind: SecretKind) {
            if let Ok(mut failure) = self.fail_put.lock() {
                *failure = Some(kind);
            }
        }

        fn fail_next_delete(&self, kind: SecretKind) {
            if let Ok(mut failure) = self.fail_delete.lock() {
                *failure = Some(kind);
            }
        }

        fn entry_count(&self) -> usize {
            self.secrets
                .lock()
                .map_or(usize::MAX, |values| values.len())
        }
    }

    impl SecretStore for FaultySecretStore {
        fn put(
            &self,
            kind: SecretKind,
            profile_id: Uuid,
            value: SecretString,
        ) -> Result<(), SecretStoreError> {
            let mut fail_put = self
                .fail_put
                .lock()
                .map_err(|_| SecretStoreError::Storage)?;
            if fail_put.is_some_and(|failure| failure == kind) {
                *fail_put = None;
                return Err(SecretStoreError::Storage);
            }
            drop(fail_put);
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
            let mut fail_delete = self
                .fail_delete
                .lock()
                .map_err(|_| SecretStoreError::Storage)?;
            if fail_delete.is_some_and(|failure| failure == kind) {
                *fail_delete = None;
                return Err(SecretStoreError::Storage);
            }
            drop(fail_delete);
            self.secrets
                .lock()
                .map_err(|_| SecretStoreError::Storage)?
                .remove(&(kind, profile_id));
            Ok(())
        }
    }

    fn test_state(secrets: Arc<dyn SecretStore>) -> (DesktopState, std::path::PathBuf) {
        let directory = std::env::temp_dir().join(format!("lpg-desktop-test-{}", Uuid::new_v4()));
        let created_directory = std::fs::create_dir_all(&directory);
        assert!(created_directory.is_ok());
        let database_path = directory.join("profiles.sqlite");
        let repository = match Repository::open(&database_path) {
            Ok(repository) => repository,
            Err(error) => panic!("open test repository: {error}"),
        };
        let activity = match ActivityService::open(&database_path) {
            Ok(activity) => activity,
            Err(error) => panic!("open activity service: {error}"),
        };
        (
            DesktopState {
                gateway: Arc::new(Mutex::new(GatewayControl::default())),
                profiles: Arc::new(Mutex::new(Vec::new())),
                startup_issues: Mutex::new(Vec::new()),
                repository: Arc::new(Mutex::new(repository)),
                activity,
                secrets,
                discoveries: Arc::new(Mutex::new(HashMap::new())),
            },
            directory,
        )
    }

    fn fixture_draft() -> ProfileDraft {
        ProfileDraft {
            name: "fixture".to_owned(),
            protocol: "openai".to_owned(),
            upstream_url: "https://api.example.test".to_owned(),
            upstream_key: "fixture-upstream-key".to_owned(),
            discovery_id: None,
            policy: "balanced".to_owned(),
            custom_terms: vec!["Project A".to_owned(), " Project A ".to_owned()],
        }
    }

    fn secret_value(store: &dyn SecretStore, kind: SecretKind, profile_id: Uuid) -> Option<String> {
        store
            .get(kind, profile_id)
            .ok()
            .flatten()
            .map(|value| value.expose_secret().to_owned())
    }

    #[test]
    fn create_profile_uses_protocol_for_local_url_after_persisting() {
        let (state, directory) = test_state(Arc::new(MemorySecretStore::default()));

        let created = match create_profile_with_state(fixture_draft(), &state) {
            Ok(created) => created,
            Err(error) => panic!("create profile: {error}"),
        };

        assert_eq!(created.profile.protocol, "openai");
        assert_eq!(created.profile.custom_term_count, 1);
        assert_eq!(
            created.local_base_url,
            format!("http://127.0.0.1:{LOCAL_GATEWAY_PORT}/v1")
        );
        let profile_count = match state.profiles.lock() {
            Ok(profiles) => profiles.len(),
            Err(error) => panic!("read created profiles: {error}"),
        };
        assert_eq!(profile_count, 1);
        drop(state);
        let removed_directory = std::fs::remove_dir_all(directory);
        assert!(removed_directory.is_ok());
    }

    #[test]
    fn empty_environment_values_are_ignored_without_trimming_valid_secrets() {
        assert!(non_empty_environment_value(String::new(), "process").is_none());
        assert!(non_empty_environment_value("  \r\n".to_owned(), "user").is_none());

        let value = non_empty_environment_value("  secret with spaces  ".to_owned(), "system")
            .unwrap_or_else(|| panic!("non-empty environment value"));
        assert_eq!(value.value, "  secret with spaces  ");
        assert_eq!(value.source, "system");
    }

    #[test]
    fn create_profile_rejects_an_expired_environment_discovery() {
        let (state, directory) = test_state(Arc::new(MemorySecretStore::default()));
        let discovery_id = "expired-discovery".to_owned();
        assert!(state.discoveries.lock().is_ok_and(|mut discoveries| {
            discoveries.insert(
                discovery_id.clone(),
                DiscoveredSecret {
                    key: SecretString::from("expired-upstream-key".to_owned()),
                    protocol: "openai".to_owned(),
                    upstream_url: "https://api.example.test".to_owned(),
                    expires_at_ms: current_time_ms().saturating_sub(1),
                },
            );
            true
        }));
        let mut draft = fixture_draft();
        draft.upstream_key.clear();
        draft.discovery_id = Some(discovery_id);

        let result = create_profile_with_state(draft, &state);

        assert_eq!(
            result.err().as_deref(),
            Some("discovered credential expired; scan again")
        );
        assert!(
            state
                .discoveries
                .lock()
                .is_ok_and(|discoveries| discoveries.is_empty())
        );
        drop(state);
        assert!(std::fs::remove_dir_all(directory).is_ok());
    }

    #[test]
    fn create_profile_removes_partial_credentials_after_storage_failure() {
        let secrets = Arc::new(FaultySecretStore::default());
        secrets.fail_next_put(SecretKind::Mapping);
        let (state, directory) = test_state(secrets.clone());

        let result = create_profile_with_state(fixture_draft(), &state);

        assert!(result.is_err());
        assert_eq!(secrets.entry_count(), 0);
        assert!(
            state
                .profiles
                .lock()
                .is_ok_and(|profiles| profiles.is_empty())
        );
        assert!(state.repository.lock().is_ok_and(|repository| {
            repository
                .profiles()
                .is_ok_and(|profiles| profiles.is_empty())
        }));
        drop(state);
        assert!(std::fs::remove_dir_all(directory).is_ok());
    }

    #[test]
    fn profile_loading_reports_missing_credentials_instead_of_silently_hiding_the_record() {
        let repository = match Repository::open_in_memory() {
            Ok(repository) => repository,
            Err(error) => panic!("open repository: {error}"),
        };
        let id = Uuid::new_v4();
        let saved = repository.save_profile(&ProfileInput {
            id,
            name: "unavailable fixture".to_owned(),
            protocol: Protocol::OpenAi,
            upstream_url: "https://api.example.test".to_owned(),
            upstream_secret_ref: "upstream-ref".to_owned(),
            local_secret_ref: "local-ref".to_owned(),
            local_key_digest: [0_u8; 32],
            policy: "strict".to_owned(),
        });
        assert!(saved.is_ok());

        let loaded = load_desktop_profiles(&repository, &MemorySecretStore::default());

        assert!(loaded.is_ok_and(|(profiles, issues)| {
            profiles.is_empty()
                && issues.len() == 1
                && issues[0].profile_id == id.to_string()
                && issues[0].profile_name == "unavailable fixture"
                && issues[0].reason == "missing_credentials"
        }));
    }

    #[test]
    fn unavailable_profile_removal_deletes_the_record_after_explicit_recovery_action() {
        let secrets = Arc::new(FaultySecretStore::default());
        let (state, directory) = test_state(secrets);
        let id = Uuid::new_v4();
        assert!(state.repository.lock().is_ok_and(|repository| {
            repository
                .save_profile(&ProfileInput {
                    id,
                    name: "unavailable fixture".to_owned(),
                    protocol: Protocol::OpenAi,
                    upstream_url: "https://api.example.test".to_owned(),
                    upstream_secret_ref: "upstream-ref".to_owned(),
                    local_secret_ref: "local-ref".to_owned(),
                    local_key_digest: [0_u8; 32],
                    policy: "strict".to_owned(),
                })
                .is_ok()
        }));
        assert!(state.startup_issues.lock().is_ok_and(|mut issues| {
            issues.push(StartupIssue {
                profile_id: id.to_string(),
                profile_name: "unavailable fixture".to_owned(),
                reason: "missing_credentials".to_owned(),
            });
            true
        }));

        let removed = remove_unavailable_profile_with_state(id.to_string(), &state);

        assert!(removed.is_ok());
        assert!(
            state
                .startup_issues
                .lock()
                .is_ok_and(|issues| issues.is_empty())
        );
        assert!(state.repository.lock().is_ok_and(|repository| {
            repository
                .profile(id)
                .is_ok_and(|profile| profile.is_none())
        }));
        drop(state);
        assert!(std::fs::remove_dir_all(directory).is_ok());
    }

    #[test]
    fn profile_mutation_is_rejected_while_gateway_startup_is_reserved() {
        let (state, directory) = test_state(Arc::new(MemorySecretStore::default()));
        assert!(state.gateway.lock().is_ok_and(|mut control| {
            control.starting = true;
            true
        }));

        let result = create_profile_with_state(fixture_draft(), &state);

        assert_eq!(
            result.err().as_deref(),
            Some("stop the gateway before changing profiles")
        );
        assert!(state.repository.lock().is_ok_and(|repository| {
            repository
                .profiles()
                .is_ok_and(|profiles| profiles.is_empty())
        }));
        drop(state);
        assert!(std::fs::remove_dir_all(directory).is_ok());
    }

    #[test]
    fn update_profile_requires_a_new_key_before_changing_upstream_origin() {
        let secrets = Arc::new(MemorySecretStore::default());
        let (state, directory) = test_state(secrets.clone());
        let created = match create_profile_with_state(fixture_draft(), &state) {
            Ok(created) => created,
            Err(error) => panic!("create profile: {error}"),
        };
        let profile_id = match Uuid::parse_str(&created.profile.id) {
            Ok(id) => id,
            Err(error) => panic!("parse profile id: {error}"),
        };
        let original_key = secret_value(secrets.as_ref(), SecretKind::Upstream, profile_id);
        let mut update = fixture_draft();
        update.upstream_url = "https://other.example.test/v1".to_owned();
        update.upstream_key.clear();

        let result = update_profile_with_state(created.profile.id, update, &state);

        assert_eq!(
            result.err().as_deref(),
            Some("new upstream API key required when changing upstream origin")
        );
        assert_eq!(
            secret_value(secrets.as_ref(), SecretKind::Upstream, profile_id),
            original_key
        );
        assert!(state.profiles.lock().is_ok_and(|profiles| {
            profiles.len() == 1 && profiles[0].summary.upstream_url == "https://api.example.test"
        }));
        drop(state);
        assert!(std::fs::remove_dir_all(directory).is_ok());
    }

    #[test]
    fn update_profile_preserves_local_identity_and_replaces_editable_secrets() {
        let secrets = Arc::new(FaultySecretStore::default());
        let (state, directory) = test_state(secrets.clone());
        let created = match create_profile_with_state(fixture_draft(), &state) {
            Ok(created) => created,
            Err(error) => panic!("create profile: {error}"),
        };
        let profile_id = match Uuid::parse_str(&created.profile.id) {
            Ok(profile_id) => profile_id,
            Err(error) => panic!("parse profile id: {error}"),
        };
        let original_local = secret_value(secrets.as_ref(), SecretKind::Local, profile_id);
        let original_mapping = secret_value(secrets.as_ref(), SecretKind::Mapping, profile_id);
        let mut update = fixture_draft();
        update.name = "updated fixture".to_owned();
        update.upstream_url = "https://gateway.example.test/v1".to_owned();
        update.upstream_key = "replacement-upstream-key".to_owned();
        update.policy = "monitor".to_owned();
        update.custom_terms = vec!["Project B".to_owned()];

        let updated = match update_profile_with_state(created.profile.id, update, &state) {
            Ok(updated) => updated,
            Err(error) => panic!("update profile: {error}"),
        };

        assert_eq!(updated.name, "updated fixture");
        assert_eq!(updated.policy, "monitor");
        assert_eq!(updated.custom_term_count, 1);
        assert_eq!(
            secret_value(secrets.as_ref(), SecretKind::Local, profile_id),
            original_local
        );
        assert_eq!(
            secret_value(secrets.as_ref(), SecretKind::Mapping, profile_id),
            original_mapping
        );
        assert!(
            secrets
                .get(SecretKind::Upstream, profile_id)
                .is_ok_and(|value| {
                    value.is_some_and(|value| value.expose_secret() == "replacement-upstream-key")
                })
        );
        assert!(state.profiles.lock().is_ok_and(|profiles| {
            profiles.first().is_some_and(|profile| {
                profile.local_key == created.local_key
                    && profile.custom_terms == ["Project B"]
                    && profile.summary.id == updated.id
                    && profile.summary.name == updated.name
                    && profile.summary.upstream_url == updated.upstream_url
                    && profile.summary.policy == updated.policy
            })
        }));
        drop(state);
        assert!(std::fs::remove_dir_all(directory).is_ok());
    }

    #[test]
    fn update_profile_restores_secrets_and_state_after_storage_failure() {
        let secrets = Arc::new(FaultySecretStore::default());
        let (state, directory) = test_state(secrets.clone());
        let created = match create_profile_with_state(fixture_draft(), &state) {
            Ok(created) => created,
            Err(error) => panic!("create profile: {error}"),
        };
        let profile_id = match Uuid::parse_str(&created.profile.id) {
            Ok(profile_id) => profile_id,
            Err(error) => panic!("parse profile id: {error}"),
        };
        let original_upstream = secret_value(secrets.as_ref(), SecretKind::Upstream, profile_id);
        let original_dictionary =
            secret_value(secrets.as_ref(), SecretKind::Dictionary, profile_id);
        let mut update = fixture_draft();
        update.name = "must not persist".to_owned();
        update.upstream_key = "must-not-persist".to_owned();
        update.custom_terms = vec!["Must not persist".to_owned()];
        secrets.fail_next_put(SecretKind::Dictionary);

        let result = update_profile_with_state(created.profile.id.clone(), update, &state);

        assert!(result.is_err());
        assert_eq!(
            secret_value(secrets.as_ref(), SecretKind::Upstream, profile_id),
            original_upstream
        );
        assert_eq!(
            secret_value(secrets.as_ref(), SecretKind::Dictionary, profile_id),
            original_dictionary
        );
        assert!(state.profiles.lock().is_ok_and(|profiles| {
            profiles.first().is_some_and(|profile| {
                profile.summary.id == created.profile.id
                    && profile.summary.name == created.profile.name
                    && profile.summary.upstream_url == created.profile.upstream_url
                    && profile.summary.policy == created.profile.policy
            })
        }));
        assert!(state.repository.lock().is_ok_and(|repository| {
            repository.profile(profile_id).is_ok_and(|stored| {
                stored.is_some_and(|stored| stored.name == created.profile.name)
            })
        }));
        drop(state);
        assert!(std::fs::remove_dir_all(directory).is_ok());
    }

    #[test]
    fn delete_profile_restores_credentials_when_removal_fails() {
        let secrets = Arc::new(FaultySecretStore::default());
        let (state, directory) = test_state(secrets.clone());
        let created = match create_profile_with_state(fixture_draft(), &state) {
            Ok(created) => created,
            Err(error) => panic!("create profile: {error}"),
        };
        let profile_id = match Uuid::parse_str(&created.profile.id) {
            Ok(profile_id) => profile_id,
            Err(error) => panic!("parse profile id: {error}"),
        };
        secrets.fail_next_delete(SecretKind::Mapping);

        let result = delete_profile_with_state(created.profile.id, &state);

        assert!(result.is_err());
        assert!(
            state
                .profiles
                .lock()
                .is_ok_and(|profiles| profiles.len() == 1)
        );
        assert!(state.repository.lock().is_ok_and(|repository| {
            repository
                .profile(profile_id)
                .is_ok_and(|profile| profile.is_some())
        }));
        for kind in PROFILE_SECRET_KINDS {
            assert!(
                secrets
                    .get(kind, profile_id)
                    .is_ok_and(|value| value.is_some())
            );
        }
        drop(state);
        assert!(std::fs::remove_dir_all(directory).is_ok());
    }

    #[test]
    fn rotate_local_key_restores_old_secret_when_persistence_fails() {
        let secrets = Arc::new(FaultySecretStore::default());
        let (state, directory) = test_state(secrets.clone());
        let created = match create_profile_with_state(fixture_draft(), &state) {
            Ok(created) => created,
            Err(error) => panic!("create profile: {error}"),
        };
        let profile_id = match Uuid::parse_str(&created.profile.id) {
            Ok(profile_id) => profile_id,
            Err(error) => panic!("parse profile id: {error}"),
        };
        let repository = Arc::clone(&state.repository);
        let poisoned = std::thread::spawn(move || {
            let guard = match repository.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            let _keep_guard_alive = guard;
            panic!("poison repository mutex for rollback test");
        })
        .join();
        assert!(poisoned.is_err());

        let result = rotate_local_key_with_state(created.profile.id, &state);

        assert!(result.is_err());
        assert!(state.profiles.lock().is_ok_and(|profiles| {
            profiles
                .first()
                .is_some_and(|profile| profile.local_key == created.local_key)
        }));
        assert!(
            secrets.get(SecretKind::Local, profile_id).is_ok_and(
                |value| value.is_some_and(|value| value.expose_secret() == created.local_key)
            )
        );
        drop(state);
        assert!(std::fs::remove_dir_all(directory).is_ok());
    }

    #[test]
    fn legacy_database_migration_copies_a_consistent_snapshot_without_removing_source() {
        let root = std::env::temp_dir().join(format!("lpg-migration-test-{}", Uuid::new_v4()));
        let legacy = root.join("legacy");
        let current = root.join("current");
        assert!(std::fs::create_dir_all(&legacy).is_ok());
        let source_path = legacy.join(PROFILE_DATABASE_NAME);
        let source = match Connection::open(&source_path) {
            Ok(connection) => connection,
            Err(error) => panic!("open migration source: {error}"),
        };
        assert!(
            source
                .execute_batch(
                    "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; \
                     CREATE TABLE migration_fixture(value TEXT NOT NULL); \
                     INSERT INTO migration_fixture VALUES ('preserved');",
                )
                .is_ok()
        );

        assert_eq!(migrate_legacy_database(&legacy, &current), Ok(true));
        let migrated = match Connection::open(current.join(PROFILE_DATABASE_NAME)) {
            Ok(connection) => connection,
            Err(error) => panic!("open migrated database: {error}"),
        };
        let value = migrated.query_row("SELECT value FROM migration_fixture", [], |row| {
            row.get::<_, String>(0)
        });
        assert_eq!(value.as_deref(), Ok("preserved"));
        assert!(source_path.exists());
        assert!(!current.join(".profiles.sqlite.migrating").exists());
        drop(migrated);
        drop(source);
        assert!(std::fs::remove_dir_all(root).is_ok());
    }

    #[test]
    fn legacy_database_migration_never_overwrites_current_data() {
        let root = std::env::temp_dir().join(format!("lpg-migration-test-{}", Uuid::new_v4()));
        let legacy = root.join("legacy");
        let current = root.join("current");
        assert!(std::fs::create_dir_all(&legacy).is_ok());
        assert!(std::fs::create_dir_all(&current).is_ok());
        for (directory, value) in [(&legacy, "legacy"), (&current, "current")] {
            let database = match Connection::open(directory.join(PROFILE_DATABASE_NAME)) {
                Ok(connection) => connection,
                Err(error) => panic!("open migration fixture: {error}"),
            };
            assert!(
                database
                    .execute_batch(&format!(
                        "CREATE TABLE migration_fixture(value TEXT NOT NULL); \
                         INSERT INTO migration_fixture VALUES ('{value}');"
                    ))
                    .is_ok()
            );
        }

        assert_eq!(migrate_legacy_database(&legacy, &current), Ok(false));
        let current_database = match Connection::open(current.join(PROFILE_DATABASE_NAME)) {
            Ok(connection) => connection,
            Err(error) => panic!("reopen current migration fixture: {error}"),
        };
        let value = current_database.query_row("SELECT value FROM migration_fixture", [], |row| {
            row.get::<_, String>(0)
        });
        assert_eq!(value.as_deref(), Ok("current"));
        drop(current_database);
        assert!(std::fs::remove_dir_all(root).is_ok());
    }

    #[test]
    fn failed_legacy_database_migration_keeps_the_source_and_no_destination() {
        let root = std::env::temp_dir().join(format!("lpg-migration-test-{}", Uuid::new_v4()));
        let legacy = root.join("legacy");
        let current = root.join("current");
        assert!(std::fs::create_dir_all(&legacy).is_ok());
        let source = legacy.join(PROFILE_DATABASE_NAME);
        assert!(std::fs::write(&source, b"not a sqlite database").is_ok());

        assert!(migrate_legacy_database(&legacy, &current).is_err());
        assert!(std::fs::read(&source).is_ok_and(|bytes| bytes == b"not a sqlite database"));
        assert!(!current.join(PROFILE_DATABASE_NAME).exists());
        assert!(!current.join(".profiles.sqlite.migrating").exists());
        assert!(std::fs::remove_dir_all(root).is_ok());
    }

    #[test]
    fn environment_provider_inference_distinguishes_custom_compatible_urls() {
        assert_eq!(
            infer_provider("https://api.deepseek.com", "openai", "OpenAI").0,
            "deepseek"
        );
        assert_eq!(
            infer_provider("https://gateway.example.test/v1", "openai", "OpenAI").0,
            "custom"
        );
        assert_eq!(
            infer_provider("https://gateway.example.test", "deepseek", "DeepSeek").0,
            "deepseek"
        );
    }

    #[test]
    fn secret_preview_does_not_duplicate_key_separator() {
        assert_eq!(mask_secret("sk-1234567890"), "sk-••••••7890");
        assert_eq!(mask_secret("short"), "••••••••");
    }

    #[test]
    fn parses_verified_provider_balance_contracts() {
        let deepseek = serde_json::json!({
            "is_available": true,
            "balance_infos": [{"currency": "CNY", "total_balance": "110.00"}]
        });
        let kimi = serde_json::json!({
            "code": 0,
            "data": {"available_balance": 49.58894, "voucher_balance": 46.58893},
            "status": true
        });
        let siliconflow = serde_json::json!({
            "code": 20000,
            "status": true,
            "data": {"balance": "0.88", "chargeBalance": "88.00", "totalBalance": "88.88"}
        });
        let openrouter = serde_json::json!({
            "data": {"limit": 100, "limit_remaining": 74.5, "usage": 25.5}
        });

        assert!(
            parse_balance_response("DeepSeek", &deepseek).is_ok_and(|value| {
                value.remaining == Some(110.0) && value.unit.as_deref() == Some("CNY")
            })
        );
        assert!(parse_balance_response("Kimi", &kimi).is_ok_and(|value| {
            value.remaining == Some(49.58894) && value.unit.as_deref() == Some("CNY")
        }));
        assert!(
            parse_balance_response("SiliconFlow", &siliconflow)
                .is_ok_and(|value| value.remaining == Some(88.88))
        );
        assert!(
            parse_balance_response("OpenRouter", &openrouter).is_ok_and(|value| {
                value.remaining == Some(74.5)
                    && value.total == Some(100.0)
                    && value.used == Some(25.5)
            })
        );
    }

    #[test]
    fn rejects_malformed_balance_responses() {
        assert!(parse_balance_response("DeepSeek", &serde_json::json!({})).is_err());
        assert!(parse_balance_response("Kimi", &serde_json::json!({"data": {}})).is_err());
        assert!(parse_balance_response("OpenRouter", &serde_json::json!({"data": {}})).is_err());
    }
}
