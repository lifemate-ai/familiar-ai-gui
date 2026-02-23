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
                "-rtsp_transport",
                "tcp",
                "-i",
                &stream_url,
                "-vframes",
                "1",
                "-q:v",
                "3",
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

    /// Move PTZ camera via ONVIF RelativeMove (same as Python version).
    pub async fn look(&self, direction: &str, degrees: u32) -> Result<ToolOutput> {
        if !self.is_configured() {
            return Ok((format!("(No camera — cannot look {direction})"), None));
        }

        if direction == "around" {
            // Sweep: left 45° → right 90° → back left 45° (returns to center)
            let _ = self.ptz_relative(-45.0, 0.0).await;
            tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
            let _ = self.ptz_relative(90.0, 0.0).await;
            tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
            let _ = self.ptz_relative(-45.0, 0.0).await;
            tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
            return Ok((
                "Swept left-center-right. Camera is now facing forward. Call see() to capture."
                    .to_string(),
                None,
            ));
        }

        let (pan_deg, tilt_deg) = direction_to_degrees(direction, degrees);
        let _ = self.ptz_relative(pan_deg, tilt_deg).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;

        let desc = match direction {
            "left" => format!("Turned left {degrees}°"),
            "right" => format!("Turned right {degrees}°"),
            "up" => format!("Tilted up {degrees}°"),
            "down" => format!("Tilted down {degrees}°"),
            _ => format!("Moved {direction}"),
        };
        Ok((desc, None))
    }

    /// Send ONVIF RelativeMove SOAP request with WS-Security authentication.
    ///
    /// Tapo C220 coordinate system (confirmed from Python version):
    ///   positive x = physical LEFT, negative x = physical RIGHT
    ///   positive y = physical DOWN, negative y = physical UP
    ///
    /// ONVIF range is -1.0..+1.0, so we divide degrees by 180 (pan) or 90 (tilt).
    async fn ptz_relative(&self, pan_deg: f32, tilt_deg: f32) -> Result<()> {
        let pan = pan_deg / 180.0;
        let tilt = tilt_deg / 90.0;
        let ws_security = self.ws_security_header();

        let soap = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"
            xmlns:ptz="http://www.onvif.org/ver20/ptz/wsdl"
            xmlns:tt="http://www.onvif.org/ver10/schema">
  <s:Header>{ws_security}</s:Header>
  <s:Body>
    <ptz:RelativeMove>
      <ptz:ProfileToken>Profile_1</ptz:ProfileToken>
      <ptz:Translation>
        <tt:PanTilt x="{pan}" y="{tilt}"/>
      </ptz:Translation>
    </ptz:RelativeMove>
  </s:Body>
</s:Envelope>"#
        );

        let url = format!("http://{}:{}/onvif/PTZ", self.host, self.onvif_port);
        let _ = self
            .client
            .post(&url)
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(soap)
            .send()
            .await;

        Ok(())
    }

    /// Build ONVIF WS-Security UsernameToken header (PasswordDigest).
    ///
    /// PasswordDigest = Base64(SHA1(nonce_bytes + created_utf8 + password_utf8))
    fn ws_security_header(&self) -> String {
        use base64::{engine::general_purpose::STANDARD as B64, Engine};
        use sha1::{Digest, Sha1};

        // Use UUID v4 bytes as a random nonce (uuid crate already in deps)
        let nonce_bytes = uuid::Uuid::new_v4().as_bytes().to_vec();
        let nonce_b64 = B64.encode(&nonce_bytes);
        let created = utc_now_iso8601();

        let mut hasher = Sha1::new();
        hasher.update(&nonce_bytes);
        hasher.update(created.as_bytes());
        hasher.update(self.password.as_bytes());
        let digest = B64.encode(hasher.finalize());

        format!(
            r#"<wsse:Security xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd"
                             xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">
  <wsse:UsernameToken>
    <wsse:Username>{}</wsse:Username>
    <wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{digest}</wsse:Password>
    <wsse:Nonce EncodingType="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary">{nonce_b64}</wsse:Nonce>
    <wsu:Created>{created}</wsu:Created>
  </wsse:UsernameToken>
</wsse:Security>"#,
            self.username
        )
    }
}

// ── Pure functions (extracted for testability) ─────────────────────

/// Map direction + degrees to (pan_deg, tilt_deg) for ONVIF RelativeMove.
///
/// Tapo C220 coordinate system (matches Python version):
///   positive pan  = physical LEFT
///   negative pan  = physical RIGHT
///   negative tilt = physical UP
///   positive tilt = physical DOWN
pub(crate) fn direction_to_degrees(direction: &str, degrees: u32) -> (f32, f32) {
    let d = degrees as f32;
    match direction {
        "left"  => ( d, 0.0),
        "right" => (-d, 0.0),
        "up"    => (0.0, -d),
        "down"  => (0.0,  d),
        _       => (0.0, 0.0),
    }
}

