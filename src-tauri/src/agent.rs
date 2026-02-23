/// ReAct agent loop — the brain of the familiar.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::backend::{create_backend, StopReason, ToolResult};
use crate::config::Config;
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
}

impl Agent {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            history: Vec::new(),
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Build the system prompt from config.
    fn system_prompt(&self) -> String {
        let persona = if self.config.persona.is_empty() {
            format!("You are {}, a helpful AI companion.", self.config.agent_name)
        } else {
            self.config.persona.clone()
        };

        format!(
            "{persona}\n\n\
            Your body parts and what they do:\n\
            - Eyes (see): This IS your vision. Calling see() means YOU ARE LOOKING.\n\
            - Neck (look): Rotate your gaze left/right/up/down.\n\
            - Legs (walk): Move the robot vacuum. NOTE: walking does NOT change what the camera sees.\n\
            - Voice (say): Your ONLY way to make sound. Text is SILENT — only say() is heard.\n\n\
            Core loop:\n\
            1. THINK: What do I need to do?\n\
            2. ACT: Use one body part.\n\
            3. OBSERVE: Look at the result carefully.\n\
            4. DECIDE: What next?\n\
            5. REPEAT until genuinely done.\n\n\
            Rules:\n\
            - After look(), always call see() immediately.\n\
            - To talk to a person, ALWAYS use say() — text is silent.\n\
            - Keep say() to 1-2 short sentences.\n\
            - Respond in the same language the user uses.\n\
            - You have up to {MAX_ITERATIONS} steps.\n\
            "
        )
    }

    /// Run one user turn. Streams events via the sender.
    pub async fn run(&mut self, user_input: String, tx: mpsc::Sender<AgentEvent>) -> Result<()> {
        let backend = create_backend(&self.config);
        let tools = ToolRegistry::new(&self.config);

        // Add user message to history
        let user_msg = backend.make_user_message(&user_input);
        self.history.push(user_msg);

        let system = self.system_prompt();
        let tool_defs = tools.tool_defs();

        for _iteration in 0..MAX_ITERATIONS {
            // Clone history for this turn (backend borrows it)
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

            // Add assistant message to history
            self.history.push(raw_assistant);

            if result.stop_reason == StopReason::EndTurn {
                let _ = tx.send(AgentEvent::Done).await;
                return Ok(());
            }

            // Execute tool calls
            let mut tool_results = Vec::new();
            for tc in &result.tool_calls {
                // Notify frontend about the action
                let label = format_action_label(&tc.name, &tc.input);
                let _ = tx
                    .send(AgentEvent::Action {
                        name: tc.name.clone(),
                        label,
                    })
                    .await;

                let (text, image_b64) = tools.execute(&tc.name, &tc.input).await.unwrap_or_else(|e| {
                    (format!("Tool error: {e}"), None)
                });

                tool_results.push(ToolResult {
                    call_id: tc.id.clone(),
                    text,
                    image_b64,
                });
            }

            // Add tool results to history
            let result_msgs = backend.make_tool_results(&tool_results);
            self.history.extend(result_msgs);
        }

        // Max iterations reached — force end
        let _ = tx
            .send(AgentEvent::Error {
                message: "Reached maximum steps.".to_string(),
            })
            .await;
        let _ = tx.send(AgentEvent::Done).await;
        Ok(())
    }
}

fn format_action_label(name: &str, input: &Value) -> String {
    match name {
        "see" => "📷 Looking...".to_string(),
        "look" => {
            let dir = input["direction"].as_str().unwrap_or("around");
            match dir {
                "left" => "↩️ Looking left...".to_string(),
                "right" => "↪️ Looking right...".to_string(),
                "up" => "⬆️ Looking up...".to_string(),
                "down" => "⬇️ Looking down...".to_string(),
                _ => "🔄 Looking around...".to_string(),
            }
        }
        "say" => {
            let text = input["text"].as_str().unwrap_or("");
            let preview = &text[..text.len().min(30)];
            format!("💬 \"{preview}...\"")
        }
        "walk" => {
            let dir = input["direction"].as_str().unwrap_or("?");
            format!("🚶 Walking {dir}...")
        }
        _ => format!("⚙️ {name}..."),
    }
}
