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

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> GeminiBackend {
        GeminiBackend::new("test_key".to_string(), "gemini-2.5-flash".to_string())
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
    fn user_message_uses_parts_not_content() {
        let msg = backend().make_user_message("hello");
        assert!(msg.get("parts").is_some(), "Gemini uses 'parts', not 'content'");
        assert!(msg.get("content").is_none());
    }

    #[test]
    fn user_message_text_in_parts_array() {
        let msg = backend().make_user_message("test text");
        let parts = msg["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["text"], "test text");
    }

    #[test]
    fn user_message_empty_text() {
        let msg = backend().make_user_message("");
        let parts = msg["parts"].as_array().unwrap();
        assert_eq!(parts[0]["text"], "");
    }

    // ── make_tool_results ─────────────────────────────────────────

    #[test]
    fn tool_results_wrapped_in_user_message() {
        let results = vec![tool_result("id1", "result", None)];
        let msgs = backend().make_tool_results(&results);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn tool_result_uses_function_response_format() {
        let results = vec![tool_result("my_tool", "output text", None)];
        let msgs = backend().make_tool_results(&results);
        let parts = msgs[0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert!(parts[0].get("functionResponse").is_some());
    }

    #[test]
    fn tool_result_function_response_has_name_and_result() {
        let results = vec![tool_result("search_tool", "found it", None)];
        let msgs = backend().make_tool_results(&results);
        let fr = &msgs[0]["parts"][0]["functionResponse"];
        assert_eq!(fr["name"], "search_tool");
        assert_eq!(fr["response"]["result"], "found it");
    }

    #[test]
    fn tool_result_with_image_adds_inline_data_part() {
        let results = vec![tool_result("cam", "snap", Some("base64abc"))];
        let msgs = backend().make_tool_results(&results);
        let parts = msgs[0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert!(parts[1].get("inlineData").is_some());
        assert_eq!(parts[1]["inlineData"]["mimeType"], "image/jpeg");
        assert_eq!(parts[1]["inlineData"]["data"], "base64abc");
    }

    #[test]
    fn multiple_tool_results_all_in_one_user_message() {
        let results = vec![
            tool_result("tool1", "result1", None),
            tool_result("tool2", "result2", None),
        ];
        let msgs = backend().make_tool_results(&results);
        assert_eq!(msgs.len(), 1, "All results should be in one user message");
        let parts = msgs[0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn empty_tool_results_returns_empty_parts_user_message() {
        let msgs = backend().make_tool_results(&[]);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        let parts = msgs[0]["parts"].as_array().unwrap();
        assert!(parts.is_empty());
    }

    // ── convert_history ───────────────────────────────────────────

    #[test]
    fn convert_history_assistant_maps_to_model_role() {
        let history = vec![json!({"role": "assistant", "content": "hello"})];
        let converted = GeminiBackend::convert_history(&history);
        assert_eq!(converted[0]["role"], "model");
    }

    #[test]
    fn convert_history_model_role_stays_model() {
        let history = vec![json!({"role": "model", "parts": [{"text": "hi"}]})];
        let converted = GeminiBackend::convert_history(&history);
        assert_eq!(converted[0]["role"], "model");
    }

    #[test]
    fn convert_history_user_role_stays_user() {
        let history = vec![json!({"role": "user", "content": "hello"})];
        let converted = GeminiBackend::convert_history(&history);
        assert_eq!(converted[0]["role"], "user");
    }

    #[test]
    fn convert_history_string_content_becomes_text_part() {
        let history = vec![json!({"role": "user", "content": "hello world"})];
        let converted = GeminiBackend::convert_history(&history);
        let parts = converted[0]["parts"].as_array().unwrap();
        assert_eq!(parts[0]["text"], "hello world");
    }

    #[test]
    fn convert_history_empty_parts_message_is_skipped() {
        // A message with only tool_result content blocks (filtered out) should be skipped
        let history = vec![json!({"role": "user", "content": [{"type": "tool_result"}]})];
        let converted = GeminiBackend::convert_history(&history);
        assert!(converted.is_empty(), "Message with no usable parts should be skipped");
    }

    // ── convert_tools ─────────────────────────────────────────────

    #[test]
    fn convert_tools_wraps_in_function_declarations() {
        let tool = ToolDef {
            name: "search".to_string(),
            description: "Search things".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let converted = GeminiBackend::convert_tools(&[tool]);
        assert_eq!(converted.len(), 1);
        assert!(converted[0].get("functionDeclarations").is_some());
    }

    #[test]
    fn convert_tools_declaration_has_name_and_parameters() {
        let tool = ToolDef {
            name: "my_tool".to_string(),
            description: "desc".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let converted = GeminiBackend::convert_tools(&[tool]);
        let decls = converted[0]["functionDeclarations"].as_array().unwrap();
        assert_eq!(decls[0]["name"], "my_tool");
        assert!(decls[0].get("parameters").is_some());
    }

    #[test]
    fn convert_tools_empty_returns_one_empty_declarations_wrapper() {
        let converted = GeminiBackend::convert_tools(&[]);
        assert_eq!(converted.len(), 1);
        let decls = converted[0]["functionDeclarations"].as_array().unwrap();
        assert!(decls.is_empty());
    }
}
