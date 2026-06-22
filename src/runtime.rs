use crate::mcp::McpManager;
use crate::skill::SkillRunner;
use anyhow::{Context, Result};
use async_trait::async_trait;
use autoagents::core::agent::memory::SlidingWindowMemory;
use autoagents::core::agent::prebuilt::executor::ReActAgent;
use autoagents::core::agent::task::Task;
use autoagents::core::agent::{AgentBuilder, AgentDeriveT, DirectAgent};
use autoagents::core::tool::ToolT;
use autoagents::llm::backends::anthropic::Anthropic;
use autoagents::llm::backends::azure_openai::AzureOpenAI;
use autoagents::llm::backends::google::Google;
use autoagents::llm::backends::groq::Groq;
use autoagents::llm::backends::ollama::Ollama;
use autoagents::llm::backends::openai::OpenAI;
use autoagents::llm::backends::openrouter::OpenRouter;
use autoagents::llm::builder::LLMBuilder;
use autoagents_toolkit::mcp::{McpToolWrapper, McpTools};
use autoagents_derive::AgentHooks;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, AgentHooks)]
struct FrameworkAgent {
    tools: Vec<Arc<dyn ToolT>>,
}

// Ensure FrameworkAgent is Send + Sync
unsafe impl Send for FrameworkAgent {}
unsafe impl Sync for FrameworkAgent {}

impl FrameworkAgent {
    fn new(tools: Vec<Arc<dyn ToolT>>) -> Self {
        Self { tools }
    }
}

impl AgentDeriveT for FrameworkAgent {
    type Output = String;

