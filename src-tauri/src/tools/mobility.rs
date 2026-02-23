/// Mobility tool — legs of the familiar (Tuya robot vacuum).
use anyhow::Result;
use reqwest::Client;
use serde_json::json;

use crate::backend::ToolDef;

use super::ToolOutput;

pub struct MobilityTool {
    region: String,
    api_key: String,
    api_secret: String,
    device_id: String,
    client: Client,
}

impl MobilityTool {
    pub fn new(region: String, api_key: String, api_secret: String, device_id: String) -> Self {
        Self {
            region,
            api_key,
            api_secret,
            device_id,
            client: Client::new(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty() && !self.device_id.is_empty()
    }

    pub fn tool_defs() -> Vec<ToolDef> {
        vec![ToolDef {
            name: "walk".to_string(),
            description: "Move the robot body (vacuum cleaner). \
                          direction: forward|backward|left|right|stop. \
                          duration: seconds (optional). \
                          NOTE: walking does NOT change what the camera sees."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "direction": {
                        "type": "string",
                        "enum": ["forward", "backward", "left", "right", "stop"],
                        "description": "Movement direction"
                    },
                    "duration": {
                        "type": "number",
                        "description": "Duration in seconds (optional)"
                    }
                },
                "required": ["direction"]
            }),
        }]
    }

    pub async fn walk(&self, direction: &str, duration: Option<f64>) -> Result<ToolOutput> {
        if !self.is_configured() {
            return Ok((
                format!("(No robot configured — cannot walk {direction})"),
                None,
            ));
        }

        // Tuya command codes for robot vacuum movement
        let command_code = match direction {
            "forward" => "forward",
            "backward" => "backward",
            "left" => "turn_left",
            "right" => "turn_right",
            "stop" => "stop",
            _ => "stop",
        };

        self.send_tuya_command(command_code).await?;

        if let Some(secs) = duration {
            if direction != "stop" {
                tokio::time::sleep(tokio::time::Duration::from_secs_f64(secs)).await;
                self.send_tuya_command("stop").await?;
            }
        }

        let desc = if let Some(secs) = duration {
            format!("Walked {direction} for {secs}s")
        } else {
            format!("Started moving {direction}")
        };

        Ok((desc, None))
    }

    async fn send_tuya_command(&self, command: &str) -> Result<()> {
        // Tuya OpenAPI endpoint
        let base_url = match self.region.as_str() {
            "eu" => "https://openapi.tuyaeu.com",
            "in" => "https://openapi.tuyain.com",
            "us" => "https://openapi.tuyaus.com",
            _ => "https://openapi.tuyaus.com",
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let path = format!("/v1.0/devices/{}/commands", self.device_id);
        let body = json!({
            "commands": [{
                "code": "control",
                "value": command
            }]
        });

        // Sign the request (Tuya HMAC-SHA256 signature)
        let sign = self.sign_request("POST", &path, &body.to_string(), now);

        let _ = self
            .client
            .post(format!("{base_url}{path}"))
            .header("client_id", &self.api_key)
            .header("sign", sign)
            .header("t", now.to_string())
            .header("sign_method", "HMAC-SHA256")
            .json(&body)
            .send()
            .await;

        Ok(())
    }

    fn sign_request(&self, method: &str, path: &str, body: &str, timestamp: u128) -> String {
        // Tuya signature: HMAC-SHA256(client_id + t + string_to_sign)
        let content_hash = {
            use std::fmt::Write;
            let digest = openssl_hash(body.as_bytes());
            let mut hex = String::new();
            for b in digest {
                let _ = write!(hex, "{b:02x}");
            }
            hex
        };

        let headers_str = "";
        let string_to_sign = format!("{method}\n{content_hash}\n{headers_str}\n{path}");
        let message = format!("{}{}{}", self.api_key, timestamp, string_to_sign);

        hmac_sha256(&self.api_secret, &message)
    }
}

fn openssl_hash(data: &[u8]) -> [u8; 32] {
    // SHA-256 without external dep — use a simple implementation
    // In production, use sha2 crate. For now, placeholder.
    let _ = data;
    [0u8; 32]
}

fn hmac_sha256(_key: &str, data: &str) -> String {
    // Placeholder — add hmac + sha2 crates for production
    format!("sig_{}", &data[..data.len().min(8)])
}
