/// Google Gemini API backend (native, not OpenAI-compat)
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{LlmBackendDyn, StopReason, TextCallback, ToolCall, ToolDef, ToolResult, TurnResult};

const MAX_TOKENS: u32 = 4096;

pub struct GeminiBackend {
    client: Client,
    api_key: String,
    model: String,
}

impl GeminiBackend {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
        }
    }

    fn api_url(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.model, self.api_key
        )
    }

    fn convert_tools(tools: &[ToolDef]) -> Vec<Value> {
        let declarations: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })
            })
            .collect();
        vec![json!({"functionDeclarations": declarations})]
    }

    /// Convert OpenAI-style history to Gemini contents format.
    fn convert_history(history: &[Value]) -> Vec<Value> {
        let mut contents = Vec::new();
        for msg in history {
            let role = msg["role"].as_str().unwrap_or("user");
            let gemini_role = match role {
                "assistant" | "model" => "model",
                _ => "user",
            };

            // Handle content as string or array
            let parts = if let Some(text) = msg["content"].as_str() {
                vec![json!({"text": text})]
            } else if let Some(arr) = msg["content"].as_array() {
                arr.iter()
                    .filter_map(|item| {
                        if let Some(t) = item["text"].as_str() {
                            Some(json!({"text": t}))
                        } else if item["type"] == "image_url" {
                            let url = item["image_url"]["url"].as_str().unwrap_or("");
                            let b64 = url.strip_prefix("data:image/jpeg;base64,").unwrap_or("");
                            Some(json!({
                                "inlineData": {
                                    "mimeType": "image/jpeg",
                                    "data": b64,
                                }
                            }))
                        } else if item["type"] == "tool_result" {
                            // tool results handled separately below
                            None
                        } else {
                            None
                        }
                    })
                    .collect()
            } else if let Some(parts_arr) = msg["parts"].as_array() {
                parts_arr.to_vec()
            } else {
                vec![]
            };

            if !parts.is_empty() {
                contents.push(json!({"role": gemini_role, "parts": parts}));
            }
        }
        contents
    }
}

impl LlmBackendDyn for GeminiBackend {
    fn stream_turn_dyn<'a>(
        &'a self,
        system: &'a str,
        history: &'a [Value],
        tools: &'a [ToolDef],
        on_text: TextCallback,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(TurnResult, Value)>> + Send + 'a>> {
        Box::pin(async move {
            let contents = Self::convert_history(history);
            let gemini_tools = Self::convert_tools(tools);

            let body = json!({
                "systemInstruction": {"parts": [{"text": system}]},
                "contents": contents,
                "tools": gemini_tools,
                "generationConfig": {
                    "maxOutputTokens": MAX_TOKENS,
                    "thinkingConfig": {"thinkingBudget": 0},
                }
            });

            let resp = self
                .client
                .post(self.api_url())
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Gemini API error {status}: {text}");
            }

            let body_bytes = resp.bytes().await?;
            let body_str = String::from_utf8_lossy(&body_bytes);

            let mut text_chunks = Vec::new();
            let mut tool_calls = Vec::new();
            let mut raw_parts: Vec<Value> = Vec::new();

            for line in body_str.lines() {
                let Some(data) = line.strip_prefix("data: ") else {
                    continue;
                };
                let Ok(chunk): Result<Value, _> = serde_json::from_str(data) else {
                    continue;
                };

                let candidate = &chunk["candidates"][0];
                if let Some(parts) = candidate["content"]["parts"].as_array() {
                    for part in parts {
                        raw_parts.push(part.clone());
                        if let Some(text) = part["text"].as_str() {
                            text_chunks.push(text.to_string());
                            on_text(text.to_string());
                        }
                        if let Some(fc) = part["functionCall"].as_object() {
                            let name = fc["name"].as_str().unwrap_or("").to_string();
                            let args = fc
                                .get("args")
                                .cloned()
                                .unwrap_or(Value::Object(Default::default()));
                            tool_calls.push(ToolCall {
                                id: format!("call_{}", Uuid::new_v4().simple()),
                                name,
                                input: args,
                            });
                        }
                    }
                }
            }

            let text = text_chunks.join("");
            let stop_reason = if tool_calls.is_empty() {
                StopReason::EndTurn
            } else {
                StopReason::ToolUse
            };

            let raw_assistant = json!({
                "role": "model",
                "parts": raw_parts,
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
        json!({"role": "user", "parts": [{"text": text}]})
    }

    fn make_tool_results(&self, results: &[ToolResult]) -> Vec<Value> {
        let mut parts = Vec::new();
        for r in results {
            parts.push(json!({
                "functionResponse": {
                    "name": r.call_id,
                    "response": {"result": r.text}
                }
            }));
            if let Some(img) = &r.image_b64 {
                parts.push(json!({
                    "inlineData": {
                        "mimeType": "image/jpeg",
                        "data": img,
                    }
                }));
            }
        }
        vec![json!({"role": "user", "parts": parts})]
    }
}