    fn name(&self) -> &'static str {
        "my-agent-framework"
    }

    fn description(&self) -> &'static str {
        "AI agent framework runtime"
    }

    fn tools(&self) -> Vec<Box<dyn ToolT>> {
        self.tools
            .iter()
            .map(|tool| Box::new(McpToolWrapper::new(Arc::clone(tool))) as Box<dyn ToolT>)
            .collect()
    }

    fn output_schema(&self) -> Option<serde_json::Value> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentInput {
    pub agent: String,
    pub task: String,
    pub context: Vec<AgentOutput>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub mcp: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentOutput {
    pub agent: String,
    pub content: String,
}

#[async_trait]
pub trait AgentRuntime: Send + Sync {
    async fn run(&self, input: AgentInput) -> Result<AgentOutput>;
}

#[derive(Debug, Clone, Copy)]
pub struct DeterministicRuntime;

#[async_trait]
impl AgentRuntime for DeterministicRuntime {
    async fn run(&self, input: AgentInput) -> Result<AgentOutput> {
        let previous = input
            .context
            .last()
            .map(|output| output.content.as_str())
            .unwrap_or("no previous output");
        Ok(AgentOutput {
            agent: input.agent.clone(),
            content: format!(
                "{} handled task '{}' with context: {}",
                input.agent, input.task, previous
            ),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AutoAgentsRuntime {
    provider: String,
    model: String,
    api_key_env: Option<String>,
    base_url: Option<String>,
    mcp_tools: Vec<Arc<dyn ToolT>>,
}

impl AutoAgentsRuntime {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            api_key_env: None,
            base_url: None,
            mcp_tools: Vec::new(),
        }
    }

    pub fn with_api_key_env(mut self, env_name: impl Into<String>) -> Self {
        self.api_key_env = Some(env_name.into());
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub async fn with_mcp_tools(mut self, _root: impl AsRef<std::path::Path>, server_names: &[String]) -> Result<Self> {
        let manager = McpManager::from_names(server_names);
        let mut tools = Vec::new();
        
        for server in manager.servers() {
            if server.disabled {
                continue;
            }
            
            // Load tools from MCP server based on transport type
            match server.transport {
                crate::config::McpTransport::Stdio => {
                    if let Some(command) = server.endpoint.as_ref() {
                        let parts: Vec<&str> = command.split_whitespace().collect();
                        if let Some((cmd, args)) = parts.split_first() {
                            let args_vec: Vec<String> = args.iter().map(|s| s.to_string()).collect();
                            
                            if let Ok(mcp_tools) = McpTools::from_config_object(&autoagents_toolkit::mcp::McpConfig {
                                servers: vec![autoagents_toolkit::mcp::McpServerConfig {
                                    name: server.name.clone(),
                                    protocol: "stdio".to_string(),
                                    command: cmd.to_string(),
                                    args: args_vec,
                                    env: std::collections::HashMap::new(),
                                    cwd: None,
                                    timeout: server.timeout.unwrap_or(30),
                                }],
                            }).await {
                                let server_tools = mcp_tools.get_tools().await;
                                tools.extend(server_tools);
                            }
                        }
                    }
                }
                crate::config::McpTransport::Http | crate::config::McpTransport::Websocket => {
                    if server.endpoint.is_some() {
                        if let Ok(mcp_tools) = McpTools::from_config_object(&autoagents_toolkit::mcp::McpConfig {
                            servers: vec![autoagents_toolkit::mcp::McpServerConfig {
                                name: server.name.clone(),
                                protocol: "http".to_string(),
                                command: String::new(),
                                args: Vec::new(),
                                env: std::collections::HashMap::new(),
                                cwd: None,
                                timeout: server.timeout.unwrap_or(30),
                            }],
                        }).await {
                            let server_tools = mcp_tools.get_tools().await;
                            tools.extend(server_tools);
                        }
                    }
                }
            }
        }
        
        self.mcp_tools = tools;
        Ok(self)
    }

    pub async fn prompt_for(&self, input: &AgentInput, project_root: &std::path::Path) -> Result<String> {
        let context = if input.context.is_empty() {
            "No previous agent output.".to_string()
        } else {
            input
                .context
                .iter()
                .map(|output| format!("{}: {}", output.agent, output.content))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let mut sections = vec![
            format!("Role: {}", input.agent),
            format!("Task: {}", input.task),
        ];

        if !input.skills.is_empty() {
            let mut skill_results = Vec::new();
            let runner = SkillRunner::new(project_root);
            for skill_name in &input.skills {
                if let Ok(response) = runner
                    .run(skill_name, serde_json::json!({"query": input.task}))
                    .await
                {
                    skill_results.push(format!("{}: {}", skill_name, serde_json::to_string(&response.result).unwrap_or_default()));
                }
            }
            if !skill_results.is_empty() {
                sections.push(format!("Skills:\n{}", skill_results.join("\n")));
            }
        }

        if let Some(system_prompt) = input.system_prompt.as_deref() {
            sections.push(format!("System Prompt:\n{}", system_prompt));
        }

        if !input.rules.is_empty() {
            let rules = input.rules.join("\n");
            sections.push(format!("Rules:\n{}", rules));
        }

        if !input.mcp.is_empty() {
            let manager = McpManager::from_names(&input.mcp);
            let tool_descriptions = manager.tool_descriptions();
            sections.push(format!("Available MCP tools:\n{}", tool_descriptions.join("\n")));
            sections.push("You can use these tools to complete the task. Describe what you would do with them.".to_string());
        }

        sections.push(format!("Previous outputs:\n{}", context));
        sections.push("Respond as the named role. Keep the answer concise and actionable.".to_string());

        Ok(sections.join("\n\n"))
    }

    fn api_key(&self, default_env: &str) -> Result<String> {
        let env_name = self.api_key_env.as_deref().unwrap_or(default_env);

        if let Ok(value) = std::env::var(env_name) {
            if !value.trim().is_empty() {
                return Ok(value);
            }
        }

        if let Some(value) = load_dotenv_value(env_name) {
            return Ok(value);
        }

        Err(anyhow::anyhow!(
            "missing required environment variable {}",
            env_name
        ))
    }
}

#[async_trait]
impl AgentRuntime for AutoAgentsRuntime {
    async fn run(&self, input: AgentInput) -> Result<AgentOutput> {
        let prompt = self.prompt_for(&input, &std::env::current_dir().context("failed to resolve current working directory")?).await?;
        let provider = normalize_provider(self.provider.as_str());
        let tools = self.mcp_tools.clone();
        let content = match provider.as_str() {
            "openai" => run_openai(&self.model, self.api_key("OPENAI_API_KEY")?, &prompt, tools).await?,
            "ollama" => {
                run_ollama(
                    &self.model,
                    self.base_url.as_deref().unwrap_or("http://localhost:11434"),
                    &prompt,
                    tools,
                )
                .await?
            }
            "openrouter" => {
                run_openrouter(&self.model, self.api_key("OPENROUTER_API_KEY")?, &prompt, tools).await?
            }
            "anthropic" => {
                run_anthropic(&self.model, self.api_key("ANTHROPIC_API_KEY")?, &prompt, tools).await?
            }
            "groq" => run_groq(&self.model, self.api_key("GROQ_API_KEY")?, &prompt, tools).await?,
            "google" | "gemini" => {
                run_google(&self.model, self.api_key("GOOGLE_API_KEY")?, &prompt, tools).await?
            }
            "azure-openai" | "azure_openai" | "azure" | "azureopenai" => {
                run_azure_openai(
                    &self.model,
                    self.api_key("AZURE_OPENAI_API_KEY")?,
                    self.base_url.as_deref().unwrap_or("https://example.openai.azure.com"),
                    &prompt,
                    tools,
                )
                .await?
            }
            provider => anyhow::bail!(
                "unsupported AutoAgents provider '{}'; supported providers: openai, ollama, openrouter, anthropic, groq, google, azure-openai",
                provider
            ),
        };

        Ok(AgentOutput {
            agent: input.agent,
            content,
        })
    }
}

async fn run_openai(model: &str, api_key: String, prompt: &str, tools: Vec<Arc<dyn ToolT>>) -> Result<String> {
    let llm: Arc<OpenAI> = LLMBuilder::<OpenAI>::new()
        .api_key(api_key)
        .model(model)
        .build()?;
    run_with_agent(llm, prompt, tools).await
}

async fn run_ollama(model: &str, base_url: &str, prompt: &str, tools: Vec<Arc<dyn ToolT>>) -> Result<String> {
    let llm: Arc<Ollama> = LLMBuilder::<Ollama>::new()
        .base_url(base_url)
        .model(model)
        .build()?;
    run_with_agent(llm, prompt, tools).await
}

async fn run_openrouter(model: &str, api_key: String, prompt: &str, tools: Vec<Arc<dyn ToolT>>) -> Result<String> {
    let llm: Arc<OpenRouter> = LLMBuilder::<OpenRouter>::new()
        .api_key(api_key)
        .model(model)
        .build()?;
    run_with_agent(llm, prompt, tools).await
}

async fn run_anthropic(model: &str, api_key: String, prompt: &str, tools: Vec<Arc<dyn ToolT>>) -> Result<String> {
    let llm: Arc<Anthropic> = LLMBuilder::<Anthropic>::new()
        .api_key(api_key)
        .model(model)
        .build()?;
    run_with_agent(llm, prompt, tools).await
}

async fn run_groq(model: &str, api_key: String, prompt: &str, tools: Vec<Arc<dyn ToolT>>) -> Result<String> {
    let llm: Arc<Groq> = LLMBuilder::<Groq>::new()
        .api_key(api_key)
        .model(model)
        .build()?;
    run_with_agent(llm, prompt, tools).await
}

async fn run_google(model: &str, api_key: String, prompt: &str, tools: Vec<Arc<dyn ToolT>>) -> Result<String> {
    let llm: Arc<Google> = LLMBuilder::<Google>::new()
        .api_key(api_key)
        .model(model)
        .build()?;
    run_with_agent(llm, prompt, tools).await
}

async fn run_azure_openai(model: &str, api_key: String, base_url: &str, prompt: &str, tools: Vec<Arc<dyn ToolT>>) -> Result<String> {
    let llm: Arc<AzureOpenAI> = LLMBuilder::<AzureOpenAI>::new()
        .api_key(api_key)
        .base_url(base_url)
        .model(model)
        .api_version("2024-02-01")
        .deployment_id(model)
        .build()?;
    run_with_agent(llm, prompt, tools).await
}

fn normalize_provider(provider: &str) -> String {
    provider
        .trim()
        .to_lowercase()
        .replace(' ', "")
        .replace('_', "")
        .replace('-', "")
}

fn load_dotenv_value(key: &str) -> Option<String> {
    let mut dir = std::env::current_dir().ok()?;

    loop {
        let path = crate::nami_root(&dir).join(".env");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                if let Some((name, value)) = line.split_once('=') {
                    let name = name.trim();
                    if name == key {
                        let value = value.trim();
                        return Some(unquote(value));
                    }
                }
            }
        }

        if !dir.pop() {
            break;
        }
    }

    None
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let chars = trimmed.chars().collect::<Vec<_>>();
        if (chars[0] == '\'' && chars[chars.len() - 1] == '\'')
            || (chars[0] == '"' && chars[chars.len() - 1] == '"')
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

async fn run_with_agent<T>(llm: Arc<T>, prompt: &str, tools: Vec<Arc<dyn ToolT>>) -> Result<String>
where
    T: autoagents::llm::LLMProvider + Send + Sync + 'static,
{
    let framework_agent = FrameworkAgent::new(tools);
    let agent = ReActAgent::new(framework_agent);
    let sliding_window_memory = Box::new(SlidingWindowMemory::new(10));
    let handle = AgentBuilder::<_, DirectAgent>::new(agent)
        .llm(llm)
        .memory(sliding_window_memory)
        .build()
        .await?;
    let out = handle.agent.run(Task::new(prompt)).await?;
    Ok(String::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    static CURRENT_DIR_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn current_dir_lock() -> &'static Mutex<()> {
        CURRENT_DIR_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[tokio::test]
    async fn deterministic_runtime_includes_agent_name_and_task() {
        let runtime = DeterministicRuntime;

        let output = runtime
            .run(AgentInput {
                agent: "planner".to_string(),
                task: "READMEを書いて".to_string(),
                context: Vec::new(),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(output.agent, "planner");
        assert!(output.content.contains("planner"));
        assert!(output.content.contains("README"));
    }

    #[test]
    fn autoagents_runtime_builds_prompt_from_agent_input() {
        let runtime = AutoAgentsRuntime::new("openai", "gpt-4o-mini");
        let prompt = tokio::runtime::Runtime::new().unwrap().block_on(async {
            runtime
                .prompt_for(
                    &AgentInput {
                        agent: "planner".to_string(),
                        task: "READMEを書いて".to_string(),
                        context: vec![AgentOutput {
                            agent: "coder".to_string(),
                            content: "Created outline".to_string(),
                        }],
                        ..Default::default()
                    },
                    std::path::Path::new("."),
                )
                .await
        })
        .unwrap();

        assert!(prompt.contains("Role: planner"));
        assert!(prompt.contains("Task: README"));
        assert!(prompt.contains("coder: Created outline"));
    }

    #[tokio::test]
    async fn autoagents_runtime_rejects_missing_api_key_before_network_call() {
        let _guard = current_dir_lock().lock().unwrap();
        let temp_dir = tempdir().unwrap();
        let previous_dir = std::env::current_dir().unwrap();
        let previous_value = std::env::var_os("AGENT_TEST_MISSING_OPENAI_KEY");
        std::env::set_current_dir(temp_dir.path()).unwrap();
        std::env::remove_var("AGENT_TEST_MISSING_OPENAI_KEY");

        let runtime = AutoAgentsRuntime::new("openai", "gpt-4o-mini")
            .with_api_key_env("AGENT_TEST_MISSING_OPENAI_KEY");

        let error = runtime
            .run(AgentInput {
                agent: "planner".to_string(),
                task: "READMEを書いて".to_string(),
                context: Vec::new(),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .to_string();

        if let Some(value) = previous_value {
            std::env::set_var("AGENT_TEST_MISSING_OPENAI_KEY", value);
        } else {
            std::env::remove_var("AGENT_TEST_MISSING_OPENAI_KEY");
        }
        std::env::set_current_dir(previous_dir).unwrap();

        assert!(error.contains("AGENT_TEST_MISSING_OPENAI_KEY"));
    }

    #[tokio::test]
    async fn autoagents_runtime_supports_additional_provider_names() {
        let _guard = current_dir_lock().lock().unwrap();
        let temp_dir = tempdir().unwrap();
        let previous_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let cases = [
            ("OpenRouter", "OPENROUTER_API_KEY"),
            ("Anthropic", "ANTHROPIC_API_KEY"),
            ("Groq", "GROQ_API_KEY"),
            ("Google", "GOOGLE_API_KEY"),
            ("Azure OpenAI", "AZURE_OPENAI_API_KEY"),
            ("azure_openai", "AZURE_OPENAI_API_KEY"),
        ];

        for (provider, env_name) in cases {
            let previous_value = std::env::var_os(env_name);
            std::env::remove_var(env_name);

            let runtime = AutoAgentsRuntime::new(provider, "test-model")
                .with_api_key_env(env_name);
            let error = runtime
                .run(AgentInput {
                    agent: "planner".to_string(),
                    task: "READMEを書いて".to_string(),
                    context: Vec::new(),
                    ..Default::default()
                })
                .await
                .unwrap_err()
                .to_string();

            if let Some(value) = previous_value {
                std::env::set_var(env_name, value);
            } else {
                std::env::remove_var(env_name);
            }

            assert!(
                error.contains(env_name),
                "provider {provider} should resolve to {env_name}, got: {error}"
            );
        }

        std::env::set_current_dir(previous_dir).unwrap();
    }

    #[tokio::test]
    async fn autoagents_runtime_reads_api_key_from_dotenv_file() {
        let _guard = current_dir_lock().lock().unwrap();
        let temp_dir = tempdir().unwrap();
        let previous_dir = std::env::current_dir().unwrap();
        fs::create_dir_all(temp_dir.path().join(".nami")).unwrap();
        fs::write(temp_dir.path().join(".nami").join(".env"), "OPENAI_API_KEY=dotenv-key\n").unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let runtime = AutoAgentsRuntime::new("openai", "gpt-4o-mini");
        let api_key = runtime.api_key("OPENAI_API_KEY").unwrap();

        std::env::set_current_dir(previous_dir).unwrap();

        assert_eq!(api_key, "dotenv-key");
    }
}
