use super::*;
use crate::models::tool::Tool;
use anyhow::Context;
use serde_json::json;

pub struct ShellSkill;

impl ShellSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for ShellSkill {
    fn name(&self) -> String {
        "shell".to_string()
    }

    fn tool_spec(&self) -> Tool {
        Tool {
            name: self.name().to_string(),
            description: Some("Execute a shell command".into()),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            })),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<Value> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .context("command is required")?;

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.code().unwrap_or(-1);

        Ok(json!({
            "status": status,
            "stdout": stdout,
            "stderr": stderr
        }))
    }
}
