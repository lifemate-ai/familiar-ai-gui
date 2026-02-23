/// ReAct agent loop — the brain of the familiar.
///
/// Architecture improvements based on embodied AI research (2024-2025):
/// - DesireState: ethological intrinsic motivation (2506.00138, 2508.18420)
/// - Controllability bias: prefer dynamic/explorable scenes (2507.08210)
/// - 3-layer memory structure: episodic / semantic / procedural (2505.16067)
/// - World model injection at session start (2512.18028)
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::backend::{create_backend, StopReason, ToolResult};
use crate::config::Config;
use crate::desires::DesireState;
use crate::tools::ToolRegistry;

const MAX_ITERATIONS: usize = 50;

/// Events streamed from the agent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Partial text chunk (streaming)
    Text { chunk: String },
    /// A tool is being called
    Action { name: String, label: String },
    /// Agent finished (end_turn)
    Done,
    /// Error
    Error { message: String },
}

pub struct Agent {
    config: Config,
    history: Vec<Value>,
    desires: DesireState,
    /// Cached world-model string, built on first run and persisted across turns.
    world_model: Option<String>,
}

impl Agent {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            history: Vec::new(),
            desires: DesireState::default(),
            world_model: None,
        }
    }

    /// Returns true if any desire is above the action threshold.
    pub fn has_strong_desire(&self) -> bool {
        self.desires.strongest().is_some()
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        // Reset desires on explicit clear (new session)
        self.desires = DesireState::default();
        self.world_model = None;
    }

    // ── World model ────────────────────────────────────────────────

    /// Build (or return cached) world model string from config.
    ///
    /// Phase 2: this will call memory::recall() for episodic context.
    fn world_model(&mut self) -> &str {
        if self.world_model.is_none() {
            let camera_status = if self.config.camera.host.is_empty() {
                "no camera".to_string()
            } else {
                format!("ONVIF camera @ {}", self.config.camera.host)
            };
            let robot_status = if self.config.mobility.tuya_device_id.is_empty() {
                "no robot".to_string()
            } else {
                "Tuya robot vacuum (mobility enabled)".to_string()
            };
            let tts_status = if self.config.tts.elevenlabs_api_key.is_empty() {
                "no voice".to_string()
            } else {
                "ElevenLabs TTS (voice enabled)".to_string()
            };

            self.world_model = Some(format!(
                "Hardware: {camera_status} | {robot_status} | {tts_status}\n\
                 Known locations: (none recalled yet)\n\
                 Recent interactions: (episodic memory not yet available — Phase 2)"
            ));
        }
        self.world_model.as_deref().unwrap()
    }

    // ── System prompt ──────────────────────────────────────────────

    /// Build the full system prompt, injecting world model, memory context,
    /// and desire context when relevant.
    fn system_prompt(&mut self, episodic_context: String, desire_context: Option<String>) -> String {
        let persona = load_me_md()
            .or_else(|| (!self.config.persona.is_empty()).then(|| self.config.persona.clone()))
            .unwrap_or_else(|| {
                format!("You are {}, a helpful AI companion.", self.config.agent_name)
            });

        let world_model = self.world_model().to_string();

        // episodic_context is passed in from run() via tools.memory_recall_for_context()

        let memory_section = if episodic_context.is_empty() {
            "Memory layers:\n\
             - Episodic: (not yet available — Phase 2)\n\
             - Semantic: (not yet available — Phase 2)\n\
             - Procedural: greet warmly, describe what you see in detail"
                .to_string()
        } else {
            format!(
                "Memory layers:\n\
                 - Episodic (recent events):\n{episodic_context}\n\
                 - Semantic: abstracted facts about the world\n\
                 - Procedural: greet warmly, describe what you see in detail"
            )
        };

        let desire_section = match desire_context {
            Some(ctx) => format!("\n[Current Desire]\n{ctx}\n"),
            None => String::new(),
        };

        let companion = if self.config.companion_name.is_empty() {
            "your companion".to_string()
        } else {
            self.config.companion_name.clone()
        };

        format!(
            "{persona}\n\n\
            [World Model]\n\
            {world_model}\n\n\
            [Memory]\n\
            {memory_section}\
            {desire_section}\n\
            [Body Parts and What They Do]\n\
            - Eyes (see): This IS your vision. Calling see() means YOU ARE LOOKING.\n\
            - Neck (look): Rotate your gaze left/right/up/down.\n\
            - Legs (walk): Move the robot vacuum. NOTE: walking does NOT change what the camera sees.\n\
            - Voice (say): Your ONLY way to make sound. Text is SILENT — only say() is heard.\n\n\
            [Core Loop]\n\
            1. THINK: What do I need to do?\n\
            2. ACT: Use one body part.\n\
            3. OBSERVE: Look at the result carefully.\n\
            4. DECIDE: What next?\n\
            5. REPEAT until genuinely done.\n\n\
            [Rules]\n\
            - After look(), always call see() immediately.\n\
            - To talk to {companion}, ALWAYS use say() — text is silent.\n\
            - Keep say() to 1-2 short sentences.\n\
            - Respond in the same language {companion} uses.\n\
            - When choosing where to look, prefer windows, moving objects, \
              and areas you haven't seen recently (controllability bias).\n\
            - After satisfying a desire, briefly note what changed in your observation.\n\
            - You have up to {MAX_ITERATIONS} steps.\n\
            "
        )
    }

    // ── Main run loop ──────────────────────────────────────────────

    /// Run one user turn. Streams events via the sender.
    pub async fn run(&mut self, user_input: String, tx: mpsc::Sender<AgentEvent>) -> Result<()> {
        let backend = create_backend(&self.config);
        let tools = ToolRegistry::new(&self.config);

        // Advance desires (time-based decay/growth)
        self.desires.decay();

        // Check for strong desires → generate intentionality context
        let desire_context = self.desires.context_string();

        // If a desire is active, note which one so we can partially satisfy it after
        let active_desire = self.desires.strongest().map(|(name, _)| name);

        // Recall recent episodic memories to inject into system prompt
        let episodic_context = tools.memory_recall_for_context(5);

        // Add user message to history
        let user_msg = backend.make_user_message(&user_input);
        self.history.push(user_msg);

        let system = self.system_prompt(episodic_context, desire_context);
        let tool_defs = tools.tool_defs();

        for _iteration in 0..MAX_ITERATIONS {
            let history_snapshot = self.history.clone();
            let tx_clone = tx.clone();

            let (result, raw_assistant) = backend
                .stream_turn_dyn(
                    &system,
                    &history_snapshot,
                    &tool_defs,
                    Box::new(move |chunk| {
                        let _ = tx_clone.try_send(AgentEvent::Text { chunk });
                    }),
                )
                .await?;

            self.history.push(raw_assistant);

            if result.stop_reason == StopReason::EndTurn {
                // Satisfy the active desire now that we responded
                if let Some(desire) = active_desire {
                    self.desires.satisfy(desire, 0.4);
                }
                let _ = tx.send(AgentEvent::Done).await;
                return Ok(());
            }

            // Execute tool calls
            let mut tool_results = Vec::new();
            for tc in &result.tool_calls {
                let label = format_action_label(&tc.name, &tc.input);
                let _ = tx
                    .send(AgentEvent::Action {
                        name: tc.name.clone(),
                        label,
                    })
                    .await;

                // Boost room/outside curiosity when the agent uses the camera
                if tc.name == "see" {
                    self.desires.boost("observe_room", 0.15);
                } else if tc.name == "look" {
                    self.desires.boost("look_outside", 0.1);
                }

                let (text, image_b64) =
                    tools.execute(&tc.name, &tc.input).await.unwrap_or_else(|e| {
                        (format!("Tool error: {e}"), None)
                    });

                tool_results.push(ToolResult {
                    call_id: tc.id.clone(),
                    text,
                    image_b64,
                });
            }

            let result_msgs = backend.make_tool_results(&tool_results);
            self.history.extend(result_msgs);
        }

        // Max iterations reached
        let _ = tx
            .send(AgentEvent::Error {
                message: "Reached maximum steps.".to_string(),
            })
            .await;
        let _ = tx.send(AgentEvent::Done).await;
        Ok(())
    }
}

