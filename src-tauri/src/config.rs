use anyhow::Result;
use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::permissions::{PermRule, TrustMode};

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodingConfig {
    /// Working directory for file/shell tools. Defaults to home dir.
    #[serde(default)]
    pub work_dir: String,
    /// Permission mode: "prompt" | "full" | "custom"
    #[serde(default)]
    pub trust_mode: TrustMode,
    /// Custom allow/deny rules (used when trust_mode = "custom").
    #[serde(default)]
    pub rules: Vec<PermRule>,
}

impl CodingConfig {
    pub fn effective_work_dir(&self) -> String {
        if self.work_dir.is_empty() {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .to_string_lossy()
                .to_string()
        } else {
            self.work_dir.clone()
        }
    }
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
    /// Persona is no longer stored in TOML — use ~/.familiar_ai/ME.md instead.
    /// Kept with `skip_serializing` so existing configs don't break on load.
    #[serde(default, skip_serializing)]
    pub persona: String,
    pub companion_name: String,

    #[serde(default)]
    pub camera: CameraConfig,
    #[serde(default)]
    pub tts: TtsConfig,
    #[serde(default)]
    pub mobility: MobilityConfig,
    #[serde(default)]
    pub coding: CodingConfig,
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
            coding: CodingConfig::default(),
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
