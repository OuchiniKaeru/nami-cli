use anyhow::Context;
use clap::{Arg, Command};
use futures_util::stream::{FuturesUnordered, StreamExt};
use nami::agent::Agent;
use nami::config::load;
use nami::mcp::McpClient;
use nami::session::SessionRecord;
use nami::skill::SkillRegistry;
use nami::utils::logger::init as init_logger;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
struct SessionSummary {
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
    message_count: usize,
    provider: String,
    model: String,
    stream: bool,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    tool_calls: u32,
    mcp_calls: u32,
    error_count: usize,
    path: String,
    size_bytes: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = Command::new("nami")
        .version("0.2.0")
        .about("Rust-only lightweight CLI AI agent")
        .subcommand(
            Command::new("init")
                .about("Initialize a nami project in the executable directory"),
        )
        .subcommand(
            Command::new("session")
                .about("Manage sessions")
                .subcommand(
                    Command::new("list")
                        .about("List sessions")
                        .arg(
                            Arg::new("json")
                                .long("json")
                                .action(clap::ArgAction::SetTrue),
                        ),
                )
                .subcommand(
                    Command::new("show")
                        .about("Show session details")
                        .arg(
                            Arg::new("id")
                                .required(true)
                                .index(1)
                                .value_name("SESSION_ID"),
                        )
                        .arg(
                            Arg::new("json")
                                .long("json")
                                .action(clap::ArgAction::SetTrue),
                        ),
                )
                .subcommand(
                    Command::new("delete")
                        .about("Delete a session")
                        .arg(
                            Arg::new("id")
                                .required(true)
                                .index(1)
                                .value_name("SESSION_ID"),
                        ),
                )
                .subcommand(
                    Command::new("rename")
                        .about("Rename a session")
                        .arg(
                            Arg::new("id")
                                .required(true)
                                .index(1)
                                .value_name("SESSION_ID"),
                        )
                        .arg(
                            Arg::new("title")
                                .required(true)
                                .index(2)
                                .value_name("TITLE"),
                        ),
                )
                .subcommand(
                    Command::new("export")
                        .about("Export a session")
                        .arg(
                            Arg::new("id")
                                .required(true)
                                .index(1)
                                .value_name("SESSION_ID"),
                        )
                        .arg(
                            Arg::new("format")
                                .required(true)
                                .index(2)
                                .value_name("FORMAT")
                                .help("markdown"),
                        ),
                ),
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
        .arg(
            Arg::new("attach")
                .long("attach")
                .help("Attach files to the prompt (repeatable, supports directories)")
                .value_name("PATH")
                .action(clap::ArgAction::Append),
        )
        .get_matches();

    if let Some(session_cmd) = matches.subcommand_matches("session") {
        return run_session_command(session_cmd, &matches);
    }

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
    let mut tasks = FuturesUnordered::new();

    for server in &cfg.mcp.servers {
        let mcp = &mcp;
        let server = server.clone();
        tasks.push(async move {
            match server.transport {
                nami::models::config::McpTransport::Stdio => {
                    let command = server.command.as_deref().unwrap_or_default();
                    let args = server.args.as_deref().unwrap_or_default();
                    mcp.connect_stdio(&server.name, command, args, server.env.clone())
                        .await?;
                }
                nami::models::config::McpTransport::Http => {
                    let url = server
                        .url
                        .as_deref()
                        .context("http transport requires url")?;
                    mcp.connect_http(&server.name, url).await?;
                }
            }
            Ok::<_, anyhow::Error>(())
        });
    }

    while let Some(result) = tasks.next().await {
        result?;
    }
    drop(tasks);

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

    let raw_attachments: Vec<String> = matches
        .get_many::<String>("attach")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();
    let mut attachments: Vec<nami::models::Attachment> = Vec::new();
    for value in raw_attachments {
        let expanded = expand_attach_value(&value);
        if std::path::Path::new(&expanded).is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(&expanded)
                .context("failed to read attach directory")?
                .filter_map(|e| e.ok())
                .collect();
            entries.sort_by_key(|e| e.path());
            for entry in entries {
                let path = entry.path();
                if path.is_file() {
                    if let Some(attachment) = nami::agent::prompt_parser::build_local_attachment(
                        path.to_string_lossy().as_ref(),
                    ) {
                        attachments.push(attachment);
                    }
                }
            }
        } else {
            for part in expanded.split(',') {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    continue;
                }
                for candidate in expand_glob(trimmed) {
                    if let Some(attachment) =
                        nami::agent::prompt_parser::build_local_attachment(&candidate)
                    {
                        attachments.push(attachment);
                    }
                }
            }
        }
    }

    println!("Running with provider: {}", cfg.provider.kind);
    println!("Model: {}\n", cfg.provider.model);

    let answer = agent.run(&prompt, attachments).await?;

    // Gemini 等、ストリーミング未対応プロバイダーの場合は集約された応答を表示する。
    let actually_streamed = cfg.stream && agent.provider.supports_streaming();
    if actually_streamed {
        // ストリーミング中に既にレスポンス内容は表示されている
        print!("\n");
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

fn run_session_command(
    matches: &clap::ArgMatches,
    root_matches: &clap::ArgMatches,
) -> anyhow::Result<()> {
    let config_path = resolve_config_path(root_matches)?;
    let cfg = load(Some(config_path.to_str().unwrap()))?;

    if let Some(list_matches) = matches.subcommand_matches("list") {
        cmd_list(cfg, list_matches)
    } else if let Some(show_matches) = matches.subcommand_matches("show") {
        cmd_show(cfg, show_matches)
    } else if let Some(delete_matches) = matches.subcommand_matches("delete") {
        cmd_delete(cfg, delete_matches)
    } else if let Some(rename_matches) = matches.subcommand_matches("rename") {
        cmd_rename(cfg, rename_matches)
    } else if let Some(export_matches) = matches.subcommand_matches("export") {
        cmd_export(cfg, export_matches)
    } else {
        anyhow::bail!("unknown session command")
    }
}

fn cmd_list(cfg: nami::models::config::AppConfig, matches: &clap::ArgMatches) -> anyhow::Result<()> {
    let json = matches.get_flag("json");
    let dir = std::path::Path::new(&cfg.session.directory);
    if !dir.exists() {
        if json {
            println!("[]");
        }
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .context("failed to read session directory")?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|e| e == "json").unwrap_or_default())
        .collect();

    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    entries.reverse();

    let mut summaries = Vec::new();
    for entry in entries {
        let path = entry.path();
        let meta = entry.metadata().context("failed to read session metadata")?;
        let mut record = match SessionRecord::load(&path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        record.derive_title();

        let usage = record.metrics.usage;
        summaries.push(SessionSummary {
            id: record.id,
            title: record.title,
            created_at: record.created_at,
            updated_at: record.updated_at,
            message_count: record.messages.len(),
            provider: format!("{}", record.config.provider.kind),
            model: record.config.provider.model,
            stream: record.config.stream,
            input_tokens: usage.as_ref().map(|u| u.input_tokens),
            output_tokens: usage.as_ref().map(|u| u.output_tokens),
            total_tokens: usage.as_ref().map(|u| u.total_tokens),
            tool_calls: record.metrics.tool_calls,
            mcp_calls: record.metrics.mcp_calls,
            error_count: record.errors.len(),
            path: path.to_string_lossy().into_owned(),
            size_bytes: meta.len(),
        });
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&summaries)?);
    } else {
        for s in &summaries {
            println!("{}", s.id);
        }
    }

    Ok(())
}

