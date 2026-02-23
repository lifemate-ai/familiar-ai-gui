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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn config_with_platform(platform: &str) -> Config {
        Config {
            platform: platform.to_string(),
            api_key: "test_key".to_string(),
            ..Config::default()
        }
    }

    // ── create_backend factory ────────────────────────────────────

    #[test]
    fn create_backend_anthropic_uses_content_string() {
        let config = config_with_platform("anthropic");
        let backend = create_backend(&config);
        let msg = backend.make_user_message("hello");
        // Anthropic: {"role":"user","content":"hello"}
        assert_eq!(msg["role"], "user");
        assert_eq!(msg["content"], "hello");
    }

    #[test]
    fn create_backend_gemini_uses_parts_format() {
        let config = config_with_platform("gemini");
        let backend = create_backend(&config);
        let msg = backend.make_user_message("hello");
        // Gemini: {"role":"user","parts":[{"text":"hello"}]}
        assert_eq!(msg["role"], "user");
        assert!(msg.get("parts").is_some(), "Gemini should use 'parts' not 'content'");
        assert!(msg.get("content").is_none());
    }

    #[test]
    fn create_backend_openai_uses_content_string() {
        let config = config_with_platform("openai");
        let backend = create_backend(&config);
        let msg = backend.make_user_message("hello");
        assert_eq!(msg["role"], "user");
        assert_eq!(msg["content"], "hello");
    }

    #[test]
    fn create_backend_kimi_uses_content_string() {
        let config = config_with_platform("kimi");
        let backend = create_backend(&config);
        let msg = backend.make_user_message("hello");
        assert_eq!(msg["role"], "user");
        assert_eq!(msg["content"], "hello");
    }

    #[test]
    fn create_backend_unknown_defaults_to_kimi() {
        let config = config_with_platform("unknown_platform_xyz");
        let backend = create_backend(&config);
        let msg = backend.make_user_message("hello");
        // Kimi is default — same format as OpenAI: content string
        assert_eq!(msg["role"], "user");
        assert_eq!(msg["content"], "hello");
        // Gemini would use "parts", so absence of "parts" confirms it's not Gemini
        assert!(msg.get("parts").is_none());
    }

    #[test]
    fn create_backend_anthropic_tool_result_format() {
        let config = config_with_platform("anthropic");
        let backend = create_backend(&config);
        let results = vec![ToolResult {
            call_id: "id1".to_string(),
            text: "result".to_string(),
            image_b64: None,
        }];
        let msgs = backend.make_tool_results(&results);
        // Anthropic wraps all in a single user message with content array
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert!(msgs[0]["content"].as_array().is_some());
    }

    #[test]
    fn create_backend_openai_tool_result_format() {
        let config = config_with_platform("openai");
        let backend = create_backend(&config);
        let results = vec![ToolResult {
            call_id: "id1".to_string(),
            text: "result".to_string(),
            image_b64: None,
        }];
        let msgs = backend.make_tool_results(&results);
        // OpenAI: separate message per tool result with role="tool"
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "tool");
        assert_eq!(msgs[0]["tool_call_id"], "id1");
    }
}
