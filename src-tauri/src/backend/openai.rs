/// OpenAI API backend
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

use super::{LlmBackendDyn, StopReason, TextCallback, ToolCall, ToolDef, ToolResult, TurnResult};

const BASE_URL: &str = "https://api.openai.com/v1";
const MAX_TOKENS: u32 = 4096;

pub struct OpenAiBackend {
    client: Client,
    api_key: String,
    model: String,
}

impl OpenAiBackend {
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
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                })
            })
            .collect()
    }
}

impl LlmBackendDyn for OpenAiBackend {
    fn stream_turn_dyn<'a>(
        &'a self,
        system: &'a str,
        history: &'a [Value],
        tools: &'a [ToolDef],
        on_text: TextCallback,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(TurnResult, Value)>> + Send + 'a>> {
        Box::pin(async move {
            let mut messages = vec![json!({"role": "system", "content": system})];
            messages.extend_from_slice(history);

            let oai_tools = Self::convert_tools(tools);

            let mut body = json!({
                "model": self.model,
                "max_completion_tokens": MAX_TOKENS,
                "messages": messages,
                "stream": true,
            });
            if !oai_tools.is_empty() {
                body["tools"] = json!(oai_tools);
            }

            let resp = self
                .client
                .post(format!("{BASE_URL}/chat/completions"))
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenAI API error {status}: {text}");
            }

            let body_bytes = resp.bytes().await?;
            let body_str = String::from_utf8_lossy(&body_bytes);

            let mut text_chunks = Vec::new();
            let mut raw_tcs: std::collections::HashMap<usize, (String, String, String)> =
                std::collections::HashMap::new();
            let mut finish_reason = String::new();

            for line in body_str.lines() {
                if line == "data: [DONE]" {
                    break;
                }
                let Some(data) = line.strip_prefix("data: ") else {
                    continue;
                };
                let Ok(chunk): Result<Value, _> = serde_json::from_str(data) else {
                    continue;
                };

                let choice = &chunk["choices"][0];
                if let Some(fr) = choice["finish_reason"].as_str() {
                    finish_reason = fr.to_string();
                }
                let delta = &choice["delta"];

                if let Some(content) = delta["content"].as_str() {
                    text_chunks.push(content.to_string());
                    on_text(content.to_string());
                }

                if let Some(tc_array) = delta["tool_calls"].as_array() {
                    for tc_delta in tc_array {
                        let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                        let entry = raw_tcs
                            .entry(idx)
                            .or_insert_with(|| (String::new(), String::new(), String::new()));
                        if let Some(id) = tc_delta["id"].as_str() {
                            entry.0 = id.to_string();
                        }
                        if let Some(name) = tc_delta["function"]["name"].as_str() {
                            entry.1 = name.to_string();
                        }
                        if let Some(args) = tc_delta["function"]["arguments"].as_str() {
                            entry.2.push_str(args);
                        }
                    }
                }
            }

            let text = text_chunks.join("");

            let mut tool_calls = Vec::new();
            let mut sorted_idx: Vec<usize> = raw_tcs.keys().cloned().collect();
            sorted_idx.sort();
            for idx in sorted_idx {
                let (id, name, arguments) = &raw_tcs[&idx];
                let input: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
                tool_calls.push(ToolCall {
                    id: if id.is_empty() {
                        format!("call_{}", Uuid::new_v4().simple())
                    } else {
                        id.clone()
                    },
                    name: name.clone(),
                    input,
                });
            }

            let stop_reason = if finish_reason == "tool_calls" {
                StopReason::ToolUse
            } else {
                StopReason::EndTurn
            };

            let mut raw_assistant = json!({
                "role": "assistant",
                "content": if text.is_empty() { Value::Null } else { json!(text) },
            });
            if !tool_calls.is_empty() {
                raw_assistant["tool_calls"] = json!(tool_calls
                    .iter()
                    .map(|tc| json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": serde_json::to_string(&tc.input).unwrap_or_default(),
                        }
                    }))
                    .collect::<Vec<_>>());
            }

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
        let mut msgs = Vec::new();
        for r in results {
            msgs.push(json!({
                "role": "tool",
                "tool_call_id": r.call_id,
                "content": r.text,
            }));
            if let Some(img) = &r.image_b64 {
                msgs.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "image_url",
                        "image_url": {"url": format!("data:image/jpeg;base64,{img}")}
                    }]
                }));
            }
        }
        msgs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> OpenAiBackend {
        OpenAiBackend::new("test_key".to_string(), "gpt-4o".to_string())
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
    fn user_message_content_is_string() {
        let msg = backend().make_user_message("test content");
        assert_eq!(msg["content"], "test content");
    }

    // ── make_tool_results ─────────────────────────────────────────

    #[test]
    fn tool_result_role_is_tool() {
        let results = vec![tool_result("id1", "text", None)];
        let msgs = backend().make_tool_results(&results);
        assert_eq!(msgs[0]["role"], "tool");
    }

    #[test]
    fn tool_result_has_tool_call_id() {
        let results = vec![tool_result("call_xyz", "text", None)];
        let msgs = backend().make_tool_results(&results);
        assert_eq!(msgs[0]["tool_call_id"], "call_xyz");
    }

    #[test]
    fn tool_result_content_equals_text() {
        let results = vec![tool_result("id1", "the result", None)];
        let msgs = backend().make_tool_results(&results);
        assert_eq!(msgs[0]["content"], "the result");
    }

    #[test]
    fn tool_result_without_image_is_single_message() {
        let results = vec![tool_result("id1", "text", None)];
        let msgs = backend().make_tool_results(&results);
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn tool_result_with_image_adds_user_message() {
        let results = vec![tool_result("id1", "text", Some("b64data"))];
        let msgs = backend().make_tool_results(&results);
        assert_eq!(msgs.len(), 2, "Should have tool msg + image msg");
        assert_eq!(msgs[1]["role"], "user");
        let url = &msgs[1]["content"][0]["image_url"]["url"];
        assert!(url.as_str().unwrap().contains("b64data"));
        assert!(url.as_str().unwrap().starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn multiple_tool_results_each_get_tool_message() {
        let results = vec![
            tool_result("id1", "first", None),
            tool_result("id2", "second", None),
        ];
        let msgs = backend().make_tool_results(&results);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["tool_call_id"], "id1");
        assert_eq!(msgs[1]["tool_call_id"], "id2");
    }

    // ── convert_tools ─────────────────────────────────────────────

    #[test]
    fn convert_tools_uses_function_wrapper() {
        let tool = ToolDef {
            name: "search".to_string(),
            description: "Search something".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let converted = OpenAiBackend::convert_tools(&[tool]);
        assert_eq!(converted[0]["type"], "function");
        assert_eq!(converted[0]["function"]["name"], "search");
        // OpenAI uses "parameters" not "input_schema"
        assert!(converted[0]["function"].get("parameters").is_some());
    }
}
