use crate::event::EventRecord;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            root: crate::nami_root(project_root),
        }
    }

    pub fn create(&self, project: impl Into<String>) -> Result<Session> {
        self.ensure_root()?;
        let now = Utc::now();
        let metadata = SessionMetadata {
            session_id: Uuid::new_v4().to_string(),
            project: project.into(),
            created_at: now,
            updated_at: now,
        };
        let path = self.session_path(&metadata.session_id);
        fs::create_dir_all(path.join("artifacts"))
            .with_context(|| format!("failed to create {}", path.display()))?;
        fs::write(
            path.join("metadata.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;
        File::create(path.join("messages.jsonl"))?;
        File::create(path.join("events.jsonl"))?;
        fs::write(path.join("state.json"), "{}\n")?;

        let session = Session { metadata, path };
        session.append_event(&EventRecord::new("SESSION_CREATED"))?;
        Ok(session)
    }

    pub fn resume(&self, session_id: &str) -> Result<Session> {
        let path = self.session_path(session_id);
        let metadata_path = path.join("metadata.json");
        let mut metadata: SessionMetadata =
            serde_json::from_str(&fs::read_to_string(&metadata_path)?)?;
        metadata.updated_at = Utc::now();
        fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;
        Ok(Session { metadata, path })
    }

    pub fn list(&self) -> Result<Vec<SessionMetadata>> {
        let sessions_dir = self.root.join("sessions");
        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        for entry in fs::read_dir(sessions_dir)? {
            let entry = entry?;
            let metadata_path = entry.path().join("metadata.json");
            if metadata_path.exists() {
                sessions.push(serde_json::from_str(&fs::read_to_string(metadata_path)?)?);
            }
        }
        sessions.sort_by_key(|metadata: &SessionMetadata| metadata.created_at);
        Ok(sessions)
    }

    pub fn show(&self, session_id: &str) -> Result<SessionSummary> {
        let session = self.resume(session_id)?;
        let message_count = count_lines(session.path.join("messages.jsonl"))?;
        let event_count = count_lines(session.path.join("events.jsonl"))?;
        let state = session.read_state()?;
        Ok(SessionSummary {
            metadata: session.metadata,
            message_count,
            event_count,
            state,
        })
    }

    pub fn delete(&self, session_id: &str) -> Result<()> {
        let path = self.session_path(session_id);
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }

    fn ensure_root(&self) -> Result<()> {
        fs::create_dir_all(self.root.join("sessions"))?;
        fs::create_dir_all(self.root.join("logs"))?;
        fs::create_dir_all(self.root.join("cache"))?;
        fs::create_dir_all(self.root.join("runtime"))?;
        Ok(())
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.root.join("sessions").join(session_id)
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub metadata: SessionMetadata,
    pub path: PathBuf,
}

impl Session {
    pub fn append_message(&self, message: &MessageRecord) -> Result<()> {
        append_jsonl(self.path.join("messages.jsonl"), message)
    }

    pub fn append_event(&self, event: &EventRecord) -> Result<()> {
        append_jsonl(self.path.join("events.jsonl"), event)
    }

    pub fn load_messages(&self) -> Result<Vec<MessageRecord>> {
        let path = self.path.join("messages.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut messages = Vec::new();
        
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let message: MessageRecord = serde_json::from_str(&line)?;
            messages.push(message);
        }
        
        Ok(messages)
    }

    pub fn write_state(&self, state: &Value) -> Result<()> {
        fs::write(
            self.path.join("state.json"),
            serde_json::to_string_pretty(state)?,
        )
        .with_context(|| "failed to write state.json")?;
        Ok(())
    }

    pub fn read_state(&self) -> Result<Value> {
        Ok(serde_json::from_str(&fs::read_to_string(
            self.path.join("state.json"),
        )?)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    pub project: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRecord {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl MessageRecord {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            agent: None,
            name: None,
        }
    }

    pub fn assistant(agent: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            agent: Some(agent.into()),
            name: None,
        }
    }

    pub fn tool(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            agent: None,
            name: Some(name.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub metadata: SessionMetadata,
    pub message_count: usize,
    pub event_count: usize,
    pub state: Value,
}

fn append_jsonl(path: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", serde_json::to_string(value)?)?;
    Ok(())
}

fn count_lines(path: impl AsRef<Path>) -> Result<usize> {
    let file = File::open(path)?;
    Ok(BufReader::new(file).lines().count())
}

pub fn completed_state(agents: &[&str]) -> Value {
    let mut state = serde_json::Map::new();
    for agent in agents {
        state.insert(agent.to_string(), json!({ "status": "completed" }));
    }
    Value::Object(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventRecord;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn create_session_writes_expected_layout() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path());

        let session = store.create("ai-kanban").unwrap();

        assert!(session.path.join("metadata.json").exists());
        assert!(session.path.join("messages.jsonl").exists());
        assert!(session.path.join("events.jsonl").exists());
        assert!(session.path.join("state.json").exists());
        assert!(session.path.join("artifacts").is_dir());
        assert_eq!(session.metadata.project, "ai-kanban");
        assert!(!session.metadata.session_id.is_empty());
    }

    #[test]
    fn append_message_event_and_state_are_persisted() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path());
        let session = store.create("ai-kanban").unwrap();

        session
            .append_message(&MessageRecord::user("READMEを書いて"))
            .unwrap();
        session
            .append_event(&EventRecord::new("TASK_CREATED").with_agent("planner"))
            .unwrap();
        session
            .write_state(&json!({"planner": {"status": "running"}}))
            .unwrap();

        let summary = store.show(&session.metadata.session_id).unwrap();
        assert_eq!(summary.message_count, 1);
        assert_eq!(summary.event_count, 2);
        assert_eq!(summary.state["planner"]["status"], "running");
    }

    #[test]
    fn list_and_delete_sessions() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path());
        let session = store.create("ai-kanban").unwrap();

        let sessions = store.list().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, session.metadata.session_id);

        store.delete(&session.metadata.session_id).unwrap();
        assert!(store.list().unwrap().is_empty());
    }
}
