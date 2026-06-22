pub mod config;
pub mod event;
pub mod mcp;
pub mod runtime;
pub mod session;
pub mod skill;
pub mod workflow;

use std::path::{Path, PathBuf};

pub fn nami_root(root: impl AsRef<Path>) -> PathBuf {
    let root = root.as_ref();
    let candidate = root.join(".nami");
    if candidate.exists() {
        return candidate;
    }

    if root.join(".agent").exists() {
        return root.join(".agent");
    }

    candidate
}

pub fn config_root(root: impl AsRef<Path>) -> PathBuf {
    let root = root.as_ref();
    let config_dir = nami_root(root);
    let config_path = config_dir.join("agent.yaml");
    if config_path.exists() {
        return config_dir;
    }

    root.to_path_buf()
}

pub fn load_dotenv_from_project_root(root: impl AsRef<Path>) -> std::io::Result<()> {
    let mut dir = root.as_ref().to_path_buf();
    let mut found = false;

    loop {
        let dotenv_path = nami_root(&dir).join(".env");
        if dotenv_path.is_file() {
            found = true;
            for line in std::fs::read_to_string(&dotenv_path)?.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    if key.is_empty() || std::env::var(key).is_ok() {
                        continue;
                    }
                    std::env::set_var(key, unquote(value));
                }
            }
        }

        if !dir.pop() {
            break;
        }
    }

    if found {
        Ok(())
    } else {
        Ok(())
    }
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let chars: Vec<char> = trimmed.chars().collect();
        if (chars[0] == '\'' && chars[chars.len() - 1] == '\'')
            || (chars[0] == '"' && chars[chars.len() - 1] == '"')
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}
