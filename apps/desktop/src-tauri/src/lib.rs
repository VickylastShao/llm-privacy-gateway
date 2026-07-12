#![forbid(unsafe_code)]
#[tauri::command]
fn gateway_status() -> &'static str { "stopped" }
pub fn run() { tauri::Builder::default().invoke_handler(tauri::generate_handler![gateway_status]).run(tauri::generate_context!()).unwrap_or_else(|error| panic!("Tauri startup failed: {error}")); }
