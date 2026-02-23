/// Camera tool — eyes and neck of the familiar.
/// Snapshot via RTSP + ffmpeg subprocess, PTZ via ONVIF SOAP over reqwest.
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::Client;
use serde_json::json;
use tokio::process::Command;

use crate::backend::ToolDef;

use super::ToolOutput;

pub struct CameraTool {
    host: String,
    username: String,
    password: String,
    onvif_port: u16,
    client: Client,
}

impl CameraTool {
    pub fn new(host: String, username: String, password: String, onvif_port: u16) -> Self {
        Self {
            host,
            username,
            password,
            onvif_port,
            client: Client::new(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.host.is_empty()
    }

    pub fn tool_defs() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "see".to_string(),
                description: "Take a photo with your camera (your eyes). Call this after looking around to actually see what is there.".to_string(),
                input_schema: json!({"type": "object", "properties": {}, "required": []}),
            },
            ToolDef {
                name: "look".to_string(),
                description: "Move your camera neck. direction: left|right|up|down|around. degrees: how far (default 30).".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "direction": {
                            "type": "string",
                            "enum": ["left", "right", "up", "down", "around"],
                            "description": "Direction to look"
                        },
                        "degrees": {
                            "type": "integer",
                            "description": "How far in degrees (1-90, default 30)",
                            "default": 30
                        }
                    },
                    "required": ["direction"]
                }),
            },
        ]
    }

    /// Capture a JPEG snapshot via RTSP+ffmpeg. Returns (description, Some(base64_jpeg)).
    pub async fn capture(&self) -> Result<ToolOutput> {
        if !self.is_configured() {
            return Ok(("(No camera configured)".to_string(), None));
        }

        let stream_url = format!(
            "rtsp://{}:{}@{}:554/stream1",
            self.username, self.password, self.host
        );

        let tmp = std::env::temp_dir().join(format!(
            "familiar_cap_{}.jpg",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));

        let output = Command::new("ffmpeg")
            .args([
                "-rtsp_transport", "tcp",
                "-i", &stream_url,
                "-vframes", "1",
                "-q:v", "3",
                "-y",
                tmp.to_str().unwrap_or("/tmp/familiar_cap.jpg"),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok((format!("Camera capture failed: {stderr}"), None));
        }

        let bytes = tokio::fs::read(&tmp).await?;
        let b64 = B64.encode(&bytes);
        let _ = tokio::fs::remove_file(&tmp).await;

        Ok(("(Camera image captured)".to_string(), Some(b64)))
    }

    /// Move PTZ camera via ONVIF ContinuousMove + stop.
    pub async fn look(&self, direction: &str, degrees: u32) -> Result<ToolOutput> {
        if !self.is_configured() {
            return Ok((format!("(No camera — cannot look {direction})"), None));
        }

        if direction == "around" {
            // look_around: capture center, left, right, up — return combined message
            self.ptz_move(0.0, 0.0).await?; // reset first
            let desc = "Looking around...".to_string();
            // For now, just capture after a brief pause
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            return Ok((desc, None));
        }

        let duration_ms = (degrees as f64 / 90.0 * 1500.0) as u64;
        let (pan, tilt) = match direction {
            "left" => (-0.5_f32, 0.0_f32),
            "right" => (0.5, 0.0),
            "up" => (0.0, 0.5),
            "down" => (0.0, -0.5),
            _ => (0.0, 0.0),
        };

        self.ptz_move(pan, tilt).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(duration_ms)).await;
        self.ptz_stop().await?;

        let desc = match direction {
            "left" => format!("Turned left {degrees}°"),
            "right" => format!("Turned right {degrees}°"),
            "up" => format!("Tilted up {degrees}°"),
            "down" => format!("Tilted down {degrees}°"),
            _ => format!("Moved {direction}"),
        };

        Ok((desc, None))
    }

    /// Send ONVIF ContinuousMove SOAP request.
    async fn ptz_move(&self, pan: f32, tilt: f32) -> Result<()> {
        let soap = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"
            xmlns:ptz="http://www.onvif.org/ver20/ptz/wsdl"
            xmlns:tt="http://www.onvif.org/ver10/schema">
  <s:Body>
    <ptz:ContinuousMove>
      <ptz:ProfileToken>Profile_1</ptz:ProfileToken>
      <ptz:Velocity>
        <tt:PanTilt x="{pan}" y="{tilt}"/>
      </ptz:Velocity>
    </ptz:ContinuousMove>
  </s:Body>
</s:Envelope>"#
        );

        let url = format!("http://{}:{}/onvif/PTZ", self.host, self.onvif_port);
        let _ = self
            .client
            .post(&url)
            .header("Content-Type", "application/soap+xml")
            .basic_auth(&self.username, Some(&self.password))
            .body(soap)
            .send()
            .await;

        Ok(())
    }

    async fn ptz_stop(&self) -> Result<()> {
        let soap = r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"
            xmlns:ptz="http://www.onvif.org/ver20/ptz/wsdl">
  <s:Body>
    <ptz:Stop>
      <ptz:ProfileToken>Profile_1</ptz:ProfileToken>
      <ptz:PanTilt>true</ptz:PanTilt>
      <ptz:Zoom>false</ptz:Zoom>
    </ptz:Stop>
  </s:Body>
</s:Envelope>"#;

        let url = format!("http://{}:{}/onvif/PTZ", self.host, self.onvif_port);
        let _ = self
            .client
            .post(&url)
            .header("Content-Type", "application/soap+xml")
            .basic_auth(&self.username, Some(&self.password))
            .body(soap)
            .send()
            .await;

        Ok(())
    }
}
