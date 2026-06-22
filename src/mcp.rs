use crate::config::{Config, McpServerConfig, McpTransport};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct McpManager {
    servers: Vec<McpServerConfig>,
}

#[derive(Debug, Deserialize)]
struct McpSettingsFile {
    #[serde(default, alias = "mcpServers")]
    mcp_servers: std::collections::BTreeMap<String, McpSettingsEntry>,
}

#[derive(Debug, Deserialize)]
struct McpSettingsEntry {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    headers: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    disable: bool,
}

impl McpManager {
    pub fn from_config(config: &Config) -> Self {
        Self {
            servers: config.mcp.servers.clone(),
        }
    }

    pub fn from_names(names: &[String]) -> Self {
        Self {
            servers: names
                .iter()
                .map(|name| McpServerConfig {
                    name: name.clone(),
                    transport: McpTransport::Stdio,
                    endpoint: None,
                    timeout: None,
                    disabled: false,
                })
                .collect(),
        }
    }

    pub fn from_project_root(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let settings_path = crate::nami_root(root).join("mcp_setting.json");
        if settings_path.exists() {
            return Self::from_settings_file(&settings_path);
        }

        let legacy_path = root.join("mcp_setting.json");
        if legacy_path.exists() {
            return Self::from_settings_file(&legacy_path);
        }

        Ok(Self::from_config(&Config::default()))
    }

    pub fn from_settings_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let settings: McpSettingsFile = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        let servers = settings
            .mcp_servers
            .into_iter()
            .map(|(name, entry)| {
                if entry.url.is_some() {
                    McpServerConfig {
                        name: name.clone(),
                        transport: if entry.r#type.as_deref() == Some("streamableHttp") || entry.url.as_deref().is_some_and(|value| value.starts_with("http")) {
                            McpTransport::Http
                        } else {
                            McpTransport::Websocket
                        },
                        endpoint: entry.url,
                        timeout: entry.timeout,
                        disabled: entry.disable,
                    }
                } else {
                    McpServerConfig {
                        name: name.clone(),
                        transport: McpTransport::Stdio,
                        endpoint: Some(format!("{} {}", entry.command.unwrap_or_default(), entry.args.join(" ")).trim().to_string()),
                        timeout: entry.timeout,
                        disabled: entry.disable,
                    }
                }
            })
            .collect();

        Ok(Self { servers })
    }

    pub fn servers(&self) -> &[McpServerConfig] {
        &self.servers
    }

    pub fn validate(&self) -> Result<()> {
        for server in &self.servers {
            match server.transport {
                McpTransport::Stdio => {}
                McpTransport::Http | McpTransport::Websocket => {
                    if server.endpoint.as_deref().unwrap_or_default().is_empty() {
                        bail!(
                            "MCP server '{}' requires an endpoint for {:?} transport",
                            server.name,
                            server.transport
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub fn tool_descriptions(&self) -> Vec<String> {
        self.servers
            .iter()
            .map(|server| {
                let endpoint = server.endpoint.as_deref().unwrap_or("<local>");
                format!(
                    "{} ({:?}, endpoint={})",
                    server.name, server.transport, endpoint
                )
            })
            .collect()
    }

    pub fn invoke_tool(&self, name: &str) -> Result<String> {
        let server = self
            .servers
            .iter()
            .find(|server| server.name == name)
            .with_context(|| format!("MCP server '{}' is not configured", name))?;

        Ok(format!(
            "MCP server '{}' is available via {:?} transport{}",
            server.name,
            server.transport,
            server
                .endpoint
                .as_deref()
                .map(|endpoint| format!(" at {}", endpoint))
                .unwrap_or_default()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, McpConfig, McpServerConfig, McpTransport};

    #[test]
    fn lists_configured_servers() {
        let config = Config {
            mcp: McpConfig {
                servers: vec![McpServerConfig {
                    name: "filesystem".to_string(),
                    transport: McpTransport::Stdio,
                    endpoint: None,
                    timeout: None,
                    disabled: false,
                }],
            },
            ..Config::default()
        };

        let manager = McpManager::from_config(&config);

        assert_eq!(manager.servers().len(), 1);
        assert_eq!(manager.servers()[0].name, "filesystem");
    }

    #[test]
    fn validate_rejects_network_transport_without_endpoint() {
        let config = Config {
            mcp: McpConfig {
                servers: vec![McpServerConfig {
                    name: "github".to_string(),
                    transport: McpTransport::Http,
                    endpoint: None,
                    timeout: None,
                    disabled: false,
                }],
            },
            ..Config::default()
        };
        let manager = McpManager::from_config(&config);

        let error = manager.validate().unwrap_err().to_string();

        assert!(error.contains("github"));
        assert!(error.contains("endpoint"));
    }

    #[test]
    fn parses_mcp_setting_json_style_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_setting.json");
        std::fs::write(
            &path,
            r#"{
                "mcpServers": {
                    "brave-search": {
                        "command": "npx",
                        "args": ["-y", "@brave/brave-search-mcp-server", "--transport", "http"],
                        "timeout": 45,
                        "disable": false
                    },
                    "context7": {
                        "url": "https://mcp.context7.com/mcp",
                        "type": "streamableHttp",
                        "timeout": 30,
                        "disable": true
                    }
                }
            }"#,
        )
        .unwrap();

        let manager = McpManager::from_settings_file(&path).unwrap();

        assert_eq!(manager.servers().len(), 2);
        assert_eq!(manager.servers()[0].name, "brave-search");
        assert_eq!(manager.servers()[0].transport, McpTransport::Stdio);
        assert_eq!(manager.servers()[0].timeout, Some(45));
        assert!(!manager.servers()[0].disabled);
        assert_eq!(manager.servers()[1].name, "context7");
        assert_eq!(manager.servers()[1].transport, McpTransport::Http);
        assert_eq!(manager.servers()[1].timeout, Some(30));
        assert!(manager.servers()[1].disabled);
        assert_eq!(
            manager.servers()[1].endpoint.as_deref(),
            Some("https://mcp.context7.com/mcp")
        );
    }
}