fn cmd_show(cfg: nami::models::config::AppConfig, matches: &clap::ArgMatches) -> anyhow::Result<()> {
    let json = matches.get_flag("json");
    let id = matches.get_one::<String>("id").unwrap();
    let path = SessionRecord::path(&cfg.session.directory, id);
    let mut record = SessionRecord::load(&path)?;
    record.derive_title();

    if json {
        let mut redacted = record.clone();
        redacted.config.provider.api_key = None;
        println!("{}", serde_json::to_string_pretty(&redacted)?);
    } else {
        println!("ID: {}", record.id);
        println!("Title: {}", record.title);
        println!("Provider: {}", record.config.provider.kind);
        println!("Model: {}", record.config.provider.model);
        println!("Stream: {}", record.config.stream);
        if let Some(u) = &record.metrics.usage {
            println!("Input Tokens: {}", u.input_tokens);
            println!("Output Tokens: {}", u.output_tokens);
            println!("Total Tokens: {}", u.total_tokens);
        }
        println!("Tool Calls: {}", record.metrics.tool_calls);
        println!("MCP Calls: {}", record.metrics.mcp_calls);
        println!("Errors: {}", record.errors.len());
        println!("Messages:");
        for (i, msg) in record.messages.iter().enumerate() {
            println!(
                "  [{}] {:?}: {}",
                i,
                msg.role,
                msg.content.as_deref().unwrap_or("")
            );
        }
    }

    Ok(())
}

