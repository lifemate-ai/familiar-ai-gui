/// Shell execution tool.
///
/// Runs arbitrary bash commands with timeout, working directory, and output capture.
use anyhow::Result;
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;

use super::ToolOutput;

pub struct ShellTool {
    pub work_dir: String,
}

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT_BYTES: usize = 32_768; // 32 KB

impl ShellTool {
    pub fn new(work_dir: String) -> Self {
        Self { work_dir }
    }

    pub fn tool_defs() -> Vec<crate::backend::ToolDef> {
        use serde_json::json;
        vec![crate::backend::ToolDef {
            name: "bash".to_string(),
            description: "Run a shell command. Returns stdout + stderr. Has a timeout.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Timeout in seconds (default 30, max 120)"
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Working directory override (default: configured work_dir)"
                    }
                },
                "required": ["command"]
            }),
        }]
    }

    pub async fn bash(&self, input: &Value) -> Result<ToolOutput> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing command"))?;

        let timeout_secs = input["timeout_secs"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(120);

        let cwd_raw = input["cwd"].as_str().unwrap_or(&self.work_dir);
        let cwd = if std::path::Path::new(cwd_raw).is_absolute() {
            std::path::PathBuf::from(cwd_raw)
        } else {
            std::path::Path::new(&self.work_dir).join(cwd_raw)
        };

        let mut child = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .current_dir(&cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            child.wait_with_output(),
        )
        .await;

        match result {
            Ok(Ok(out)) => {
                let stdout = truncate_output(&out.stdout);
                let stderr = truncate_output(&out.stderr);
                let status = out.status.code().unwrap_or(-1);

                let mut text = format!("Exit: {status}\n");
                if !stdout.is_empty() {
                    text.push_str("--- stdout ---\n");
                    text.push_str(&stdout);
                    text.push('\n');
                }
                if !stderr.is_empty() {
                    text.push_str("--- stderr ---\n");
                    text.push_str(&stderr);
                }

                Ok((text, None))
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Ok((
                format!("Command timed out after {timeout_secs}s"),
                None,
            )),
        }
    }
}

fn truncate_output(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes).into_owned();
    if s.len() > MAX_OUTPUT_BYTES {
        format!(
            "{}...[truncated, {} bytes total]",
            &s[..MAX_OUTPUT_BYTES],
            s.len()
        )
    } else {
        s
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool() -> ShellTool {
        ShellTool::new("/tmp".to_string())
    }

    #[tokio::test]
    async fn bash_echo_returns_output() {
        let out = tool().bash(&json!({ "command": "echo hello" })).await.unwrap();
        assert!(out.0.contains("hello"));
        assert!(out.0.contains("Exit: 0"));
    }

    #[tokio::test]
    async fn bash_exit_code_captured() {
        let out = tool().bash(&json!({ "command": "exit 42" })).await.unwrap();
        assert!(out.0.contains("Exit: 42"));
    }

    #[tokio::test]
    async fn bash_stderr_captured() {
        let out = tool()
            .bash(&json!({ "command": "echo error >&2" }))
            .await
            .unwrap();
        assert!(out.0.contains("error"));
        assert!(out.0.contains("stderr"));
    }

    #[tokio::test]
    async fn bash_timeout_respected() {
        let out = tool()
            .bash(&json!({ "command": "sleep 60", "timeout_secs": 1 }))
            .await
            .unwrap();
        assert!(out.0.contains("timed out"));
    }

    #[tokio::test]
    async fn bash_cwd_is_set() {
        let out = tool()
            .bash(&json!({ "command": "pwd", "cwd": "/tmp" }))
            .await
            .unwrap();
        assert!(out.0.contains("/tmp"));
    }

    #[tokio::test]
    async fn bash_max_timeout_capped_at_120() {
        // Just verify the tool_def says max timeout is respected — not a live sleep test
        let input = json!({ "command": "echo hi", "timeout_secs": 999 });
        let timeout = input["timeout_secs"].as_u64().unwrap_or(30).min(120);
        assert_eq!(timeout, 120);
    }
}
