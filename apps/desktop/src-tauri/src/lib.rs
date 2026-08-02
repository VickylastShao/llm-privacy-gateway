#![forbid(unsafe_code)]

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use activity_service::{ActivityQuery, ActivityService};
use activity_store::{
    ProfileInput, Protocol, Repository, RepositoryMappingBackend, validate_upstream_url,
};
use gateway::{
    GatewayConfig, GatewayProtocol, GatewayState, RuntimeProfile, bind_listener, local_key_digest,
    serve_with_listener_and_shutdown,
};
use privacy_core::StaticDictionary;
use rand::{TryRngCore, rngs::OsRng};
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
    upstream_key: String,
    policy: String,
    #[serde(default)]
    custom_terms: Vec<String>,
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
    running: Option<RunningGateway>,
}

struct DesktopState {
    gateway: Arc<Mutex<GatewayControl>>,
    profiles: Arc<Mutex<Vec<DesktopProfile>>>,
    repository: Arc<Mutex<Repository>>,
    activity: Arc<ActivityService>,
    secrets: Arc<dyn SecretStore>,
}

impl DesktopState {
    fn open(database_path: &std::path::Path) -> Result<Self, String> {
        let activity =
            ActivityService::open(database_path).map_err(|_| "activity database unavailable")?;
        let repository =
            Repository::open(database_path).map_err(|_| "profile database unavailable")?;
        let secrets = default_secret_store();
        let mut profiles = Vec::new();
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
            let (Some(upstream_key), Some(local_key), Some(mapping_key)) =
                (upstream, local, mapping)
            else {
                continue;
            };
            let Ok(mapping_key) = decode_mapping_key(mapping_key.expose_secret()) else {
                continue;
            };
            let custom_terms = match dictionary {
                Some(value) => decode_custom_terms(value.expose_secret())?,
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
        Ok(Self {
            gateway: Arc::new(Mutex::new(GatewayControl::default())),
            profiles: Arc::new(Mutex::new(profiles)),
            repository: Arc::new(Mutex::new(repository)),
            activity,
            secrets,
        })
    }
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
    Ok(GatewayStatus {
        running: state
            .gateway
            .lock()
            .map_err(|_| "gateway state unavailable")?
            .running
            .is_some(),
    })
}

#[tauri::command]
async fn start_gateway(state: State<'_, DesktopState>) -> Result<GatewayStatus, String> {
    if state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?
        .running
        .is_some()
    {
        return Ok(GatewayStatus { running: true });
    }
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
    let runtime = GatewayState::new_with_sink(runtime_profiles, state.activity.clone())
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
    if control.running.is_some() {
        return Ok(GatewayStatus { running: true });
    }
    control.next_generation = control.next_generation.wrapping_add(1);
    let generation = control.next_generation;
    control.running = Some(RunningGateway {
        generation,
        shutdown,
    });
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
    let shutdown = state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?
        .running
        .take()
        .map(|running| running.shutdown);
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
fn create_profile(
    draft: ProfileDraft,
    state: State<'_, DesktopState>,
) -> Result<CreatedProfile, String> {
    create_profile_with_state(draft, state.inner())
}

fn create_profile_with_state(
    draft: ProfileDraft,
    state: &DesktopState,
) -> Result<CreatedProfile, String> {
    if draft.name.trim().is_empty() || draft.upstream_key.trim().is_empty() {
        return Err("profile name and upstream API key are required".to_owned());
    }
    let gateway_protocol = protocol_from_str(&draft.protocol)?;
    policy_from_str(&draft.policy)?;
    validate_upstream_url(&draft.upstream_url)
        .map_err(|_| "upstream URL must be HTTPS or loopback HTTP".to_owned())?;
    if state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?
        .running
        .is_some()
    {
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
    state
        .secrets
        .put(
            SecretKind::Upstream,
            id,
            SecretString::from(draft.upstream_key.clone()),
        )
        .map_err(|_| "credential storage failed")?;
    state
        .secrets
        .put(SecretKind::Local, id, SecretString::from(local_key.clone()))
        .map_err(|_| "credential storage failed")?;
    state
        .secrets
        .put(SecretKind::Mapping, id, SecretString::from(mapping_secret))
        .map_err(|_| "credential storage failed")?;
    state
        .secrets
        .put(
            SecretKind::Dictionary,
            id,
            SecretString::from(encode_custom_terms(&custom_terms)?),
        )
        .map_err(|_| "credential storage failed")?;
    let protocol = match gateway_protocol {
        GatewayProtocol::OpenAi => Protocol::OpenAi,
        GatewayProtocol::Anthropic => Protocol::Anthropic,
    };
    let (_, upstream_secret_ref) = service_account(SecretKind::Upstream, id);
    let (_, local_secret_ref) = service_account(SecretKind::Local, id);
    state
        .repository
        .lock()
        .map_err(|_| "profile database unavailable")?
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
        .map_err(|_| "profile persistence failed")?;
    state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?
        .push(DesktopProfile {
            id,
            summary: summary.clone(),
            upstream_key: SecretString::from(draft.upstream_key),
            local_key: local_key.clone(),
            mapping_key,
            custom_terms,
        });
    Ok(CreatedProfile {
        profile: summary,
        local_base_url,
        local_key,
    })
}

#[tauri::command]
fn delete_profile(id: String, state: State<'_, DesktopState>) -> Result<(), String> {
    if state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?
        .running
        .is_some()
    {
        return Err("stop the gateway before changing profiles".to_owned());
    }
    let profiles = &mut *state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?;
    let before = profiles.len();
    profiles.retain(|profile| profile.summary.id != id);
    if profiles.len() == before {
        return Err("profile does not exist".to_owned());
    }
    let id = Uuid::parse_str(&id).map_err(|_| "profile does not exist")?;
    state
        .repository
        .lock()
        .map_err(|_| "profile database unavailable")?
        .delete_profile(id)
        .map_err(|_| "profile persistence failed")?;
    for kind in [
        SecretKind::Upstream,
        SecretKind::Local,
        SecretKind::Mapping,
        SecretKind::Dictionary,
    ] {
        state
            .secrets
            .delete(kind, id)
            .map_err(|_| "credential removal failed")?;
    }
    Ok(())
}

#[tauri::command]
fn rotate_local_key(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<serde_json::Value, String> {
    if state
        .gateway
        .lock()
        .map_err(|_| "gateway state unavailable")?
        .running
        .is_some()
    {
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
    profile.local_key = format!("lpg_local_{}", Uuid::new_v4());
    state
        .secrets
        .put(
            SecretKind::Local,
            profile.id,
            SecretString::from(profile.local_key.clone()),
        )
        .map_err(|_| "credential storage failed")?;
    let protocol = match protocol_from_str(&profile.summary.protocol)? {
        GatewayProtocol::OpenAi => Protocol::OpenAi,
        GatewayProtocol::Anthropic => Protocol::Anthropic,
    };
    let (_, upstream_secret_ref) = service_account(SecretKind::Upstream, profile.id);
    let (_, local_secret_ref) = service_account(SecretKind::Local, profile.id);
    state
        .repository
        .lock()
        .map_err(|_| "profile database unavailable")?
        .save_profile(&ProfileInput {
            id: profile.id,
            name: profile.summary.name.clone(),
            protocol,
            upstream_url: profile.summary.upstream_url.clone(),
            upstream_secret_ref,
            local_secret_ref,
            local_key_digest: local_key_digest(&profile.local_key),
            policy: profile.summary.policy.clone(),
        })
        .map_err(|_| "profile persistence failed")?;
    Ok(serde_json::json!({"profile_id": id, "local_key": profile.local_key}))
}

#[tauri::command]
fn query_activity(limit: usize, state: State<'_, DesktopState>) -> Result<ActivityQuery, String> {
    state
        .activity
        .query(limit, current_time_ms())
        .map_err(|_| "activity database unavailable".to_owned())
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

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = app
                .path()
                .app_data_dir()
                .map_err(|_| std::io::Error::other("application data directory unavailable"))?;
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
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            gateway_status,
            start_gateway,
            stop_gateway,
            list_profiles,
            create_profile,
            delete_profile,
            rotate_local_key,
            query_activity
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("Tauri startup failed: {error}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_profile_uses_protocol_for_local_url_after_persisting() {
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
        let state = DesktopState {
            gateway: Arc::new(Mutex::new(GatewayControl::default())),
            profiles: Arc::new(Mutex::new(Vec::new())),
            repository: Arc::new(Mutex::new(repository)),
            activity,
            secrets: Arc::new(MemorySecretStore::default()),
        };

        let created = match create_profile_with_state(
            ProfileDraft {
                name: "fixture".to_owned(),
                protocol: "openai".to_owned(),
                upstream_url: "https://api.example.test".to_owned(),
                upstream_key: "fixture-upstream-key".to_owned(),
                policy: "balanced".to_owned(),
                custom_terms: vec!["Project A".to_owned(), " Project A ".to_owned()],
            },
            &state,
        ) {
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
}
