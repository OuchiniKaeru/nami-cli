use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default = "default_agents")]
    pub agents: BTreeMap<String, AgentConfig>,
}

impl Config {
    pub fn load_from(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let config_dir = crate::config_root(root);
        let path = config_dir.join("agent.yaml");
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Self = serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        config.resolve(&config_dir)
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Self = serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        config.resolve(parent)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            project: ProjectConfig::default(),
            model: ModelConfig::default(),
            system_prompt: None,
            rules: Vec::new(),
            skills: Vec::new(),
            mcp: McpConfig::default(),
            agents: default_agents(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default = "default_project_name")]
    pub name: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: default_project_name(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "default_model_provider")]
    pub provider: String,
    #[serde(default = "default_model_name")]
    pub model: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: default_model_provider(),
            model: default_model_name(),
            api_key_env: None,
            base_url: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDefinition {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_model_provider")]
    pub model: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpConfig {
    pub servers: Vec<McpServerConfig>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
        }
    }
}

impl<'de> Deserialize<'de> for McpConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawMcp {
            Names(Vec<String>),
            Map(BTreeMap<String, RawMcpServer>),
        }

        match RawMcp::deserialize(deserializer)? {
            RawMcp::Names(names) => Ok(Self {
                servers: names
                    .into_iter()
                    .map(|name| McpServerConfig {
                        name,
                        transport: McpTransport::Stdio,
                        endpoint: None,
                        timeout: None,
                        disabled: false,
                    })
                    .collect(),
            }),
            RawMcp::Map(map) => Ok(Self {
                servers: map
                    .into_iter()
                    .map(|(name, server)| McpServerConfig {
                        name,
                        transport: server.transport,
                        endpoint: server.endpoint,
                        timeout: server.timeout,
                        disabled: server.disabled,
                    })
                    .collect(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawMcpServer {
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    Http,
    Websocket,
}

impl Default for McpTransport {
    fn default() -> Self {
        Self::Stdio
    }
}

fn default_agents() -> BTreeMap<String, AgentConfig> {
    ["planner", "coder", "reviewer"]
        .into_iter()
        .map(|name| {
            (
                name.to_string(),
                AgentConfig {
                    model: default_model_provider(),
                    skills: Vec::new(),
                    system_prompt: None,
                    rules: Vec::new(),
                },
            )
        })
        .collect()
}

impl Config {
    pub fn resolve(&self, root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let mut resolved = self.clone();

        if let Some(prompt) = self.system_prompt.as_ref() {
            resolved.system_prompt = Some(resolve_text_value(root, prompt)?);
        }

        resolved.rules = self
            .rules
            .iter()
            .map(|rule| resolve_text_value(root, rule))
            .collect::<Result<Vec<_>>>()?;

        resolved.skills = self
            .skills
            .iter()
            .map(|skill| resolve_skill_name(root, skill))
            .collect::<Result<Vec<_>>>()?;

        Ok(resolved)
    }
}

fn resolve_text_value(root: &Path, value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let candidate = PathBuf::from(trimmed);
    let full_path = if candidate.is_absolute() {
        candidate
    } else {
        root.join(&candidate)
    };

    if full_path.exists() {
        return fs::read_to_string(&full_path)
            .with_context(|| format!("failed to read {}", full_path.display()));
    }

    if trimmed.contains('\n') || trimmed.contains(".md") || trimmed.contains('/') || trimmed.contains('\\') {
        return Ok(trimmed.to_string());
    }

    Ok(trimmed.to_string())
}

fn resolve_skill_name(root: &Path, skill: &str) -> Result<String> {
    let skill_dir = crate::nami_root(root).join("skills").join(skill);
    let skill_md = skill_dir.join("SKILL.md");
    if skill_md.exists() {
        let frontmatter = load_skill_frontmatter(&skill_md)?;
        if let Some(definition) = frontmatter {
            return Ok(if definition.name.is_empty() {
                skill.to_string()
            } else {
                definition.name
            });
        }
    }

    Ok(skill.to_string())
}

fn load_skill_frontmatter(path: &Path) -> Result<Option<SkillDefinition>> {
    let contents = fs::read_to_string(path)?;
    let Some(contents) = contents.strip_prefix("---\n") else {
        return Ok(None);
    };

    let Some((frontmatter, _)) = contents.split_once("\n---") else {
        return Ok(None);
    };

    let definition: SkillDefinition = serde_yaml::from_str(frontmatter)
        .with_context(|| format!("failed to parse skill frontmatter {}", path.display()))?;
    Ok(Some(definition))
}

fn default_project_name() -> String {
    "my-agent-project".to_string()
}

fn default_model_provider() -> String {
    "local".to_string()
}

fn default_model_name() -> String {
    "deterministic".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn load_from_returns_default_when_agent_yaml_is_missing() {
        let dir = tempdir().unwrap();

        let config = Config::load_from(dir.path()).unwrap();

        assert_eq!(config.project.name, "my-agent-project");
        assert_eq!(config.model.provider, "local");
        assert_eq!(config.agents.len(), 3);
        assert!(config.agents.contains_key("planner"));
        assert!(config.agents.contains_key("coder"));
        assert!(config.agents.contains_key("reviewer"));
    }

    #[test]
    fn load_from_parses_agent_yaml() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("agent.yaml"),
            r#"
project:
  name: ai-kanban
model:
  provider: gemini
  model: gemini-3
skills:
  - github
mcp:
  - filesystem
agents:
  planner:
    model: gemini
    skills:
      - browser
"#,
        )
        .unwrap();

        let config = Config::load_from(dir.path()).unwrap();

        assert_eq!(config.project.name, "ai-kanban");
        assert_eq!(config.model.provider, "gemini");
        assert_eq!(config.model.model, "gemini-3");
        assert_eq!(config.skills, vec!["github"]);
        assert_eq!(config.mcp.servers[0].name, "filesystem");
        assert_eq!(config.mcp.servers[0].transport, McpTransport::Stdio);
        assert_eq!(config.agents["planner"].skills, vec!["browser"]);
    }

    #[test]
    fn load_from_parses_map_style_mcp() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("agent.yaml"),
            r#"
project:
  name: ai-kanban
mcp:
  filesystem:
    transport: stdio
  github:
    transport: http
    endpoint: http://localhost:8080
"#,
        )
        .unwrap();

        let config = Config::load_from(dir.path()).unwrap();

        assert_eq!(config.mcp.servers.len(), 2);
        assert_eq!(config.mcp.servers[0].name, "filesystem");
        assert_eq!(config.mcp.servers[0].transport, McpTransport::Stdio);
        assert_eq!(config.mcp.servers[1].name, "github");
        assert_eq!(config.mcp.servers[1].transport, McpTransport::Http);
        assert_eq!(
            config.mcp.servers[1].endpoint.as_deref(),
            Some("http://localhost:8080")
        );
    }

    #[test]
    fn load_from_parses_autoagents_model_options() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("agent.yaml"),
            r#"
model:
  provider: openai
  model: gpt-4o-mini
  api_key_env: MY_OPENAI_KEY
  base_url: http://localhost:11434
"#,
        )
        .unwrap();

        let config = Config::load_from(dir.path()).unwrap();

        assert_eq!(config.model.provider, "openai");
        assert_eq!(config.model.model, "gpt-4o-mini");
        assert_eq!(config.model.api_key_env.as_deref(), Some("MY_OPENAI_KEY"));
        assert_eq!(
            config.model.base_url.as_deref(),
            Some("http://localhost:11434")
        );
    }

    #[test]
    fn load_from_path_reads_explicit_yaml_file() {
        let dir = tempdir().unwrap();
        let config_dir = dir.path().join("custom-config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("agent.yaml"),
            r#"
project:
  name: custom-project
model:
  provider: openai
  model: gpt-4o-mini
"#,
        )
        .unwrap();

        let config = Config::load_from_path(config_dir.join("agent.yaml")).unwrap();

        assert_eq!(config.project.name, "custom-project");
        assert_eq!(config.model.provider, "openai");
        assert_eq!(config.model.model, "gpt-4o-mini");
    }
}