fn cmd_delete(cfg: nami::models::config::AppConfig, matches: &clap::ArgMatches) -> anyhow::Result<()> {
    let id = matches.get_one::<String>("id").unwrap();
    let path = SessionRecord::path(&cfg.session.directory, id);
    std::fs::remove_file(&path)?;
    println!("deleted: {}", id);
    Ok(())
}

fn cmd_rename(cfg: nami::models::config::AppConfig, matches: &clap::ArgMatches) -> anyhow::Result<()> {
    let id = matches.get_one::<String>("id").unwrap();
    let title = matches.get_one::<String>("title").unwrap();
    let path = SessionRecord::path(&cfg.session.directory, id);
    let mut record = SessionRecord::load(&path)?;
    record.title = title.clone();
    record.save()?;
    println!("renamed: {} -> {}", id, title);
    Ok(())
}

fn cmd_export(cfg: nami::models::config::AppConfig, matches: &clap::ArgMatches) -> anyhow::Result<()> {
    let id = matches.get_one::<String>("id").unwrap();
    let format = matches.get_one::<String>("format").unwrap();
    let path = SessionRecord::path(&cfg.session.directory, id);
    let record = SessionRecord::load(&path)?;

    match format.as_str() {
        "markdown" => export_markdown(&record),
        other => anyhow::bail!("unsupported format: {}", other),
    }
}

fn export_markdown(record: &SessionRecord) -> anyhow::Result<()> {
    println!("# {}", record.title);
    println!("");
    println!("Session ID: {}", record.id);
    println!("Provider: {}", record.config.provider.kind);
    println!("Model: {}", record.config.provider.model);
    println!("Stream: {}", record.config.stream);
    println!("Created: {}", record.created_at);
    if let Some(u) = &record.metrics.usage {
        println!("Input Tokens: {}", u.input_tokens);
        println!("Output Tokens: {}", u.output_tokens);
        println!("Total Tokens: {}", u.total_tokens);
    }
    println!("");
    println!("---");
    println!("");

    for msg in &record.messages {
        match msg.role {
            nami::models::message::Role::System => {
                println!("**[System]**");
                if let Some(c) = &msg.content {
                    println!("{}", c);
                }
            }
            nami::models::message::Role::User => {
                println!("**[User]**");
                if let Some(c) = &msg.content {
                    println!("{}", c);
                }
            }
            nami::models::message::Role::Assistant => {
                println!("**[Assistant]**");
                if let Some(c) = &msg.content {
                    println!("{}", c);
                }
                if let Some(rc) = &msg.reasoning_content {
                    if !rc.is_empty() {
                        println!("<thinking>\n{}\n</thinking>", rc);
                    }
                }
            }
            nami::models::message::Role::Tool => {
                println!("**[Tool: {}]**", msg.name.as_deref().unwrap_or(""));
                if let Some(c) = &msg.content {
                    println!("{}", c);
                }
            }
        }
        println!("");
    }

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

fn expand_attach_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('~') {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let suffix = trimmed.strip_prefix("~/").unwrap_or(trimmed);
            let expanded = std::path::PathBuf::from(home).join(suffix);
            return expanded.to_string_lossy().to_string();
        }
    }
    trimmed.to_string()
}

fn expand_glob(pattern: &str) -> Vec<String> {
    let mut matches = Vec::new();
    if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
        if let Ok(glob_matches) = glob::glob(pattern) {
            for entry in glob_matches {
                if let Ok(path) = entry {
                    matches.push(path.to_string_lossy().to_string());
                }
            }
        }
    } else {
        matches.push(pattern.to_string());
    }
    matches
}