/// Current UTC timestamp in ISO 8601 format required by WS-Security.
fn utc_now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as YYYY-MM-DDTHH:MM:SSZ (simple, no external crate)
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Days since epoch → calendar date (Gregorian, good until 2099)
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let months = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for dm in months {
        if days < dm { break; }
        days -= dm;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unconfigured() -> CameraTool {
        CameraTool::new("".to_string(), "".to_string(), "".to_string(), 80)
    }

    fn configured() -> CameraTool {
        CameraTool::new(
            "192.168.1.100".to_string(),
            "admin".to_string(),
            "pass".to_string(),
            8080,
        )
    }

    // ── is_configured ────────────────────────────────────────────

    #[test]
    fn is_configured_empty_host_returns_false() {
        assert!(!unconfigured().is_configured());
    }

    #[test]
    fn is_configured_with_host_returns_true() {
        assert!(configured().is_configured());
    }

    // ── tool_defs ────────────────────────────────────────────────

    #[test]
    fn tool_defs_has_exactly_two_tools() {
        let defs = CameraTool::tool_defs();
        assert_eq!(defs.len(), 2);
    }

    #[test]
    fn tool_defs_first_is_see() {
        let defs = CameraTool::tool_defs();
        assert_eq!(defs[0].name, "see");
    }

    #[test]
    fn tool_defs_second_is_look() {
        let defs = CameraTool::tool_defs();
        assert_eq!(defs[1].name, "look");
    }

    #[test]
    fn see_tool_required_is_empty() {
        let defs = CameraTool::tool_defs();
        let required = &defs[0].input_schema["required"];
        assert_eq!(required.as_array().unwrap().len(), 0);
    }

    #[test]
    fn look_tool_direction_is_required() {
        let defs = CameraTool::tool_defs();
        let required = &defs[1].input_schema["required"];
        assert!(
            required
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "direction"),
            "direction should be required"
        );
    }

    #[test]
    fn look_tool_enum_contains_all_directions() {
        let defs = CameraTool::tool_defs();
        let enum_vals = &defs[1].input_schema["properties"]["direction"]["enum"];
        let arr = enum_vals.as_array().unwrap();
        for dir in ["left", "right", "up", "down", "around"] {
            assert!(arr.iter().any(|v| v == dir), "Missing direction: {dir}");
        }
    }

    // ── direction_to_degrees (Tapo coordinate system) ─────────────
    // Tapo C220: positive pan = physical LEFT, negative tilt = physical UP

    #[test]
    fn direction_left_has_positive_pan() {
        let (pan, tilt) = direction_to_degrees("left", 30);
        assert!(pan > 0.0, "Tapo: positive pan = left, pan={pan}");
        assert!((tilt - 0.0).abs() < 1e-5, "tilt={tilt}");
    }

    #[test]
    fn direction_right_has_negative_pan() {
        let (pan, tilt) = direction_to_degrees("right", 30);
        assert!(pan < 0.0, "Tapo: negative pan = right, pan={pan}");
        assert!((tilt - 0.0).abs() < 1e-5, "tilt={tilt}");
    }

    #[test]
    fn direction_up_has_negative_tilt() {
        let (pan, tilt) = direction_to_degrees("up", 30);
        assert!((pan - 0.0).abs() < 1e-5, "pan={pan}");
        assert!(tilt < 0.0, "Tapo: negative tilt = up, tilt={tilt}");
    }

    #[test]
    fn direction_down_has_positive_tilt() {
        let (pan, tilt) = direction_to_degrees("down", 30);
        assert!((pan - 0.0).abs() < 1e-5, "pan={pan}");
        assert!(tilt > 0.0, "Tapo: positive tilt = down, tilt={tilt}");
    }

    #[test]
    fn direction_unknown_is_zero_zero() {
        let (pan, tilt) = direction_to_degrees("unknown", 30);
        assert!((pan - 0.0).abs() < 1e-5);
        assert!((tilt - 0.0).abs() < 1e-5);
    }

    #[test]
    fn direction_degrees_magnitude_matches_input() {
        let (pan, _) = direction_to_degrees("left", 45);
        assert!((pan - 45.0).abs() < 1e-5, "pan={pan}");
    }

    #[test]
    fn left_and_right_are_symmetric() {
        let (left_pan, _) = direction_to_degrees("left", 30);
        let (right_pan, _) = direction_to_degrees("right", 30);
        assert!((left_pan + right_pan).abs() < 1e-5, "Should be equal magnitude");
    }

    #[test]
    fn up_and_down_are_symmetric() {
        let (_, up_tilt) = direction_to_degrees("up", 30);
        let (_, down_tilt) = direction_to_degrees("down", 30);
        assert!((up_tilt + down_tilt).abs() < 1e-5, "Should be equal magnitude");
    }

    // ── utc_now_iso8601 ───────────────────────────────────────────

    #[test]
    fn utc_now_iso8601_format_is_valid() {
        let ts = utc_now_iso8601();
        assert_eq!(ts.len(), 20, "Expected YYYY-MM-DDTHH:MM:SSZ, got {ts}");
        assert!(ts.ends_with('Z'), "Should end with Z: {ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
    }
}
