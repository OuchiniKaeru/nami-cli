use anyhow::Context;
use clap::{Arg, Command};
use nami::agent::Agent;
use nami::config::load;
use nami::mcp::McpClient;
use nami::session::SessionRecord;
use nami::skill::SkillRegistry;
use nami::utils::logger::init as init_logger;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = Command::new("nami")
        .version("0.2.0")
        .about("Rust-only lightweight CLI AI agent")
        .subcommand(
            Command::new("init")
                .about("Initialize a nami project in the executable directory"),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .help("Path to config YAML")
                .default_value("config/config.yaml"),
        )
        .arg(Arg::new("prompt").help("Prompt text").index(1))
        .arg(
            Arg::new("no-session")
                .long("no-session")
                .help("Disable session save")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("resume")
                .long("resume")
                .short('r')
                .help("Resume from a previous session file or id")
                .value_name("SESSION"),
        )
        .get_matches();

    if let Some(_init) = matches.subcommand_matches("init") {
        let exe_dir = nami::config::current_exe_dir()?;
        let nami_dir = nami::config::init_nami_project(&exe_dir)?;
        println!("Initialized nami project at {}", nami_dir.display());
        println!("Configuration files written:");
        println!("  - {}", nami_dir.join(".env").display());
        println!("  - {}", nami_dir.join(".env.sample").display());
        println!("  - {}", nami_dir.join("NAMI.md").display());
        println!(
            "  - {}",
            nami_dir.join("config").join("config.yaml").display()
        );
        return Ok(());
    }

    let config_path = resolve_config_path(&matches)?;
    let base_dir = nami::config::resolve_config_base_dir(&config_path);

    // プロジェクト基準ディレクトリの .env を読み込み、なければカレントディレクトリの .env を試す
    let env_path = base_dir.join(".env");
    if env_path.exists() {
        let _ = dotenvy::from_path(&env_path);
    } else {
        dotenvy::dotenv().ok();
    }

    let config_path_str = config_path
        .to_str()
        .context("config path is not valid UTF-8")?;
    let mut cfg = load(Some(config_path_str))?;

    if matches.get_flag("no-session") {
        cfg.session.save = false;
    }

    let loaded_session = if let Some(resume) = matches.get_one::<String>("resume") {
        let path = resolve_session_path(resume, &cfg);
        let session = SessionRecord::load(&path)?;
        tracing::info!(session_id = session.id, "resuming session");
        cfg = session.config.clone();
        Some(session)
    } else {
        None
    };

    nami::config::ensure_directories(&cfg)?;
    init_logger(&cfg.logging.directory, &cfg.logging.level)?;

    let mut registry = SkillRegistry::new();
    let skills_base_dir = cfg.base_dir.join("skills");
    for name in &cfg.skills {
        let skill: Option<std::sync::Arc<dyn nami::skill::Skill>> = match name.as_str() {
            "filesystem" => Some(std::sync::Arc::new(nami::skill::filesystem::FilesystemSkill::new())),
            "shell" => Some(std::sync::Arc::new(nami::skill::shell::ShellSkill::new())),
            "browser" => Some(std::sync::Arc::new(nami::skill::browser::BrowserSkill::new())),
            "search" => Some(std::sync::Arc::new(nami::skill::search::SearchSkill::new())),
            "http" => Some(std::sync::Arc::new(nami::skill::http::HttpSkill::new())),
            _ => {
                let skill_dir = skills_base_dir.join(name);
                if skill_dir.join("SKILL.md").exists() {
                    match nami::skill::external::ExternalSkill::from_dir(&skill_dir) {
                        Ok(external) => {
                            tracing::info!(skill = name, "loaded external skill from {:?}", skill_dir);
                            Some(std::sync::Arc::new(external))
                        }
                        Err(e) => {
                            tracing::warn!(skill = name, "failed to load external skill: {}", e);
                            None
                        }
                    }
                } else {
                    tracing::warn!("unknown skill configured: {}", name);
                    None
                }
            }
        };
        if let Some(s) = skill {
            registry.register(s);
        }
    }

    let mcp = McpClient::new();

    for server in &cfg.mcp.servers {
        match server.transport {
            nami::models::config::McpTransport::Stdio => {
                let command = server.command.as_deref().unwrap_or_default();
                let args = server.args.as_deref().unwrap_or_default();
                mcp.connect_stdio(&server.name, command, args, server.env.clone()).await?;
            }
            nami::models::config::McpTransport::Http => {
                let url = server.url.as_deref().context("http transport requires url")?;
                mcp.connect_http(&server.name, url).await?;
            }
        }
    }

    let mut agent = Agent::new(cfg.clone(), registry, mcp).await?;

    if let Some(session) = loaded_session {
        agent.load_session(session);
        if matches.get_flag("no-session") {
            agent.session = None;
        }
    }

    let prompt = matches
        .get_one::<String>("prompt")
        .cloned()
        .unwrap_or_default();

    if prompt.is_empty() {
        eprintln!("nami requires a prompt. Example: cargo run -- \"Hello\"");
        std::process::exit(1);
    }

    println!("Running with provider: {}", cfg.provider.kind);
    println!("Model: {}\n", cfg.provider.model);

    let answer = agent.run(&prompt).await?;

    if cfg.stream {
        // ストリーミング中に既にレスポンス内容は表示されている
        println!();
    } else if let Some(text) = answer {
        println!("\n{}", text);
    }

    let m = &agent.metrics;
    println!("\nSession finished\n");
    println!("Iterations      : {}", m.iterations);
    println!("LLM Calls       : {}", m.llm_calls);
    println!("Tool Calls      : {}", m.tool_calls);
    println!("MCP Calls       : {}", m.mcp_calls);
    if let Some(usage) = &m.usage {
        println!("Input Tokens    : {}", usage.input_tokens);
        println!("Output Tokens   : {}", usage.output_tokens);
        println!("Total Tokens    : {}", usage.total_tokens);
        if let Some(c) = usage.cached_tokens {
            println!("Cached Tokens   : {}", c);
        }
        if let Some(r) = usage.reasoning_tokens {
            println!("Reasoning       : {}", r);
        }
    }
    if let Some(ms) = m.elapsed_ms {
        println!("Elapsed         : {:.1} sec", ms as f64 / 1000.0);
    }

    let session_id = agent
        .session
        .as_ref()
        .map(|s| s.id.as_str())
        .unwrap_or("none");
    println!("Session ID      : {}", session_id);

    Ok(())
}

