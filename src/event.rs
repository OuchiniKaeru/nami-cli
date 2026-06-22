use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    #[serde(rename = "type")]
    pub kind: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

impl EventRecord {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            timestamp: Utc::now(),
            agent: None,
        }
    }

    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_record_sets_timestamp_and_optional_agent() {
        let event = EventRecord::new("TASK_ASSIGNED").with_agent("coder");

        assert_eq!(event.kind, "TASK_ASSIGNED");
        assert_eq!(event.agent.as_deref(), Some("coder"));
        assert!(event.timestamp.to_rfc3339().contains('T'));
    }
}
