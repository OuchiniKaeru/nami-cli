use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::System => write!(f, "system"),
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool => write!(f, "tool"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<crate::models::tool::ToolCall>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "reasoning",
        alias = "thinking"
    )]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<crate::models::Attachment>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(content.into()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
            reasoning_content: None,
            attachments: Vec::new(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::user_with_attachments(content, Vec::new())
    }

    pub fn user_with_attachments(
        content: impl Into<String>,
        attachments: Vec<crate::models::Attachment>,
    ) -> Self {
        Self {
            role: Role::User,
            content: Some(content.into()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
            reasoning_content: None,
            attachments,
        }
    }

    pub fn assistant(
        content: Option<String>,
        tool_calls: Option<Vec<crate::models::tool::ToolCall>>,
    ) -> Self {
        Self {
            role: Role::Assistant,
            content,
            tool_call_id: None,
            name: None,
            tool_calls,
            reasoning_content: None,
            attachments: Vec::new(),
        }
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_call_id: Some(tool_call_id.into()),
            name: Some(name.into()),
            tool_calls: None,
            reasoning_content: None,
            attachments: Vec::new(),
        }
    }
}
