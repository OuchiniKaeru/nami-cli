use crate::models::tool::{Tool, ToolCall, ToolResult};
use crate::skill::SkillRegistry;
use crate::mcp::McpClient;

pub struct ToolRouter {
    pub skills: SkillRegistry,
    pub mcp: McpClient,
}

impl ToolRouter {
    pub fn new(skills: SkillRegistry, mcp: McpClient) -> Self {
        Self { skills, mcp }
    }

    pub async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        let args = call.arguments.clone();
        let is_skill = self.skills.get(&call.name).is_some();

        let out = if is_skill {
            self.skills.execute(&call.name, args).await?
        } else {
            self.mcp.call_tool_by_name(&call.name, args).await?
        };

        Ok(ToolResult {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            content: serde_json::to_string(&out)?,
            error: false,
            is_mcp: !is_skill,
        })
    }

    pub async fn list_tools(&self) -> Vec<Tool> {
        let mut list = self.skills.list_tools();
        list.extend(self.mcp.list_all_tools().await.unwrap_or_default());
        list
    }
}
