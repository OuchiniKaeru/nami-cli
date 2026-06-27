use super::*;
use crate::models::tool::Tool;
use anyhow::Context;
use serde_json::json;
use std::path::{Path, PathBuf};

/// 外部 Skill。
/// `skills/<name>/SKILL.md` の YAML フロントマターからツール情報を取得し、
/// 呼び出されたときには `SKILL.md` 全体の内容を文字列で返す。
pub struct ExternalSkill {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<serde_json::Value>,
    pub skill_md_path: PathBuf,
}

impl ExternalSkill {
    /// `skills/<name>/` ディレクトリから外部 Skill を読み込む。
    pub fn from_dir(skill_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let skill_dir = skill_dir.as_ref().to_path_buf();
        let skill_md_path = skill_dir.join("SKILL.md");
        let skill_md = std::fs::read_to_string(&skill_md_path)
            .with_context(|| format!("failed to read {}", skill_md_path.display()))?;

        let (frontmatter, _body_start) = parse_frontmatter(&skill_md);
        let meta: SkillMeta = serde_yaml::from_str(&frontmatter)
            .with_context(|| format!("failed to parse frontmatter in {}", skill_md_path.display()))?;

        Ok(Self {
            name: meta.name,
            description: meta.description,
            parameters: meta.parameters,
            skill_md_path,
        })
    }

    /// SKILL.md ファイルの内容全体を文字列として読み込む。
    fn load_skill_md(&self) -> anyhow::Result<String> {
        std::fs::read_to_string(&self.skill_md_path)
            .with_context(|| format!("failed to read {}", self.skill_md_path.display()))
    }
}

#[derive(Debug, serde::Deserialize)]
struct SkillMeta {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
}

#[async_trait]
impl Skill for ExternalSkill {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn tool_spec(&self) -> Tool {
        Tool {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: Some(self.parameters.clone().unwrap_or_else(|| {
                json!({
                    "type": "object",
                    "description": "Free-form arguments for the skill"
                })
            })),
        }
    }

    async fn execute(&self, _args: Value) -> anyhow::Result<Value> {
        let skill_md = self.load_skill_md()?;
        Ok(json!({
            "skill_md": skill_md,
        }))
    }
}

/// `SKILL.md` から YAML フロントマター部分を切り出す。
/// フロントマターがない場合は空文字列を返す。
fn parse_frontmatter(content: &str) -> (String, usize) {
    if !content.starts_with("---") {
        return (String::new(), 0);
    }

    let rest = &content[3..];
    // Windows 改行にも対応
    let rest = rest.strip_prefix('\r').unwrap_or(rest);
    let rest = rest.strip_prefix('\n').unwrap_or(rest);

    if let Some(end) = rest.find("\n---") {
        let fm = &rest[..end];
        return (fm.to_string(), 0);
    }
    (String::new(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sample_skill_frontmatter() {
        let skill = ExternalSkill::from_dir("skills/sample").expect("load sample skill");
        assert_eq!(skill.name, "sample");
        assert!(skill.description.as_deref().unwrap_or("").contains("Sample skill"));
        assert!(skill.parameters.is_none());
    }

    #[tokio::test]
    async fn execute_returns_whole_skill_md() {
        let skill = ExternalSkill::from_dir("skills/sample").expect("load sample skill");
        let result = skill.execute(json!({"query": "hello"})).await.expect("execute");
        let skill_md = result["skill_md"].as_str().expect("skill_md string");
        assert!(skill_md.contains("name: sample"));
        assert!(skill_md.contains("# Sample Skill"));
    }
}
