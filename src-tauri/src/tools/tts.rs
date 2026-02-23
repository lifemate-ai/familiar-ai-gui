/// TTS tool — voice of the familiar (ElevenLabs direct API).
use anyhow::Result;
use reqwest::Client;
use serde_json::json;

use crate::backend::ToolDef;

use super::ToolOutput;

const ELEVENLABS_URL: &str = "https://api.elevenlabs.io/v1/text-to-speech";

pub struct TtsTool {
    api_key: String,
    voice_id: String,
    client: Client,
}

impl TtsTool {
    pub fn new(api_key: String, voice_id: String) -> Self {
        Self {
            api_key,
            voice_id,
            client: Client::new(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    pub fn tool_defs() -> Vec<ToolDef> {
        vec![ToolDef {
            name: "say".to_string(),
            description:
                "Speak aloud. This is the ONLY way to make sound — text output is silent. \
                 Keep it to 1-2 short sentences."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "What to say aloud"
                    }
                },
                "required": ["text"]
            }),
        }]
    }

    pub async fn say(&self, text: &str) -> Result<ToolOutput> {
        if !self.is_configured() {
            return Ok((format!("(No TTS configured — would have said: {text})"), None));
        }

        let url = format!("{}/{}", ELEVENLABS_URL, self.voice_id);
        let body = json!({
            "text": text,
            "model_id": "eleven_multilingual_v2",
            "voice_settings": {
                "stability": 0.5,
                "similarity_boost": 0.75
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("xi-api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err = resp.text().await.unwrap_or_default();
            return Ok((format!("TTS failed ({status}): {err}"), None));
        }

        let audio_bytes = resp.bytes().await?;

        // Play audio via system command (platform-appropriate)
        play_audio(audio_bytes.to_vec()).await;

        Ok((format!("Said: {text}"), None))
    }
}

async fn play_audio(bytes: Vec<u8>) {
    // Write to temp file and play
    let tmp = std::env::temp_dir().join(format!(
        "familiar_tts_{}.mp3",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    if tokio::fs::write(&tmp, &bytes).await.is_ok() {
        // Try platform-specific player
        #[cfg(target_os = "windows")]
        let _ = tokio::process::Command::new("powershell")
            .args([
                "-c",
                &format!(
                    "(New-Object Media.SoundPlayer '{}').PlaySync()",
                    tmp.display()
                ),
            ])
            .output()
            .await;

        #[cfg(target_os = "macos")]
        let _ = tokio::process::Command::new("afplay")
            .arg(tmp.as_os_str())
            .output()
            .await;

        #[cfg(target_os = "linux")]
        {
            // Try mpv, then ffplay, then aplay
            if tokio::process::Command::new("mpv")
                .arg("--no-terminal")
                .arg(tmp.as_os_str())
                .output()
                .await
                .is_err()
            {
                let _ = tokio::process::Command::new("ffplay")
                    .args(["-nodisp", "-autoexit"])
                    .arg(tmp.as_os_str())
                    .output()
                    .await;
            }
        }

        let _ = tokio::fs::remove_file(&tmp).await;
    }
}
