/// Anthropic Messages API backend (Claude)
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};

use super::{LlmBackendDyn, StopReason, TextCallback, ToolCall, ToolDef, ToolResult, TurnResult};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MAX_TOKENS: u32 = 4096;
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicBackend {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicBackend {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
        }
    }

    fn convert_tools(tools: &[ToolDef]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect()
    }
}

impl LlmBackendDyn for AnthropicBackend {
    fn stream_turn_dyn<'a>(
        &'a self,
        system: &'a str,
        history: &'a [Value],
        tools: &'a [ToolDef],
        on_text: TextCallback,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(TurnResult, Value)>> + Send + 'a>> {
        Box::pin(async move {
            let body = json!({
                "model": self.model,
                "max_tokens": MAX_TOKENS,
                "system": system,
                "tools": Self::convert_tools(tools),
                "messages": history,
                "stream": true,
            });

            let resp = self
                .client
                .post(API_URL)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Anthropic API error {status}: {text}");
            }

            let body_bytes = resp.bytes().await?;
            let body_str = String::from_utf8_lossy(&body_bytes);

            let mut text_chunks = Vec::new();
            let mut tool_calls = Vec::new();
            let mut raw_content = Vec::new();
            let mut stop_reason_str = String::new();

            // Parse Anthropic SSE: event types are content_block_delta, message_stop, etc.
            let mut current_tool_idx: Option<usize> = None;

            for line in body_str.lines() {
                let Some(data) = line.strip_prefix("data: ") else {
                    continue;
                };
                let Ok(chunk): Result<Value, _> = serde_json::from_str(data) else {
                    continue;
                };

                match chunk["type"].as_str().unwrap_or("") {
                    "content_block_start" => {
                        let block = &chunk["content_block"];
                        match block["type"].as_str().unwrap_or("") {
                            "tool_use" => {
                                let idx = chunk["index"].as_u64().unwrap_or(0) as usize;
                                current_tool_idx = Some(tool_calls.len());
                                tool_calls.push(ToolCall {
                                    id: block["id"].as_str().unwrap_or("").to_string(),
                                    name: block["name"].as_str().unwrap_or("").to_string(),
                                    input: json!(""),
                                });
                                let _ = idx;
                            }
                            _ => {
                                current_tool_idx = None;
                            }
                        }
                    }
                    "content_block_delta" => {
                        let delta = &chunk["delta"];
                        match delta["type"].as_str().unwrap_or("") {
                            "text_delta" => {
                                if let Some(t) = delta["text"].as_str() {
                                    text_chunks.push(t.to_string());
                                    on_text(t.to_string());
                                }
                            }
                            "input_json_delta" => {
                                if let Some(idx) = current_tool_idx {
                                    if let Some(partial) = delta["partial_json"].as_str() {
                                        // Accumulate JSON string; parse at block_stop
                                        if let Value::String(s) = &mut tool_calls[idx].input {
                                            s.push_str(partial);
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    "content_block_stop" => {
                        // Parse accumulated input JSON for tool calls
                        if let Some(idx) = current_tool_idx {
                            let raw_json = if let Value::String(s) = &tool_calls[idx].input {
                                s.clone()
                            } else {
                                String::new()
                            };
                            tool_calls[idx].input =
                                serde_json::from_str(&raw_json).unwrap_or(Value::Object(Default::default()));
                        }
                        current_tool_idx = None;
                    }
                    "message_delta" => {
                        if let Some(sr) = chunk["delta"]["stop_reason"].as_str() {
                            stop_reason_str = sr.to_string();
                        }
                    }
                    _ => {}
                }
            }

            let text = text_chunks.join("");

            // Build Anthropic-format raw content blocks
            if !text.is_empty() {
                raw_content.push(json!({"type": "text", "text": text}));
            }
            for tc in &tool_calls {
                raw_content.push(json!({
                    "type": "tool_use",
                    "id": tc.id,
                    "name": tc.name,
                    "input": tc.input,
                }));
            }

            let stop_reason = if stop_reason_str == "tool_use" {
                StopReason::ToolUse
            } else {
                StopReason::EndTurn
            };

            let raw_assistant = json!({
                "role": "assistant",
                "content": raw_content,
            });

            Ok((
                TurnResult {
                    stop_reason,
                    text,
                    tool_calls,
                },
                raw_assistant,
            ))
        })
    }

    fn make_user_message(&self, text: &str) -> Value {
        json!({"role": "user", "content": text})
    }

    fn make_tool_results(&self, results: &[ToolResult]) -> Vec<Value> {
        let mut content = Vec::new();
        for r in results {
            let mut result_content: Vec<Value> = vec![json!({"type": "text", "text": r.text})];
            if let Some(img) = &r.image_b64 {
                result_content.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/jpeg",
                        "data": img,
                    }
                }));
            }
            content.push(json!({
                "type": "tool_result",
                "tool_use_id": r.call_id,
                "content": result_content,
            }));
        }
        vec![json!({"role": "user", "content": content})]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> AnthropicBackend {
        AnthropicBackend::new("test_key".to_string(), "claude-3-5-sonnet-20241022".to_string())
    }

    fn tool_result(id: &str, text: &str, image: Option<&str>) -> ToolResult {
        ToolResult {
            call_id: id.to_string(),
            text: text.to_string(),
            image_b64: image.map(|s| s.to_string()),
        }
    }

    // ── make_user_message ─────────────────────────────────────────

    #[test]
    fn user_message_role_is_user() {
        let msg = backend().make_user_message("hello");
        assert_eq!(msg["role"], "user");
    }

    #[test]
    fn user_message_content_equals_text() {
        let msg = backend().make_user_message("hello world");
        assert_eq!(msg["content"], "hello world");
    }

    #[test]
    fn user_message_empty_text() {
        let msg = backend().make_user_message("");
        assert_eq!(msg["content"], "");
    }

    // ── make_tool_results ─────────────────────────────────────────

    #[test]
    fn tool_results_wrapped_in_user_message() {
        let results = vec![tool_result("id1", "result text", None)];
        let msgs = backend().make_tool_results(&results);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn tool_result_content_has_tool_result_type() {
        let results = vec![tool_result("id1", "text", None)];
        let msgs = backend().make_tool_results(&results);
        let content = &msgs[0]["content"][0];
        assert_eq!(content["type"], "tool_result");
        assert_eq!(content["tool_use_id"], "id1");
    }

    #[test]
    fn tool_result_without_image_has_only_text_content() {
        let results = vec![tool_result("id1", "only text", None)];
        let msgs = backend().make_tool_results(&results);
        let content_arr = msgs[0]["content"][0]["content"].as_array().unwrap();
        assert_eq!(content_arr.len(), 1);
        assert_eq!(content_arr[0]["type"], "text");
        assert_eq!(content_arr[0]["text"], "only text");
    }

    #[test]
    fn tool_result_with_image_has_text_and_image() {
        let results = vec![tool_result("id1", "text", Some("base64data"))];
        let msgs = backend().make_tool_results(&results);
        let content_arr = msgs[0]["content"][0]["content"].as_array().unwrap();
        assert_eq!(content_arr.len(), 2);
        assert_eq!(content_arr[0]["type"], "text");
        assert_eq!(content_arr[1]["type"], "image");
        assert_eq!(content_arr[1]["source"]["type"], "base64");
        assert_eq!(content_arr[1]["source"]["media_type"], "image/jpeg");
        assert_eq!(content_arr[1]["source"]["data"], "base64data");
    }

    #[test]
    fn multiple_tool_results_all_included() {
        let results = vec![
            tool_result("id1", "first", None),
            tool_result("id2", "second", None),
        ];
        let msgs = backend().make_tool_results(&results);
        // Both wrapped in single user message
        assert_eq!(msgs.len(), 1);
        let content_arr = msgs[0]["content"].as_array().unwrap();
        assert_eq!(content_arr.len(), 2);
    }

    #[test]
    fn empty_tool_results_returns_single_empty_user_message() {
        let msgs = backend().make_tool_results(&[]);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    // ── convert_tools ─────────────────────────────────────────────

    #[test]
    fn convert_tools_uses_input_schema_key() {
        let tool = ToolDef {
            name: "test".to_string(),
            description: "desc".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let converted = AnthropicBackend::convert_tools(&[tool]);
        assert_eq!(converted[0]["name"], "test");
        assert_eq!(converted[0]["description"], "desc");
        assert!(converted[0].get("input_schema").is_some());
        // Anthropic uses "input_schema" not "parameters"
        assert!(converted[0].get("parameters").is_none());
    }
}
