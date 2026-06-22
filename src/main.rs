use nami::config::Config;
use nami::event::EventRecord;
use nami::load_dotenv_from_project_root;
use nami::mcp::McpManager;
use nami::runtime::{AgentInput, AgentOutput, AgentRuntime, AutoAgentsRuntime, DeterministicRuntime};
use nami::session::{MessageRecord, SessionStore};
use nami::skill::SkillRunner;
use nami::workflow::WorkflowRunner;
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use std::env;

#[derive(Debug, Parser)]
#[command(name = "agent", version, about = "Local-first AI agent framework")]
struct Cli {
    #[arg(short, long, value_name = "PATH", help = "Path to agent.yaml config file")]
    config: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Chat(ChatCommand),
    Session(SessionCommand),
    Skill(SkillCommand),
    Workflow(WorkflowCommand),
    Mcp(McpCommand),
    Init(InitCommand),
}

#[derive(Debug, Args)]
struct ChatCommand {
    #[arg(value_name = "MESSAGE")]
    message: Option<String>,
    #[arg(long, value_name = "SESSION_ID")]
    resume: Option<String>,
    #[command(subcommand)]
    command: Option<ChatSubcommand>,
}

#[derive(Debug, Subcommand)]
enum ChatSubcommand {
    Resume { session_id: String },
}

#[derive(Debug, Args)]
struct SessionCommand {
    #[command(subcommand)]
    command: SessionSubcommand,
}

#[derive(Debug, Subcommand)]
enum SessionSubcommand {
    List,
    Show { session_id: String },
    Delete { session_id: String },
}

#[derive(Debug, Args)]
struct SkillCommand {
    #[command(subcommand)]
    command: SkillSubcommand,
}

#[derive(Debug, Subcommand)]
enum SkillSubcommand {
    Run {
        name: String,
        #[arg(long, default_value = "{}")]
        input: String,
    },
}

#[derive(Debug, Args)]
struct WorkflowCommand {
    #[command(subcommand)]
    command: WorkflowSubcommand,
}

#[derive(Debug, Subcommand)]
enum WorkflowSubcommand {
    Run { task: String },
}

#[derive(Debug, Args)]
struct McpCommand {
    #[command(subcommand)]
    command: McpSubcommand,
}

#[derive(Debug, Subcommand)]
enum McpSubcommand {
    List,
}

#[derive(Debug, Args)]
struct InitCommand {
    #[arg(short, long, value_name = "PATH", help = "Project directory path (default: current directory)")]
    path: Option<String>,
}

fn init_project(project_dir: &std::path::Path) -> Result<()> {
    use std::fs;
    
    let nami_dir = project_dir.join(".nami");
    
    // Create .nami directory
    fs::create_dir_all(&nami_dir)?;
    
    // Create subdirectories
    fs::create_dir_all(nami_dir.join("skills"))?;
    fs::create_dir_all(nami_dir.join("sessions"))?;
    fs::create_dir_all(nami_dir.join("cache"))?;
    fs::create_dir_all(nami_dir.join("logs"))?;
    fs::create_dir_all(nami_dir.join("runtime"))?;
    
    // Create default agent.yaml
    let agent_yaml = r#"project:
  name: my-agent-project

model:
  provider: openrouter
  model: moonshotai/kimi-k2.7-code
  api_key_env: OPENROUTER_API_KEY

system_prompt: |
  あなたは、優秀なAIエージェントです。
  日本語で応答すること。

rules:
  - NAMI.md

skills:
  - sample

mcp:
  - context7
  - filesystem
"#;
    fs::write(nami_dir.join("agent.yaml"), agent_yaml)?;
    
    // Create default mcp_setting.json
    let mcp_setting = r#"{
  "mcpServers": {}
}
"#;
    fs::write(nami_dir.join("mcp_setting.json"), mcp_setting)?;
    
    // Create .env.sample
    let env_sample = r#"# API Keys
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
OPENROUTER_API_KEY=sk-or-...
GOOGLE_API_KEY=...
GROQ_API_KEY=...
AZURE_OPENAI_API_KEY=...
"#;
    fs::write(nami_dir.join(".env.sample"), env_sample)?;
    
    // Create empty .env file
    fs::write(nami_dir.join(".env"), "# Copy .env.sample and fill in your API keys\n")?;
    
    // Create NAMI.md
    let nami_md = r#"# NAMI.md

