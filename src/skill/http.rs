use super::*;
use crate::models::tool::Tool;
use anyhow::Context;
use serde_json::json;

pub struct HttpSkill;

impl HttpSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for HttpSkill {
    fn name(&self) -> String {
        "http".to_string()
    }

    fn tool_spec(&self) -> Tool {
        Tool {
            name: self.name().to_string(),
            description: Some("Send GET/POST requests and return body".into()),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "method": { "type": "string" },
                    "url": { "type": "string" },
                    "headers": { "type": "object" },
                    "body": { "type": "string" }
                },
                "required": ["method", "url"]
            })),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<Value> {
        let method = args
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .context("url is required")?;

        let client = reqwest::Client::builder()
            .user_agent("nami/0.1")
            .build()?;
        let mut req = match method.as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            other => anyhow::bail!("unsupported method: {}", other),
        };

        if let Some(body) = args.get("body").and_then(|v| v.as_str()) {
            req = req.body(body.to_string());
        }

        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let text = resp.text().await?;
        Ok(json!({
            "method": method,
            "url": url,
            "status": status,
            "body": text
        }))
    }
}
