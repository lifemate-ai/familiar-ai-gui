pub mod anthropic;
pub mod gemini;
pub mod kimi;
pub mod openai;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::Config;

/// A single tool definition passed to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A tool call returned by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Result of one LLM turn.
#[derive(Debug)]
pub struct TurnResult {
    pub stop_reason: StopReason,
    #[allow(dead_code)]
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
}

/// Tool result to feed back to the LLM.
pub struct ToolResult {
    pub call_id: String,
    pub text: String,
    /// Optional JPEG image as base64
    pub image_b64: Option<String>,
}

/// Callback for streaming text chunks.
pub type TextCallback = Box<dyn Fn(String) + Send>;

/// Factory: create the right backend from config.
pub fn create_backend(config: &Config) -> Box<dyn LlmBackendDyn> {
    match config.platform.as_str() {
        "anthropic" => Box::new(anthropic::AnthropicBackend::new(
            config.api_key.clone(),
            config.effective_model().to_string(),
        )),
        "gemini" => Box::new(gemini::GeminiBackend::new(
            config.api_key.clone(),
            config.effective_model().to_string(),
        )),
        "openai" => Box::new(openai::OpenAiBackend::new(
            config.api_key.clone(),
            config.effective_model().to_string(),
        )),
        // Default: kimi
        _ => Box::new(kimi::KimiBackend::new(
            config.api_key.clone(),
            config.effective_model().to_string(),
        )),
    }
}

/// Object-safe wrapper around LlmBackend.
/// Needed because `impl Future` in traits isn't object-safe directly.
pub trait LlmBackendDyn: Send + Sync {
    fn stream_turn_dyn<'a>(
        &'a self,
        system: &'a str,
        history: &'a [serde_json::Value],
        tools: &'a [ToolDef],
        on_text: TextCallback,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(TurnResult, serde_json::Value)>> + Send + 'a>>;

    fn make_user_message(&self, text: &str) -> serde_json::Value;
    fn make_tool_results(&self, results: &[ToolResult]) -> Vec<serde_json::Value>;
}