fn resolve_config_path(matches: &clap::ArgMatches) -> anyhow::Result<PathBuf> {
    if let Some(p) = matches.get_one::<String>("config") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
        // ユーザーがデフォルト値のままの場合は、以下の解決ルールに従う
        if p != "config/config.yaml" {
            return Ok(path);
        }
    }

    let cwd = std::env::current_dir()?;
    let local_nami = cwd.join(".nami");
    let local_nami_config = local_nami.join("config").join("config.yaml");

    // ローカルに .nami/config/config.yaml があればそれを使う
    if local_nami_config.exists() {
        return Ok(local_nami_config);
    }

    // .nami フォルダがなければカレントディレクトリに .nami/config を作成する
    if !local_nami.exists() {
        std::fs::create_dir_all(&local_nami.join("config"))
            .context("failed to create .nami/config directory")?;
    }

    // カレントディレクトリの config/ フォルダに yaml があれば優先する
    let local_config_dir = cwd.join("config");
    if local_config_dir.is_dir() {
        let mut yamls: Vec<PathBuf> = std::fs::read_dir(&local_config_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext == "yaml" || ext == "yml")
                    .unwrap_or(false)
            })
            .collect();
        yamls.sort();
        if let Some(first) = yamls.first() {
            return Ok(first.clone());
        }
    }

    // 最後に実行ファイルがある場所の .nami を使う
    let exe_dir = nami::config::current_exe_dir()?;
    let exe_config = exe_dir.join(".nami").join("config").join("config.yaml");
    if exe_config.exists() {
        return Ok(exe_config);
    }

    anyhow::bail!(
        "Configuration not found. Please run `nami init` to create a default config in the executable directory, or specify --config."
    )
}

fn resolve_session_path(resume: &str, cfg: &nami::models::config::AppConfig) -> PathBuf {
    let path = PathBuf::from(resume);
    if path.exists() || path.extension().map(|e| e == "json").unwrap_or(false) {
        path
    } else {
        SessionRecord::path(&cfg.session.directory, resume)
    }
}
