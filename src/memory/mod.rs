use crate::models::message::Message;
use anyhow::Context;
use std::io::Write;

pub struct JsonlMemoryStore {
    pub path: std::path::PathBuf,
    pub buffer: Vec<Message>,
}

impl JsonlMemoryStore {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: path.into(),
            buffer: Vec::new(),
        }
    }

    pub async fn append(&mut self, message: &Message) -> anyhow::Result<()> {
        self.buffer.push(message.clone());
        self.flush().await
    }

    pub async fn flush(&mut self) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create directory: {}", parent.display()))?;
            }
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("failed to open memory file: {}", self.path.display()))?;

        for m in &self.buffer {
            let line = serde_json::to_string(m)?;
            writeln!(f, "{}", line)?;
        }

        self.buffer.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_memory() {
        let store = JsonlMemoryStore::new("memory/memory.jsonl");
        assert!(store.buffer.is_empty());
    }
}
