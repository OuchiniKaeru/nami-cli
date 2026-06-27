use crate::models::tool::Tool;
use async_trait::async_trait;
use serde_json::Value;

pub mod registry;

pub use registry::SkillRegistry;

#[async_trait]
pub trait Skill: Send + Sync {
    fn name(&self) -> String;
    async fn execute(&self, args: Value) -> anyhow::Result<Value>;
    fn tool_spec(&self) -> Tool;
}

pub mod filesystem;
pub mod shell;
pub mod browser;
pub mod search;
pub mod http;
pub mod external;

