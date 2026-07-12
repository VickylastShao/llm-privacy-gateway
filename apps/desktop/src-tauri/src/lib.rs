#![forbid(unsafe_code)]

use std::sync::Mutex;

use activity_store::validate_upstream_url;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

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

#[derive(Debug, Deserialize)]
struct ProfileDraft {
    name: String,
    protocol: String,
    upstream_url: String,
    policy: String,
}

#[derive(Default)]
struct DesktopState {
    running: Mutex<bool>,
    profiles: Mutex<Vec<ProfileSummary>>,
}

#[tauri::command]
fn gateway_status(state: State<'_, DesktopState>) -> Result<GatewayStatus, String> {
    Ok(GatewayStatus {
        running: *state
            .running
            .lock()
            .map_err(|_| "gateway state unavailable")?,
    })
}

#[tauri::command]
fn start_gateway(state: State<'_, DesktopState>) -> Result<GatewayStatus, String> {
    *state
        .running
        .lock()
        .map_err(|_| "gateway state unavailable")? = true;
    Ok(GatewayStatus { running: true })
}

#[tauri::command]
fn stop_gateway(state: State<'_, DesktopState>) -> Result<GatewayStatus, String> {
    *state
        .running
        .lock()
        .map_err(|_| "gateway state unavailable")? = false;
    Ok(GatewayStatus { running: false })
}

#[tauri::command]
fn list_profiles(state: State<'_, DesktopState>) -> Result<Vec<ProfileSummary>, String> {
    Ok(state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?
        .clone())
}

#[tauri::command]
fn create_profile(
    draft: ProfileDraft,
    state: State<'_, DesktopState>,
) -> Result<ProfileSummary, String> {
    if !matches!(draft.protocol.as_str(), "openai" | "anthropic") {
        return Err("unsupported protocol".to_owned());
    }
    validate_upstream_url(&draft.upstream_url)
        .map_err(|_| "upstream URL must be HTTPS or loopback HTTP".to_owned())?;
    let profile = ProfileSummary {
        id: Uuid::new_v4().to_string(),
        name: draft.name,
        protocol: draft.protocol,
        upstream_url: draft.upstream_url,
        policy: draft.policy,
    };
    state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?
        .push(profile.clone());
    Ok(profile)
}

#[tauri::command]
fn delete_profile(id: String, state: State<'_, DesktopState>) -> Result<(), String> {
    let profiles = &mut *state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?;
    let before = profiles.len();
    profiles.retain(|profile| profile.id != id);
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
    if !state
        .profiles
        .lock()
        .map_err(|_| "profile state unavailable")?
        .iter()
        .any(|profile| profile.id == id)
    {
        return Err("profile does not exist".to_owned());
    }
    Ok(serde_json::json!({"profile_id": id, "rotated": true}))
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
