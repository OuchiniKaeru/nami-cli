use std::collections::HashMap;
use std::sync::Arc;

use crate::models::tool::Tool;
use serde_json::Value;

use super::Skill;

#[derive(Default)]
pub struct SkillRegistry {
    pub skills: HashMap<String, Arc<dyn Skill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, skill: Arc<dyn Skill>) {
        self.skills.insert(skill.name().to_string(), skill);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Skill>> {
        self.skills.get(name).cloned()
    }

    pub fn list_tools(&self) -> Vec<Tool> {
        self.skills.values().map(|s| s.tool_spec()).collect()
    }

    pub async fn execute(&self, name: &str, args: Value) -> anyhow::Result<Value> {
        let skill = self
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("skill not found: {}", name))?;
        let out = skill.execute(args).await?;
        Ok(out)
    }
}
