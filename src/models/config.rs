use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAI,
    OpenRouter,
    AzureOpenAI,
    Groq,
    Anthropic,
    Custom,
    Gemini,
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderKind::OpenAI => write!(f, "openai"),
            ProviderKind::OpenRouter => write!(f, "openrouter"),
            ProviderKind::AzureOpenAI => write!(f, "azure_openai"),
            ProviderKind::Groq => write!(f, "groq"),
            ProviderKind::Anthropic => write!(f, "anthropic"),
            ProviderKind::Custom => write!(f, "custom"),
            ProviderKind::Gemini => write!(f, "gemini"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(rename = "type")]
    pub kind: ProviderKind,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none", alias = "api-key")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub provider: ProviderConfig,
    pub temperature: f32,
    pub max_tokens: u32,
    pub max_iterations: u32,
    pub stream: bool,
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub skills: Vec<String>,
    pub mcp: McpConfig,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub rules: Vec<String>,
    /// 設定ファイルの基準ディレクトリ。実行時に解決し、相対パスの起点とする。
    #[serde(skip, default = "default_base_dir")]
    pub base_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionConfig {
    #[serde(default = "default_true")]
    pub save: bool,
    #[serde(default = "default_directory")]
    pub directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    #[serde(default = "default_directory")]
    pub directory: String,
    #[serde(default = "default_memory_file")]
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoggingConfig {
    #[serde(default = "default_directory")]
    pub directory: String,
    #[serde(default = "default_log_level")]
    pub level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub transport: McpTransport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    Http,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallControl {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

fn default_true() -> bool {
    true
}

fn default_directory() -> String {
    ".".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_memory_file() -> String {
    "memory/memory.jsonl".to_string()
}

fn default_base_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
