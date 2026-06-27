use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::models::config::{AppConfig, ProviderConfig, ProviderKind};

const DEFAULT_CONFIG_FILE: &str = "config/config.yaml";

const DEFAULT_CONFIG_YAML: &str = r#"provider:
  type: openrouter
  model: gpt-5
  # api-key: OPENROUTER_API_KEY

temperature: 0.2
max_tokens: 8000
max_iterations: 20

stream: true

session:
  save: true
  directory: sessions

system_prompt: |
  あなたは、優秀なAIエージェントです。
  日本語で応答すること。

rules:
  - NAMI.md

memory:
  directory: memory
  file: memory/memory.jsonl

logging:
  directory: logs
  level: info

skills:
  - filesystem
  - shell
  - browser
  - search
  - http

mcp:
  servers: []
"#;

const DEFAULT_ENV: &str = r#"# Copy this file to .env and fill in your API keys.
# OPENROUTER_API_KEY=sk-or-xxx
# OPENAI_API_KEY=sk-xxx
# NAMI_PROVIDER_TYPE=openrouter
# NAMI_PROVIDER_MODEL=gpt-5
"#;

const DEFAULT_ENV_SAMPLE: &str = r#"# Copy this file to .env and fill in your API keys.
# OPENROUTER_API_KEY=sk-or-xxx
# OPENAI_API_KEY=sk-xxx
# NAMI_PROVIDER_TYPE=openrouter
# NAMI_PROVIDER_MODEL=gpt-5
"#;

const DEFAULT_NAMI_MD: &str = r#"# NAMI Rules

- 日本語で応答すること
- 必要に応じて Skill / MCP を利用すること
"#;

pub fn load(path: Option<&str>) -> anyhow::Result<AppConfig> {
    let path = path.unwrap_or(DEFAULT_CONFIG_FILE);
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("config file not found or unreadable: {}", path))?;
    let mut cfg: AppConfig = serde_yaml::from_str(&raw)
        .with_context(|| format!("invalid yaml config: {}", path))?;
    let config_file = Path::new(path).to_path_buf();
    let config_file = config_file.canonicalize().unwrap_or(config_file);
    cfg.base_dir = resolve_config_base_dir(&config_file);
    resolve_relative_paths(&mut cfg);
    apply_env_overrides(&mut cfg);
    Ok(cfg)
}

/// 設定ファイルパスからプロジェクトの基準ディレクトリを決定する。
/// `config/config.yaml` または `.nami/config/config.yaml` の場合はその親ディレクトリを、
/// それ以外では設定ファイルのあるディレクトリを基準とする。
pub fn resolve_config_base_dir(config_file: impl AsRef<Path>) -> PathBuf {
    let config_file = config_file.as_ref();
    let config_dir = config_file.parent().unwrap_or_else(|| Path::new("."));
    let is_nested_config = config_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n == "config" || n == ".nami")
        .unwrap_or(false);
    if is_nested_config {
        config_dir.parent().unwrap_or(config_dir).to_path_buf()
    } else {
        config_dir.to_path_buf()
    }
}

/// 相対パスの各種ディレクトリ・ファイル設定を base_dir からの絶対パスに解決する。
fn resolve_relative_paths(cfg: &mut AppConfig) {
    let base = &cfg.base_dir;
    cfg.session.directory = resolve_path(base, &cfg.session.directory);
    cfg.memory.directory = resolve_path(base, &cfg.memory.directory);
    cfg.memory.file = resolve_path(base, &cfg.memory.file);
    cfg.logging.directory = resolve_path(base, &cfg.logging.directory);
    for rule in &mut cfg.rules {
        *rule = resolve_path(base, rule);
    }
}

fn resolve_path(base_dir: &Path, path: &str) -> String {
    if path.is_empty() {
        return path.to_string();
    }
    let p = Path::new(path);
    if p.is_absolute() {
        path.to_string()
    } else {
        base_dir.join(p).to_string_lossy().to_string()
    }
}

