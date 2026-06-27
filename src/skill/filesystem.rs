use super::*;
use crate::models::tool::Tool;
use anyhow::Context;
use serde_json::json;

pub struct FilesystemSkill;

impl FilesystemSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for FilesystemSkill {
    fn name(&self) -> String {
        "filesystem".to_string()
    }

    fn tool_spec(&self) -> Tool {
        Tool {
            name: self.name().to_string(),
            description: Some("Read/write/list files in a limited directory".into()),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "operation": { "type": "string", "enum": ["list", "read", "write"] },
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["operation", "path"]
            })),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<Value> {
        let operation = args
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .context("path is required")?;

        match operation {
            "list" => {
                let entries = std::fs::read_dir(path)
                    .with_context(|| format!("failed to read dir: {}", path))?
                    .map(|e| {
                        e.map(|entry| {
                            json!({
                                "name": entry.file_name().to_string_lossy().to_string(),
                                "is_dir": entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                            })
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({ "entries": entries }))
            }
            "read" => {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("failed to read file: {}", path))?;
                Ok(json!({ "content": content }))
            }
            "write" => {
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .context("content is required for write")?;
                std::fs::write(path, content)
                    .with_context(|| format!("failed to write file: {}", path))?;
                Ok(json!({ "status": "ok" }))
            }
            other => anyhow::bail!("unsupported filesystem operation: {}", other),
        }
    }
}
