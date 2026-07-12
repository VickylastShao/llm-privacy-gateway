#![forbid(unsafe_code)]

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};

use activity_store::validate_upstream_url;
use gateway::{GatewayConfig, GatewayProtocol, GatewayState, RuntimeProfile, start_with_shutdown};
use rand::{TryRngCore, rngs::OsRng};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::oneshot;
use uuid::Uuid;

const LOCAL_GATEWAY_PORT: u16 = 17_891;

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
}

#[derive(Debug)]
struct DesktopProfile {
    id: Uuid,
    summary: ProfileSummary,
    upstream_key: SecretString,
    local_key: String,
    mapping_key: [u8; 32],
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

#[derive(Default)]
struct DesktopState {
    gateway: Arc<Mutex<GatewayControl>>,
    profiles: Arc<Mutex<Vec<DesktopProfile>>>,
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
fn start_gateway(state: State<'_, DesktopState>) -> Result<GatewayStatus, String> {
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
        let mut runtime = RuntimeProfile::new(
            profile.id,
            protocol,
            profile.local_key.clone(),
            profile.summary.upstream_url.clone(),
            profile.upstream_key.clone(),
            profile.mapping_key,
        );
        runtime.policy = policy_from_str(&profile.summary.policy)?;
        runtime_profiles.push(runtime);
    }
    drop(profiles);
    let runtime = GatewayState::new(runtime_profiles).map_err(|_| "gateway setup failed")?;
    let config = GatewayConfig::new(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        LOCAL_GATEWAY_PORT,
    ))
    .map_err(|_| "gateway must use loopback")?;
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
        let _ = start_with_shutdown(config, runtime, async move {
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
    if draft.name.trim().is_empty() || draft.upstream_key.trim().is_empty() {
        return Err("profile name and upstream API key are required".to_owned());
    }
    protocol_from_str(&draft.protocol)?;
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
    let summary = ProfileSummary {
        id: id.to_string(),
        name: draft.name,
        protocol: draft.protocol,
        upstream_url: draft.upstream_url,
        policy: draft.policy,
    };
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
        });
    Ok(CreatedProfile {
        profile: summary,
        local_base_url: match protocol_from_str(&draft.protocol)? {
            GatewayProtocol::OpenAi => format!("http://127.0.0.1:{LOCAL_GATEWAY_PORT}/v1"),
            GatewayProtocol::Anthropic => format!("http://127.0.0.1:{LOCAL_GATEWAY_PORT}"),
        },
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
    Ok(serde_json::json!({"profile_id": id, "local_key": profile.local_key}))
}

#[tauri::command]
fn transform_preview(text: String) -> serde_json::Value {
    serde_json::json!({"outbound_text": text, "warning":"preview does not persist secrets or mappings"})
}

#[tauri::command]
fn query_activity() -> Vec<serde_json::Value> {
    Vec::new()
}

#[tauri::command]
fn test_connection(upstream_url: String) -> Result<serde_json::Value, String> {
    validate_upstream_url(&upstream_url)
        .map_err(|_| "upstream URL must be HTTPS or loopback HTTP".to_owned())?;
    Ok(serde_json::json!({"validated": true}))
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

pub fn run() {
    tauri::Builder::default()
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            gateway_status,
            start_gateway,
            stop_gateway,
            list_profiles,
            create_profile,
            delete_profile,
            rotate_local_key,
            transform_preview,
            query_activity,
            test_connection
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("Tauri startup failed: {error}"));
}