/// 実行中のEXEファイルがあるディレクトリを返す。
pub fn current_exe_dir() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("failed to get current executable path")?;
    exe.parent()
        .map(|p| p.to_path_buf())
        .context("failed to get executable directory")
}

/// 指定ディレクトリ（通常はEXEディレクトリ）に `.nami` プロジェクトを初期化する。
pub fn init_nami_project(exe_dir: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let nami_dir = exe_dir.as_ref().join(".nami");
    let dirs = [
        nami_dir.join("config"),
        nami_dir.join("logs"),
        nami_dir.join("memory"),
        nami_dir.join("sessions"),
        nami_dir.join("skills"),
    ];
    for d in &dirs {
        std::fs::create_dir_all(d)
            .with_context(|| format!("failed to create directory: {}", d.display()))?;
    }

    let config_path = nami_dir.join("config").join("config.yaml");
    if !config_path.exists() {
        std::fs::write(&config_path, DEFAULT_CONFIG_YAML)
            .with_context(|| format!("failed to write default config: {}", config_path.display()))?;
    }

    let env_path = nami_dir.join(".env");
    if !env_path.exists() {
        std::fs::write(&env_path, DEFAULT_ENV)
            .with_context(|| format!("failed to write .env: {}", env_path.display()))?;
    }

    let env_sample_path = nami_dir.join(".env.sample");
    if !env_sample_path.exists() {
        std::fs::write(&env_sample_path, DEFAULT_ENV_SAMPLE)
            .with_context(|| format!("failed to write .env.sample: {}", env_sample_path.display()))?;
    }

    let nami_md_path = nami_dir.join("NAMI.md");
    if !nami_md_path.exists() {
        std::fs::write(&nami_md_path, DEFAULT_NAMI_MD)
            .with_context(|| format!("failed to write NAMI.md: {}", nami_md_path.display()))?;
    }

    Ok(nami_dir)
}

/// NAMI_* 環境変数で YAML の設定を上書きする。
fn apply_env_overrides(cfg: &mut AppConfig) {
    if let Ok(v) = std::env::var("NAMI_PROVIDER_TYPE") {
        if let Ok(kind) = parse_provider_kind(&v) {
            cfg.provider.kind = kind;
        }
    }
    if let Ok(v) = std::env::var("NAMI_PROVIDER_MODEL") {
        cfg.provider.model = v;
    }
    if let Ok(v) = std::env::var("NAMI_PROVIDER_API_KEY") {
        cfg.provider.api_key = Some(v);
    }
    if let Ok(v) = std::env::var("NAMI_PROVIDER_BASE_URL") {
        cfg.provider.base_url = Some(v);
    }
    if let Ok(v) = std::env::var("NAMI_TEMPERATURE") {
        if let Ok(t) = v.parse() {
            cfg.temperature = t;
        }
    }
    if let Ok(v) = std::env::var("NAMI_MAX_TOKENS") {
        if let Ok(m) = v.parse() {
            cfg.max_tokens = m;
        }
    }
    if let Ok(v) = std::env::var("NAMI_MAX_ITERATIONS") {
        if let Ok(m) = v.parse() {
            cfg.max_iterations = m;
        }
    }
    if let Ok(v) = std::env::var("NAMI_STREAM") {
        cfg.stream = v.parse().unwrap_or(cfg.stream);
    }
}

fn parse_provider_kind(s: &str) -> anyhow::Result<ProviderKind> {
    match s.to_lowercase().as_str() {
        "openai" => Ok(ProviderKind::OpenAI),
        "openrouter" => Ok(ProviderKind::OpenRouter),
        "azure_openai" | "azureopenai" => Ok(ProviderKind::AzureOpenAI),
        "groq" => Ok(ProviderKind::Groq),
        "anthropic" => Ok(ProviderKind::Anthropic),
        "gemini" => Ok(ProviderKind::Gemini),
        "custom" => Ok(ProviderKind::Custom),
        other => anyhow::bail!("unknown provider kind: {}", other),
    }
}

