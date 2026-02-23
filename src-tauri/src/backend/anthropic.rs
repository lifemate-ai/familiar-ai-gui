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
