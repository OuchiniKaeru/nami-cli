use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SkillManifest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub argument_hint: Option<String>,
    #[serde(default)]
    pub disable_model_invocation: Option<bool>,
    #[serde(default)]
    pub user_invocable: Option<bool>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub shell: Option<String>,
}

impl SkillManifest {
    pub fn load(skill_dir: impl AsRef<Path>) -> Result<Self> {
        let path = skill_dir.as_ref().join("SKILL.md");
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let Some(contents) = contents.strip_prefix("---\n") else {
            return Ok(Self::default());
        };

        let Some((frontmatter, _)) = contents.split_once("\n---") else {
            return Ok(Self::default());
        };

        let manifest: Self = serde_yaml::from_str(frontmatter)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        Ok(manifest)
    }
}

impl Default for SkillManifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            argument_hint: None,
            disable_model_invocation: None,
            user_invocable: None,
            allowed_tools: Vec::new(),
            model: None,
            effort: None,
            context: None,
            agent: None,
            hooks: Vec::new(),
            paths: Vec::new(),
            shell: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SkillRunner {
    project_root: PathBuf,
}

impl SkillRunner {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            project_root: project_root.as_ref().to_path_buf(),
        }
    }

    pub async fn run(&self, name: &str, input: Value) -> Result<SkillResponse> {
        let skill_dir = crate::nami_root(&self.project_root).join("skills").join(name);
        let manifest = SkillManifest::load(&skill_dir)?;
        let skill_name = if manifest.name.is_empty() {
            name.to_string()
        } else {
            manifest.name
        };
        let _ = skill_name;
        let request_path = skill_dir.join(format!("request-{}.json", Uuid::new_v4()));
        let request = SkillRequest {
            task_id: Uuid::new_v4().to_string(),
            input,
        };
        fs::write(&request_path, serde_json::to_string_pretty(&request)?)?;

        let output = run_python(&skill_dir, &request_path)
            .await
            .with_context(|| format!("failed to run Python skill '{}'", name))?;

        let _ = fs::remove_file(&request_path);

        if !output.status.success() {
            bail!(
                "skill '{}' failed: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        serde_json::from_slice(&output.stdout)
            .with_context(|| format!("skill '{}' returned invalid JSON", name))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillRequest {
    pub task_id: String,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillResponse {
    pub success: bool,
    #[serde(default = "empty_object")]
    pub result: Value,
}

fn empty_object() -> Value {
    json!({})
}

async fn run_python(skill_dir: &Path, request_path: &Path) -> Result<std::process::Output> {
    let mut candidates = python_candidates();
    let mut last_error = None;

    for candidate in candidates.drain(..) {
        let mut command = Command::new(&candidate.program);
        command.args(&candidate.prefix_args);
        command
            .arg("main.py")
            .arg(request_path)
            .current_dir(skill_dir);

        match command.output().await {
            Ok(output) => return Ok(output),
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::NotFound | ErrorKind::PermissionDenied
                ) =>
            {
                last_error = Some(error);
            }
            Err(error) => return Err(error.into()),
        }
    }

    match last_error {
        Some(error) => Err(error.into()),
        None => bail!("no Python executable candidates configured"),
    }
}

#[derive(Debug, Clone)]
struct PythonCandidate {
    program: PathBuf,
    prefix_args: Vec<String>,
}

fn python_candidates() -> Vec<PythonCandidate> {
    let mut candidates = Vec::new();

    if let Ok(path) = std::env::var("AGENT_PYTHON") {
        candidates.push(PythonCandidate {
            program: PathBuf::from(path),
            prefix_args: Vec::new(),
        });
    }

    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        candidates.push(PythonCandidate {
            program: PathBuf::from(user_profile)
                .join(".cache")
                .join("codex-runtimes")
                .join("codex-primary-runtime")
                .join("dependencies")
                .join("python")
                .join("python.exe"),
            prefix_args: Vec::new(),
        });
    }

    candidates.push(PythonCandidate {
        program: PathBuf::from("python"),
        prefix_args: Vec::new(),
    });
    candidates.push(PythonCandidate {
        program: PathBuf::from("python3"),
        prefix_args: Vec::new(),
    });
    candidates.push(PythonCandidate {
        program: PathBuf::from("py"),
        prefix_args: vec!["-3".to_string()],
    });

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reads_skill_manifest() {
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("skills").join("github");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: github
description: GitHub operations
argument-hint: "[issue-number]"
user-invocable: true
---

# GitHub
"#,
        )
        .unwrap();

        let manifest = SkillManifest::load(&skill_dir).unwrap();

        assert_eq!(manifest.name, "github");
        assert_eq!(manifest.description, "GitHub operations");
        assert_eq!(manifest.argument_hint.as_deref(), Some("[issue-number]"));
        assert_eq!(manifest.user_invocable, Some(true));
    }

    #[tokio::test]
    async fn runs_python_skill_and_parses_response() {
        let dir = tempdir().unwrap();
        let nami_dir = dir.path().join(".nami");
        fs::create_dir_all(nami_dir.join("skills").join("echo")).unwrap();
        let skill_dir = nami_dir.join("skills").join("echo");
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: echo
description: Echo skill
---

# Echo
"#,
        )
        .unwrap();
        fs::write(
            skill_dir.join("main.py"),
            r#"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as f:
    request = json.load(f)

print(json.dumps({
    "success": True,
    "result": {"content": request["input"]["query"]}
}))
"#,
        )
        .unwrap();

        let runner = SkillRunner::new(dir.path());
        let response = runner
            .run("echo", json!({"query": "create README"}))
            .await
            .unwrap();

        assert!(response.success);
        assert_eq!(response.result["content"], "create README");
    }
}
