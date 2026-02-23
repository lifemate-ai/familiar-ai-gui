pub mod camera;
pub mod mobility;
pub mod tts;

use anyhow::Result;
use serde_json::Value;

use crate::backend::ToolDef;
use crate::config::Config;

/// Result from executing a tool: (text_description, optional_jpeg_base64)
pub type ToolOutput = (String, Option<String>);

/// All tools available to the agent.
pub struct ToolRegistry {
    pub camera: camera::CameraTool,
    pub tts: tts::TtsTool,
    pub mobility: mobility::MobilityTool,
}

impl ToolRegistry {
    pub fn new(config: &Config) -> Self {
        Self {
            camera: camera::CameraTool::new(
                config.camera.host.clone(),
                config.camera.username.clone(),
                config.camera.password.clone(),
                config.camera.onvif_port,
            ),
            tts: tts::TtsTool::new(
                config.tts.elevenlabs_api_key.clone(),
                config.tts.voice_id.clone(),
            ),
            mobility: mobility::MobilityTool::new(
                config.mobility.tuya_region.clone(),
                config.mobility.tuya_api_key.clone(),
                config.mobility.tuya_api_secret.clone(),
                config.mobility.tuya_device_id.clone(),
            ),
        }
    }

    /// Return all tool definitions for the LLM.
    pub fn tool_defs(&self) -> Vec<ToolDef> {
        let mut defs = camera::CameraTool::tool_defs();
        defs.extend(tts::TtsTool::tool_defs());
        defs.extend(mobility::MobilityTool::tool_defs());
        defs
    }

    /// Execute a tool by name with given input. Returns (text, optional_image_b64).
    pub async fn execute(&self, name: &str, input: &Value) -> Result<ToolOutput> {
        match name {
            "see" => self.camera.capture().await,
            "look" => {
                let dir = input["direction"].as_str().unwrap_or("around");
                let degrees = input["degrees"].as_u64().unwrap_or(30) as u32;
                self.camera.look(dir, degrees).await
            }
            "say" => {
                let text = input["text"].as_str().unwrap_or("");
                self.tts.say(text).await
            }
            "walk" => {
                let dir = input["direction"].as_str().unwrap_or("stop");
                let duration = input["duration"].as_f64();
                self.mobility.walk(dir, duration).await
            }
            _ => Ok((format!("Unknown tool: {name}"), None)),
        }
    }
}
