/// TTS tool — voice of the familiar (ElevenLabs direct API).
/// Plays on PC speaker AND Tapo camera speaker (if camera host is configured).
use anyhow::Result;
use reqwest::Client;
use serde_json::json;

use crate::backend::ToolDef;

use super::tapo_audio::TapoAudio;
use super::ToolOutput;

const ELEVENLABS_URL: &str = "https://api.elevenlabs.io/v1/text-to-speech";

pub struct TtsTool {
    api_key: String,
    voice_id: String,
    camera: TapoAudio,
    client: Client,
}

impl TtsTool {
    pub fn new(
        api_key: String,
        voice_id: String,
        camera_host: String,
        camera_username: String,
        camera_password: String,
    ) -> Self {
        Self {
            api_key,
            voice_id,
            camera: TapoAudio::new(camera_host, camera_username, camera_password),
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
                    },
                    "speaker": {
                        "type": "string",
                        "enum": ["camera", "pc", "both"],
                        "description": "Which speaker to use. \
                            'camera' = Tapo camera speaker (default when available, sounds like it's coming from the room), \
                            'pc' = PC/local speaker (use when asked to speak through PC), \
                            'both' = both simultaneously."
                    }
                },
                "required": ["text"]
            }),
        }]
    }

    /// `speaker`: "camera" | "pc" | "both" | "" (empty = auto)
    pub async fn say(&self, text: &str, speaker: &str) -> Result<ToolOutput> {
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

        let audio_bytes = resp.bytes().await?.to_vec();

        // Resolve which speakers to use
        let cam_available = self.camera.is_configured();
        let want_camera = cam_available && !matches!(speaker, "pc");
        let want_pc     = !cam_available || matches!(speaker, "pc" | "both");

        if want_camera {
            // Camera (primary) runs concurrently with PC.
            // PC playback acts as the "done playing" signal — mpv blocks until audio ends,
            // preventing the next say() from starting before this one finishes.
            let pc_bytes = audio_bytes.clone();
            let (cam_result, ()) = tokio::join!(
                self.camera.play(audio_bytes),
                play_audio(pc_bytes),
            );
            if let Err(e) = cam_result {
                tracing::warn!("camera speaker: {e}");
            }
        } else {
            // PC only
            play_audio(audio_bytes).await;
        }
        let _ = want_pc; // captured in want_camera branch implicitly
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
            // Try players in order — same as Python version.
            // WSL2/WSLg needs --ao=pulse to reach the PulseAudio socket.
            let attempts: &[&[&str]] = &[
                &["mpv", "--no-terminal", "--ao=pulse"],
                &["mpv", "--no-terminal"],
                &["ffplay", "-nodisp", "-autoexit", "-loglevel", "error"],
                &["aplay"],
            ];
            for base_args in attempts {
                let mut cmd = tokio::process::Command::new(base_args[0]);
                for a in &base_args[1..] {
                    cmd.arg(a);
                }
                cmd.arg(tmp.as_os_str());
                if let Ok(out) = cmd.output().await {
                    if out.status.success() {
                        break;
                    }
                }
            }
        }

        let _ = tokio::fs::remove_file(&tmp).await;
    }
}
