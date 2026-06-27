use super::*;
use crate::models::tool::Tool;
use anyhow::Context;
use serde_json::json;

pub struct SearchSkill;

impl SearchSkill {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Skill for SearchSkill {
    fn name(&self) -> String {
        "search".to_string()
    }

    fn tool_spec(&self) -> Tool {
        Tool {
            name: self.name().to_string(),
            description: Some("Search web using DuckDuckGo lite HTML".into()),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            })),
        }
    }

    async fn execute(&self, args: Value) -> anyhow::Result<Value> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .context("query is required")?;
        let encoded = urlencoding::encode(query);
        let url = format!("https://lite.duckduckgo.com/lite/?q={}", encoded);

        let client = reqwest::Client::builder()
            .user_agent("nami/0.1")
            .build()?;
        let resp = client.get(url).send().await?;
        let text = resp.text().await?;

        let results = parse_search_results(&text);
        Ok(json!({ "query": query, "results": results }))
    }
}

fn parse_search_results(html: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let re = regex::Regex::new(r#"<a[^>]+class="result-link"[^>]*href="([^"]+)"[^>]*>([^<]+)</a>"#).ok();
    if let Some(re) = re {
        for cap in re.captures_iter(html) {
            out.push(json!({
                "title": cap.get(2).map(|m| m.as_str()).unwrap_or(""),
                "url": cap.get(1).map(|m| m.as_str()).unwrap_or(""),
            }));
        }
    }
    out
}
