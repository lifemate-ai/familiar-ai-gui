use anyhow::Result;
use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn config_path() -> PathBuf {
    config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("familiar-ai")
        .join("config.toml")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CameraConfig {
    pub host: String,
    pub username: String,
    pub password: String,
    #[serde(default = "default_onvif_port")]
    pub onvif_port: u16,
}

fn default_onvif_port() -> u16 {
    2020
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TtsConfig {
    pub elevenlabs_api_key: String,
    #[serde(default = "default_voice_id")]
    pub voice_id: String,
}

fn default_voice_id() -> String {
    "cgSgspJ2msm6clMCkdW9".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MobilityConfig {
    pub tuya_region: String,
    pub tuya_api_key: String,
    pub tuya_api_secret: String,
    pub tuya_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// "anthropic" | "kimi" | "gemini" | "openai"
    #[serde(default = "default_platform")]
    pub platform: String,
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    pub agent_name: String,
    pub persona: String,
    pub companion_name: String,

    #[serde(default)]
    pub camera: CameraConfig,
    #[serde(default)]
    pub tts: TtsConfig,
    #[serde(default)]
    pub mobility: MobilityConfig,
}

fn default_platform() -> String {
    "kimi".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            platform: default_platform(),
            api_key: String::new(),
            model: String::new(),
            agent_name: "AI".to_string(),
            persona: String::new(),
            companion_name: "You".to_string(),
            camera: CameraConfig::default(),
            tts: TtsConfig::default(),
            mobility: MobilityConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            let config: Self = toml::from_str(&text)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&path, text)?;
        Ok(())
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty() && !self.agent_name.is_empty()
    }

    /// Effective model name based on platform defaults
    pub fn effective_model(&self) -> &str {
        if !self.model.is_empty() {
            return &self.model;
        }
        match self.platform.as_str() {
            "kimi" => "kimi-k2.5",
            "anthropic" => "claude-haiku-4-5-20251001",
            "gemini" => "gemini-2.5-flash",
            "openai" => "gpt-4o-mini",
            _ => "kimi-k2.5",
        }
    }
}