このファイルはエージェントのルールと指示を定義します。

## 基本ルール

- 日本語で応答すること
- ユーザーの要求を正確に理解すること
- 実行可能なタスクは即座に実行すること

## コード生成ルール

- コードは読みやすく保守性の高いものを生成すること
- コメントを適切に記述すること
- エラーハンドリングを必ず含めること

## 制約事項

- 機密情報を出力しないこと
- 不確かな情報は推測せず、確認を求めること
"#;
    fs::write(nami_dir.join("NAMI.md"), nami_md)?;
    
    // Create sample skill
    let skill_dir = nami_dir.join("skills").join("sample");
    fs::create_dir_all(&skill_dir)?;
    
    let skill_md = r#"---
name: sample
description: サンプルスキル
version: 1.0.0
tools:
  - filesystem
---

# Sample Skill

これはサンプルのスキルです。

## 使用方法

このスキルは基本的なファイル操作をサポートします。
"#;
    fs::write(skill_dir.join("SKILL.md"), skill_md)?;
    
    let skill_py = r#"import json
import sys

def main():
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        request = json.load(f)
    
    query = request.get("input", {}).get("query", "")
    
    result = {
        "success": True,
        "result": {
            "message": f"Sample skill processed: {query}",
            "timestamp": "2024-01-01T00:00:00Z"
        }
    }
    
    print(json.dumps(result, ensure_ascii=False, indent=2))

if __name__ == "__main__":
    main()
"#;
    fs::write(skill_dir.join("main.py"), skill_py)?;
    
    Ok(())
}