/// provider.type に対応する接続情報を補完する。
/// Azure OpenAI の場合は api_version や base_url の整形も行う。
pub fn resolve_provider(mut cfg: ProviderConfig) -> anyhow::Result<ProviderConfig> {
    // YAML 内で api_key に環境変数名が指定されていれば、実際の値に解決する。
    // 例: api_key: OPENROUTER_API_KEY または api_key: $OPENROUTER_API_KEY
    if let Some(key) = cfg.api_key.as_deref() {
        cfg.api_key = Some(resolve_api_key_value(key));
    }

    if cfg.api_key.as_deref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        if let Ok(v) = std::env::var(format!(
            "{}_API_KEY",
            provider_env_prefix(&cfg.kind)
        )) {
            if !v.is_empty() {
                cfg.api_key = Some(v);
            }
        }
    }

    match cfg.kind {
        crate::models::config::ProviderKind::AzureOpenAI => resolve_azure(&cfg),
        _ => Ok(cfg),
    }
}

/// api_key の文字列値を解決する。
/// `$NAME` または環境変数名らしい形式（大文字・数字・アンダースコアのみ）の場合、
/// 対応する環境変数の値を返す。環境変数が存在しない場合は元の値をそのまま返す。
fn resolve_api_key_value(value: &str) -> String {
    let var_name = if let Some(stripped) = value.strip_prefix('$') {
        stripped
    } else if is_likely_env_var_name(value) {
        value
    } else {
        return value.to_string();
    };

    std::env::var(var_name).unwrap_or_else(|_| value.to_string())
}

fn is_likely_env_var_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn resolve_azure(provider: &ProviderConfig) -> anyhow::Result<ProviderConfig> {
    let mut p = provider.clone();
    let base = p.base_url.take().unwrap_or_default();
    let api_version = p.api_version.clone().unwrap_or_else(|| "2024-10-21".to_string());
    let ok = p.api_key.as_ref().map(|k| !k.trim().is_empty()).unwrap_or(false);

    if !ok {
        anyhow::bail!("azure_openai requires api_key via config or AZURE_OPENAI_API_KEY");
    }

    // configure 互換を期待する最小形式
    // https://{resource}.openai.azure.com/openai/deployments/{deployment}/chat/completions?api-version={api_version}
    if !base.is_empty() {
        p.base_url = Some(base);
    } else {
        anyhow::bail!("azure_openai requires base_url via config");
    }

    // OpenAI互換では model に deployment 名を入れて運用する。
    p.api_version = Some(api_version);
    Ok(p)
}

fn provider_env_prefix(kind: &crate::models::config::ProviderKind) -> &'static str {
    use crate::models::config::ProviderKind::*;
    match kind {
        OpenAI => "OPENAI",
        OpenRouter => "OPENROUTER",
        AzureOpenAI => "AZURE_OPENAI",
        Groq => "GROQ",
        Anthropic => "ANTHROPIC",
        Custom => "OPENAI_COMPATIBLE_API",
        Gemini => "GOOGLE_GENERATIVE_AI",
    }
}

