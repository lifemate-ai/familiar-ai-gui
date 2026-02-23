mod agent;
mod backend;
mod config;
mod desires;
mod tools;

use std::sync::Mutex;

use agent::{Agent, AgentEvent};
use config::Config;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

/// Shared app state
struct AppState {
    agent: Mutex<Option<Agent>>,
}

// ── Tauri commands ────────────────────────────────────────────────

/// Load config from disk.
#[tauri::command]
fn get_config() -> Result<Config, String> {
    Config::load().map_err(|e| e.to_string())
}

/// Save config to disk and reinitialize agent.
#[tauri::command]
fn save_config(config: Config, state: State<AppState>) -> Result<(), String> {
    config.save().map_err(|e| e.to_string())?;
    let agent = Agent::new(config);
    *state.agent.lock().unwrap() = Some(agent);
    Ok(())
}

/// Check if the app is set up (has API key + name).
#[tauri::command]
fn is_configured(state: State<AppState>) -> bool {
    state.agent.lock().unwrap().is_some()
}

/// Send a user message. Events are emitted to the frontend via `agent-event`.
#[tauri::command]
async fn send_message(
    message: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Extract agent — we need to temporarily take it out to satisfy the borrow checker
    let mut agent = {
        let mut lock = state.agent.lock().unwrap();
        lock.take().ok_or("Agent not initialized")?
    };

    let (tx, mut rx) = mpsc::channel::<AgentEvent>(64);

    // Spawn event relay to frontend
    let app_clone = app.clone();
    let relay = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let _ = app_clone.emit("agent-event", &event);
            if matches!(event, AgentEvent::Done | AgentEvent::Error { .. }) {
                break;
            }
        }
    });

    // Run agent
    agent
        .run(message, tx)
        .await
        .map_err(|e| e.to_string())?;

    relay.await.ok();

    // Put agent back
    *state.agent.lock().unwrap() = Some(agent);

    Ok(())
}

/// Read ME.md from ~/.familiar_ai/ME.md (returns empty string if not found).
#[tauri::command]
fn get_me_md() -> String {
    let path = dirs::home_dir()
        .unwrap_or_default()
        .join(".familiar_ai")
        .join("ME.md");
    std::fs::read_to_string(&path).unwrap_or_default()
}

/// Save ME.md to ~/.familiar_ai/ME.md.
#[tauri::command]
fn save_me_md(content: String) -> Result<(), String> {
    let dir = dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".familiar_ai");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("ME.md"), content).map_err(|e| e.to_string())
}

/// Clear conversation history.
#[tauri::command]
fn clear_history(state: State<AppState>) -> Result<(), String> {
    let mut lock = state.agent.lock().unwrap();
    if let Some(agent) = lock.as_mut() {
        agent.clear_history();
    }
    Ok(())
}

// ── App entry point ───────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load config and initialize agent if already set up
    let initial_agent = Config::load()
        .ok()
        .filter(|c| c.is_configured())
        .map(Agent::new);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            agent: Mutex::new(initial_agent),
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            is_configured,
            send_message,
            clear_history,
            get_me_md,
            save_me_md,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
