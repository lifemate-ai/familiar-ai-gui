mod agent;
mod backend;
mod coding;
mod config;
mod desires;
mod feedback;
mod i18n;
mod permissions;
mod tools;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use agent::{Agent, AgentEvent};
use config::Config;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

/// Shared app state — Arc so the heartbeat thread can hold a reference too.
struct AppState {
    agent: Arc<Mutex<Option<Agent>>>,
    /// Set to true to abort the current agent run.
    cancel_flag: Arc<AtomicBool>,
    /// Pending permission requests shared across agent turns.
    pending_perms: Arc<Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
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

/// Abort the currently running agent turn.
#[tauri::command]
fn cancel_message(state: State<AppState>) {
    state.cancel_flag.store(true, Ordering::Relaxed);
}

/// Respond to a pending permission request (allow/deny).
#[tauri::command]
fn respond_permission(id: String, allowed: bool, state: State<AppState>) {
    let mut lock = state.pending_perms.lock().unwrap();
    if let Some(tx) = lock.remove(&id) {
        let _ = tx.send(allowed);
    }
}

/// Send a user message. Events are emitted to the frontend via `agent-event`.
#[tauri::command]
async fn send_message(
    message: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Reset cancel flag before each new turn
    state.cancel_flag.store(false, Ordering::Relaxed);
    run_agent_turn(
        message,
        app,
        state.agent.clone(),
        state.cancel_flag.clone(),
        state.pending_perms.clone(),
    )
    .await
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

// ── Shared agent runner ───────────────────────────────────────────

/// Take the agent, run one turn, put it back. Used by both send_message and
/// the heartbeat thread so the logic lives in one place.
async fn run_agent_turn(
    message: String,
    app: AppHandle,
    agent_arc: Arc<Mutex<Option<Agent>>>,
    cancel_flag: Arc<AtomicBool>,
    pending_perms: Arc<Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
) -> Result<(), String> {
    let mut agent = {
        let mut lock = agent_arc.lock().unwrap();
        lock.take().ok_or("Agent not initialized")?
    };

    let (tx, mut rx) = mpsc::channel::<AgentEvent>(64);

    let app_clone = app.clone();
    let relay = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let _ = app_clone.emit("agent-event", &event);
            if matches!(
                event,
                AgentEvent::Done | AgentEvent::Cancelled | AgentEvent::Error { .. }
            ) {
                break;
            }
        }
    });

    agent
        .run(message, tx, cancel_flag, pending_perms)
        .await
        .map_err(|e| e.to_string())?;

    relay.await.ok();

    *agent_arc.lock().unwrap() = Some(agent);
    Ok(())
}

// ── Heartbeat thread ──────────────────────────────────────────────

/// Spawns a background task that checks desires every `interval_secs` and
/// fires an idle tick when a strong desire is present and the agent is free.
fn spawn_heartbeat(
    agent_arc: Arc<Mutex<Option<Agent>>>,
    app: AppHandle,
    cancel_flag: Arc<AtomicBool>,
    pending_perms: Arc<Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
    interval_secs: u64,
) {
    tauri::async_runtime::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
        interval.tick().await; // skip the immediate first tick

        loop {
            interval.tick().await;

            // Check: is agent free AND does it have a strong desire?
            let should_tick = {
                let lock = agent_arc.lock().unwrap();
                lock.as_ref()
                    .map(|a| a.has_strong_desire())
                    .unwrap_or(false)
                // lock drops here — agent is still Some
            };

            if should_tick {
                tracing::debug!("heartbeat: firing idle tick");
                cancel_flag.store(false, Ordering::Relaxed);
                let _ = run_agent_turn(
                    "(idle — your desires are active, act on them naturally)".to_string(),
                    app.clone(),
                    agent_arc.clone(),
                    cancel_flag.clone(),
                    pending_perms.clone(),
                )
                .await;
            }
        }
    });
}

// ── App entry point ───────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let initial_agent = Config::load()
        .ok()
        .filter(|c| c.is_configured())
        .map(Agent::new);

    let agent_arc = Arc::new(Mutex::new(initial_agent));

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let pending_perms: Arc<Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<bool>>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            agent: agent_arc.clone(),
            cancel_flag: cancel_flag.clone(),
            pending_perms: pending_perms.clone(),
        })
        .setup(move |app| {
            // Heartbeat: check desires every 60 seconds
            spawn_heartbeat(agent_arc.clone(), app.handle().clone(), cancel_flag.clone(), pending_perms.clone(), 60);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            is_configured,
            send_message,
            cancel_message,
            respond_permission,
            clear_history,
            get_me_md,
            save_me_md,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