async fn build_runtime(config: &Config, root: &std::path::Path) -> Result<Box<dyn AgentRuntime>> {
    if matches!(config.model.provider.as_str(), "local" | "deterministic" | "") {
        return Ok(Box::new(DeterministicRuntime));
    }
    
    let mut runtime = AutoAgentsRuntime::new(&config.model.provider, &config.model.model);
    if let Some(api_key_env) = config.model.api_key_env.as_deref() {
        runtime = runtime.with_api_key_env(api_key_env);
    }
    if let Some(base_url) = config.model.base_url.as_deref() {
        runtime = runtime.with_base_url(base_url);
    }
    
    // Load MCP tools if MCP servers are configured
    if !config.mcp.servers.is_empty() {
        let server_names: Vec<String> = config
            .mcp
            .servers
            .iter()
            .map(|server| server.name.clone())
            .collect();
        runtime = runtime.with_mcp_tools(root, &server_names).await?;
    }
    
    Ok(Box::new(runtime))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = env::current_dir()?;
    load_dotenv_from_project_root(&root)?;
    let config = if let Some(config_path) = &cli.config {
        Config::load_from_path(config_path)?
    } else {
        Config::load_from(&root)?
    };
    let store = SessionStore::new(&root);

    match cli.command {
        Commands::Chat(command) => match command.command {
            Some(ChatSubcommand::Resume { session_id }) => {
                let session = store.resume(&session_id)?;
                println!("resumed session {}", session.metadata.session_id);
            }
            None => {
                if let Some(message) = command.message.as_deref() {
                    let (session, context) = if let Some(session_id) = command.resume.as_deref() {
                        let session = store.resume(session_id)?;
                        let messages = session.load_messages()?;
                        let context = messages.into_iter().map(|msg| AgentOutput {
                            agent: msg.agent.unwrap_or_else(|| msg.role.clone()),
                            content: msg.content,
                        }).collect();
                        (session, context)
                    } else {
                        let session = store.create(&config.project.name)?;
                        (session, Vec::new())
                    };
                    
                    session.append_message(&MessageRecord::user(message))?;
                    session.append_event(&EventRecord::new("USER_MESSAGE"))?;

                    let runtime = build_runtime(&config, &root).await?;
                    let output = runtime
                        .run(AgentInput {
                            agent: "assistant".to_string(),
                            task: message.to_string(),
                            context,
                            system_prompt: config.system_prompt.clone(),
                            rules: config.rules.clone(),
                            skills: config.skills.clone(),
                            mcp: config
                                .mcp
                                .servers
                                .iter()
                                .map(|server| server.name.clone())
                                .collect(),
                        })
                        .await?;

                    session.append_message(&MessageRecord::assistant("assistant", &output.content))?;
                    session.append_event(&EventRecord::new("ASSISTANT_REPLY"))?;
                    println!("{}", output.content);
                    println!("session {}", session.metadata.session_id);
                } else {
                    let session = store.create(&config.project.name)?;
                    println!("created session {}", session.metadata.session_id);
                }
            }
        },
        Commands::Session(command) => match command.command {
            SessionSubcommand::List => {
                for metadata in store.list()? {
                    println!(
                        "{}\t{}\t{}",
                        metadata.session_id,
                        metadata.project,
                        metadata.created_at.to_rfc3339()
                    );
                }
            }
            SessionSubcommand::Show { session_id } => {
                let summary = store.show(&session_id)?;
                println!("session: {}", summary.metadata.session_id);
                println!("project: {}", summary.metadata.project);
                println!("messages: {}", summary.message_count);
                println!("events: {}", summary.event_count);
                println!("state: {}", serde_json::to_string_pretty(&summary.state)?);
            }
            SessionSubcommand::Delete { session_id } => {
                store.delete(&session_id)?;
                println!("deleted session {}", session_id);
            }
        },
        Commands::Skill(command) => match command.command {
            SkillSubcommand::Run { name, input } => {
                let input: Value = serde_json::from_str(&input)?;
                let response = SkillRunner::new(&root).run(&name, input).await?;
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
        },
        Commands::Workflow(command) => match command.command {
            WorkflowSubcommand::Run { task } => {
                let runtime = build_runtime(&config, &root).await?;
                let runner = WorkflowRunner::new(&store, &config, runtime.as_ref());
                let session = runner.run(&task).await?;
                println!(
                    "workflow completed in session {}",
                    session.metadata.session_id
                );
            }
        },
        Commands::Mcp(command) => match command.command {
            McpSubcommand::List => {
                let manager = McpManager::from_project_root(&root)?;
                manager.validate()?;
                for server in manager.servers() {
                    let endpoint = server.endpoint.as_deref().unwrap_or("-");
                    println!("{}\t{:?}\t{}", server.name, server.transport, endpoint);
                }

                #[cfg(feature = "mcp")]
                {
                    if let Ok(mcp_tools) = autoagents_toolkit::mcp::McpTools::from_config_object(&autoagents_toolkit::mcp::McpConfig {
                        servers: manager
                            .servers()
                            .iter()
                            .map(|server| autoagents_toolkit::mcp::McpServerConfig {
                                name: server.name.clone(),
                                protocol: match server.transport {
                                    nami::config::McpTransport::Stdio => "stdio".to_string(),
                                    nami::config::McpTransport::Http => "http".to_string(),
                                    nami::config::McpTransport::Websocket => "http".to_string(),
                                },
                                command: server.endpoint.as_deref().unwrap_or_default().split_whitespace().next().unwrap_or_default().to_string(),
                                args: server
                                    .endpoint
                                    .as_deref()
                                    .unwrap_or_default()
                                    .split_whitespace()
                                    .skip(1)
                                    .map(str::to_string)
                                    .collect(),
                                env: std::collections::HashMap::new(),
                                cwd: None,
                                timeout: 30,
                            })
                            .collect(),
                    })
                    .await
                    {
                        let names = mcp_tools.tool_names().await;
                        if !names.is_empty() {
                            println!("available tools:");
                            for name in names {
                                println!("  - {}", name);
                            }
                        }
                    }
                }
            }
        },
        Commands::Init(InitCommand { path }) => {
            let project_dir = if let Some(p) = path {
                std::path::PathBuf::from(p)
            } else {
                root
            };
            init_project(&project_dir)?;
            println!("Initialized nami project in {}", project_dir.display());
        }
    }

    Ok(())
}
