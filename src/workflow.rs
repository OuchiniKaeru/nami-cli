use crate::config::Config;
use crate::event::EventRecord;
use crate::runtime::{AgentInput, AgentOutput, AgentRuntime};
use crate::session::{completed_state, MessageRecord, Session, SessionStore};
use anyhow::Result;
use serde_json::{json, Value};

pub struct WorkflowRunner<'a> {
    store: &'a SessionStore,
    config: &'a Config,
    runtime: &'a dyn AgentRuntime,
}

impl<'a> WorkflowRunner<'a> {
    pub fn new(store: &'a SessionStore, config: &'a Config, runtime: &'a dyn AgentRuntime) -> Self {
        Self {
            store,
            config,
            runtime,
        }
    }

    pub async fn run(&self, task: &str) -> Result<Session> {
        let session = self.store.create(&self.config.project.name)?;
        session.append_message(&MessageRecord::user(task))?;
        session.append_event(&EventRecord::new("TASK_CREATED"))?;

        let mut context = Vec::<AgentOutput>::new();
        let mut state = json!({});
        for agent in ["planner", "coder", "reviewer"] {
            session.append_event(&EventRecord::new("TASK_ASSIGNED").with_agent(agent))?;
            let output = self
                .runtime
                .run(AgentInput {
                    agent: agent.to_string(),
                    task: task.to_string(),
                    context: context.clone(),
                    system_prompt: self.config.system_prompt.clone(),
                    rules: self.config.rules.clone(),
                    skills: self.config.skills.clone(),
                    mcp: self
                        .config
                        .mcp
                        .servers
                        .iter()
                        .map(|server| server.name.clone())
                        .collect(),
                })
                .await?;
            session.append_message(&MessageRecord::assistant(agent, &output.content))?;
            session.append_event(&EventRecord::new("TASK_COMPLETED").with_agent(agent))?;
            context.push(output);
            mark_completed(&mut state, agent);
            session.write_state(&state)?;
        }

        session.write_state(&completed_state(&["planner", "coder", "reviewer"]))?;
        Ok(session)
    }
}

fn mark_completed(state: &mut Value, agent: &str) {
    if !state.is_object() {
        *state = json!({});
    }
    state[agent] = json!({ "status": "completed" });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::runtime::DeterministicRuntime;
    use crate::session::SessionStore;
    use tempfile::tempdir;

    #[tokio::test]
    async fn workflow_persists_messages_events_and_completed_state() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path());
        let config = Config::default();
        let runtime = DeterministicRuntime;
        let runner = WorkflowRunner::new(&store, &config, &runtime);

        let session = runner.run("READMEを書いて").await.unwrap();
        let summary = store.show(&session.metadata.session_id).unwrap();

        assert_eq!(summary.message_count, 4);
        assert_eq!(summary.event_count, 8);
        assert_eq!(summary.state["planner"]["status"], "completed");
        assert_eq!(summary.state["coder"]["status"], "completed");
        assert_eq!(summary.state["reviewer"]["status"], "completed");
    }
}