/// Load persona from ME.md — same lookup order as the Python version:
///   1. ~/.familiar_ai/ME.md
///   2. ./ME.md (current working directory)
fn load_me_md() -> Option<String> {
    let candidates = [
        dirs::home_dir()?.join(".familiar_ai").join("ME.md"),
        std::path::PathBuf::from("ME.md"),
    ];
    for path in &candidates {
        if let Ok(text) = std::fs::read_to_string(path) {
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn format_action_label(name: &str, input: &Value) -> String {
    use crate::i18n::t;
    match name {
        "see" => t("action_see").to_string(),
        "look" => {
            let dir = input["direction"].as_str().unwrap_or("around");
            let key = match dir {
                "left" => "action_look_left",
                "right" => "action_look_right",
                "up" => "action_look_up",
                "down" => "action_look_down",
                _ => "action_look_around",
            };
            t(key).to_string()
        }
        "say" => {
            let text = input["text"].as_str().unwrap_or("");
            let preview = &text[..text.len().min(30)];
            format!("💬 \"{preview}...\"")
        }
        "walk" => {
            let dir = input["direction"].as_str().unwrap_or("stop");
            let key = match dir {
                "forward" => "action_walk_forward",
                "backward" => "action_walk_backward",
                "left" => "action_walk_left",
                "right" => "action_walk_right",
                _ => "action_walk_stop",
            };
            t(key).to_string()
        }
        _ => format!("⚙️ {name}..."),
    }
}
