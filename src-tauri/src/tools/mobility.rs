/// Mobility tool — legs of the familiar (Tuya robot vacuum).
use anyhow::{bail, Result};
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

    fn base_url(&self) -> &str {
        match self.region.as_str() {
            "eu" => "https://openapi.tuyaeu.com",
            "in" => "https://openapi.tuyain.com",
            _ => "https://openapi.tuyaus.com",
        }
    }

    /// Step 1: Get a fresh access token from Tuya OpenAPI.
    async fn get_access_token(&self) -> Result<String> {
        let base = self.base_url();
        let path = "/v1.0/token?grant_type=1";
        let now = now_ms();

        // Token request signature: HMAC-SHA256(client_id + t + stringToSign)
        // stringToSign = METHOD\nContentHash\n\nURL
        let content_hash = sha256_hex(b"");
        let string_to_sign = format!("GET\n{content_hash}\n\n{path}");
        let message = format!("{}{}{}", self.api_key, now, string_to_sign);
        let sign = hmac_sha256(&self.api_secret, &message);

        let resp = self
            .client
            .get(format!("{base}{path}"))
            .header("client_id", &self.api_key)
            .header("t", now.to_string())
            .header("sign_method", "HMAC-SHA256")
            .header("sign", &sign)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        if resp["success"].as_bool() != Some(true) {
            bail!("Tuya token error: {resp}");
        }
        let token = resp["result"]["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no access_token in: {resp}"))?
            .to_string();
        Ok(token)
    }

    /// Step 2: Send a device command with the access token.
    async fn send_tuya_command(&self, command: &str) -> Result<()> {
        let access_token = self.get_access_token().await?;

        let base = self.base_url();
        let path = format!("/v1.0/devices/{}/commands", self.device_id);
        let body = json!({
            "commands": [{"code": "direction_control", "value": command}]
        });
        let body_str = body.to_string();
        let now = now_ms();

        // Authenticated request signature: HMAC-SHA256(client_id + access_token + t + stringToSign)
        let content_hash = sha256_hex(body_str.as_bytes());
        let string_to_sign = format!("POST\n{content_hash}\n\n{path}");
        let message = format!("{}{}{}{}", self.api_key, access_token, now, string_to_sign);
        let sign = hmac_sha256(&self.api_secret, &message);

        let resp = self
            .client
            .post(format!("{base}{path}"))
            .header("client_id", &self.api_key)
            .header("access_token", &access_token)
            .header("t", now.to_string())
            .header("sign_method", "HMAC-SHA256")
            .header("sign", &sign)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        if resp["success"].as_bool() != Some(true) {
            bail!("Tuya command error: {resp}");
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn sign_request(&self, method: &str, path: &str, body: &str, timestamp: u128) -> String {
        let content_hash = sha256_hex(body.as_bytes());
        let string_to_sign = format!("{method}\n{content_hash}\n\n{path}");
        let message = format!("{}{}{}", self.api_key, timestamp, string_to_sign);
        hmac_sha256(&self.api_secret, &message)
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// SHA-256 of `data`, returned as lowercase hex string.
fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}

/// HMAC-SHA256(key, data), returned as uppercase hex string (Tuya expects upper).
fn hmac_sha256(key: &str, data: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC accepts any key size");
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes()).to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_known_value_abc() {
        let result = sha256_hex(b"abc");
        assert_eq!(
            result,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_hex_empty_input() {
        let result = sha256_hex(b"");
        assert_eq!(
            result,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_hex_deterministic() {
        assert_eq!(sha256_hex(b"hello world"), sha256_hex(b"hello world"));
    }

    #[test]
    fn sha256_hex_different_inputs_differ() {
        assert_ne!(sha256_hex(b"hello"), sha256_hex(b"world"));
    }

    #[test]
    fn hmac_sha256_known_value() {
        let result = hmac_sha256("key", "The quick brown fox jumps over the lazy dog");
        assert_eq!(
            result,
            "F7BC83F430538424B13298E6AA6FB143EF4D59A14946175997479DBC2D1A3CD8"
        );
    }

    #[test]
    fn hmac_sha256_not_fake_signature() {
        let result = hmac_sha256("secret", "data");
        assert!(!result.starts_with("sig_"));
    }

    #[test]
    fn hmac_sha256_deterministic() {
        assert_eq!(hmac_sha256("key", "message"), hmac_sha256("key", "message"));
    }

    #[test]
    fn hmac_sha256_different_keys_differ() {
        assert_ne!(hmac_sha256("key1", "message"), hmac_sha256("key2", "message"));
    }

    #[test]
    fn hmac_sha256_different_data_differ() {
        assert_ne!(hmac_sha256("key", "message1"), hmac_sha256("key", "message2"));
    }

    #[test]
    fn hmac_sha256_returns_64_hex_chars() {
        assert_eq!(hmac_sha256("key", "data").len(), 64);
    }

    #[test]
    fn sign_request_is_deterministic() {
        let tool = MobilityTool::new(
            "us".to_string(),
            "test_key".to_string(),
            "test_secret".to_string(),
            "device123".to_string(),
        );
        let sig1 = tool.sign_request("POST", "/v1.0/test", "{}", 1700000000000);
        let sig2 = tool.sign_request("POST", "/v1.0/test", "{}", 1700000000000);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn sign_request_not_fake() {
        let tool = MobilityTool::new(
            "us".to_string(),
            "key".to_string(),
            "secret".to_string(),
            "dev".to_string(),
        );
        let sig = tool.sign_request("POST", "/path", "body", 12345);
        assert!(!sig.starts_with("sig_"));
        assert_eq!(sig.len(), 64);
    }

    #[test]
    fn sign_request_changes_with_timestamp() {
        let tool = MobilityTool::new(
            "us".to_string(),
            "key".to_string(),
            "secret".to_string(),
            "dev".to_string(),
        );
        assert_ne!(
            tool.sign_request("POST", "/path", "body", 1000),
            tool.sign_request("POST", "/path", "body", 2000)
        );
    }

    #[test]
    fn is_configured_empty_api_key() {
        let tool = MobilityTool::new("us".to_string(), "".to_string(), "s".to_string(), "d".to_string());
        assert!(!tool.is_configured());
    }

    #[test]
    fn is_configured_empty_device_id() {
        let tool = MobilityTool::new("us".to_string(), "k".to_string(), "s".to_string(), "".to_string());
        assert!(!tool.is_configured());
    }

    #[test]
    fn is_configured_with_all_values() {
        let tool = MobilityTool::new("us".to_string(), "k".to_string(), "s".to_string(), "d".to_string());
        assert!(tool.is_configured());
    }

    #[test]
    fn command_code_all_directions() {
        let defs = MobilityTool::tool_defs();
        let enum_vals = &defs[0].input_schema["properties"]["direction"]["enum"];
        let vals: Vec<_> = enum_vals.as_array().unwrap().iter().collect();
        assert!(vals.iter().any(|v| *v == "forward"));
        assert!(vals.iter().any(|v| *v == "backward"));
        assert!(vals.iter().any(|v| *v == "left"));
        assert!(vals.iter().any(|v| *v == "right"));
        assert!(vals.iter().any(|v| *v == "stop"));
    }

    #[test]
    fn tool_def_direction_is_required() {
        let defs = MobilityTool::tool_defs();
        let required = defs[0].input_schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "direction"));
    }

    #[test]
    fn tool_def_name_is_walk() {
        assert_eq!(MobilityTool::tool_defs()[0].name, "walk");
    }
}