pub fn ensure_directories(cfg: &AppConfig) -> anyhow::Result<Vec<PathBuf>> {
    let mut created = Vec::new();
    let dirs: Vec<String> = vec![
        cfg.base_dir.join("config").to_string_lossy().to_string(),
        cfg.session.directory.clone(),
        std::path::Path::new(&cfg.memory.directory)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string()),
        std::path::Path::new(&cfg.memory.file)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string()),
        cfg.logging.directory.clone(),
    ];

    for dir in dirs {
        let p = Path::new(&dir);
        if !p.exists() {
            std::fs::create_dir_all(p)
                .with_context(|| format!("failed to create directory: {}", p.display()))?;
            created.push(p.to_path_buf());
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_api_key_value_reads_env_var_for_dollar_prefix() {
        std::env::set_var("NAMI_TEST_API_KEY_1", "secret-123");
        assert_eq!(
            resolve_api_key_value("$NAMI_TEST_API_KEY_1"),
            "secret-123"
        );
        std::env::remove_var("NAMI_TEST_API_KEY_1");
    }

    #[test]
    fn resolve_api_key_value_reads_env_var_for_uppercase_name() {
        std::env::set_var("NAMI_TEST_API_KEY_2", "secret-456");
        assert_eq!(
            resolve_api_key_value("NAMI_TEST_API_KEY_2"),
            "secret-456"
        );
        std::env::remove_var("NAMI_TEST_API_KEY_2");
    }

    #[test]
    fn resolve_api_key_value_returns_literal_key_unchanged() {
        let key = "sk-or-xxxx";
        assert_eq!(resolve_api_key_value(key), key);
    }

    #[test]
    fn resolve_api_key_value_returns_original_when_env_var_missing() {
        assert_eq!(
            resolve_api_key_value("DEFINITELY_MISSING_ENV_VAR_FOR_TEST"),
            "DEFINITELY_MISSING_ENV_VAR_FOR_TEST"
        );
    }

    #[test]
    fn resolve_provider_uses_config_api_key_env_reference() {
        std::env::set_var("NAMI_TEST_API_KEY_3", "resolved-from-env");
        let cfg = ProviderConfig {
            kind: ProviderKind::OpenRouter,
            model: "test".to_string(),
            api_key: Some("NAMI_TEST_API_KEY_3".to_string()),
            base_url: None,
            api_version: None,
        };
        let resolved = resolve_provider(cfg).unwrap();
        assert_eq!(resolved.api_key.as_deref().unwrap(), "resolved-from-env");
        std::env::remove_var("NAMI_TEST_API_KEY_3");
    }

    #[test]
    fn resolve_provider_falls_back_to_provider_specific_env_var() {
        std::env::set_var("OPENROUTER_API_KEY", "fallback-key");
        let cfg = ProviderConfig {
            kind: ProviderKind::OpenRouter,
            model: "test".to_string(),
            api_key: None,
            base_url: None,
            api_version: None,
        };
        let resolved = resolve_provider(cfg).unwrap();
        assert_eq!(resolved.api_key.as_deref().unwrap(), "fallback-key");
        std::env::remove_var("OPENROUTER_API_KEY");
    }

    #[test]
    fn provider_config_accepts_api_key_hyphen_alias() {
        let cfg: ProviderConfig = serde_yaml::from_str(
            r#"
type: openrouter
model: test
api-key: HYPHEN_ALIAS_KEY
"#,
        )
        .unwrap();
        assert_eq!(cfg.api_key.as_deref().unwrap(), "HYPHEN_ALIAS_KEY");

        std::env::set_var("HYPHEN_ALIAS_KEY", "resolved-hyphen");
        let resolved = resolve_provider(cfg).unwrap();
        assert_eq!(resolved.api_key.as_deref().unwrap(), "resolved-hyphen");
        std::env::remove_var("HYPHEN_ALIAS_KEY");
    }

    #[test]
    fn resolve_config_base_dir_uses_parent_of_config_dir() {
        assert_eq!(
            resolve_config_base_dir(Path::new("/project/config/config.yaml")),
            PathBuf::from("/project")
        );
        assert_eq!(
            resolve_config_base_dir(Path::new("/project/.nami/config/config.yaml")),
            PathBuf::from("/project/.nami")
        );
    }

    #[test]
    fn resolve_config_base_dir_falls_back_to_config_file_dir() {
        assert_eq!(
            resolve_config_base_dir(Path::new("/project/nami.yaml")),
            PathBuf::from("/project")
        );
    }
}
