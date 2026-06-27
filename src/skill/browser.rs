use super::*;
use crate::models::tool::Tool;
use anyhow::Context;
use serde_json::json;

pub struct BrowserSkill;

impl BrowserSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for BrowserSkill {
    fn name(&self) -> String {
        "browser".to_string()
    }

    fn tool_spec(&self) -> Tool {
        Tool {
            name: self.name().to_string(),
            description: Some("Simulated browser fetch (returns page metadata)".into()),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" }
                },
                "required": ["url"]
            })),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<Value> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .context("url is required")?;

        let resp = reqwest::get(url).await?;
        let status = resp.status().as_u16();
        let text: String = resp.text().await?;
        Ok(json!({
            "url": url,
            "status": status,
            "body": text
        }))
    }
}
