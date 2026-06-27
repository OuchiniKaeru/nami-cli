use crate::models::config::AppConfig;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub config: AppConfig,
    pub messages: Vec<crate::models::message::Message>,
    pub metrics: crate::models::metrics::Metrics,
    pub tool_calls: Vec<crate::models::tool::ToolCall>,
    pub mcp_calls: Vec<crate::models::tool::ToolCall>,
    pub errors: Vec<String>,
}

impl SessionRecord {
    pub fn new(config: AppConfig, messages: Vec<crate::models::message::Message>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        let id = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        Self {
            id,
            created_at: now.clone(),
            updated_at: now,
            config,
            messages,
            metrics: crate::models::metrics::Metrics::default(),
            tool_calls: Vec::new(),
            mcp_calls: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn path(directory: &str, id: &str) -> PathBuf {
        PathBuf::from(directory).join(format!("{}.json", id))
    }

    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read session file: {}", path.as_ref().display()))?;
        let record: Self = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse session file: {}", path.as_ref().display()))?;
        Ok(record)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let p = Self::path(&self.config.session.directory, &self.id);
        if let Some(parent) = p.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&p, json)?;
        Ok(())
    }
}
