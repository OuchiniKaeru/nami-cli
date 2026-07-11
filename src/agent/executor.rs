use crate::{
    agent::{
        planner::Planner,
        tool_router::ToolRouter,
    },
    memory::JsonlMemoryStore,
    mcp::McpClient,
    models::{
        config::AppConfig,
        message::Message,
        metrics::Metrics,
    },
    provider::build,
    session::SessionRecord,
    skill::SkillRegistry,
};
use std::io::Write;

pub struct Agent {
    pub planner: Planner,
    pub tool_router: ToolRouter,
    pub provider: Box<dyn crate::provider::LlmProvider>,
    pub memory: Option<JsonlMemoryStore>,
    pub session: Option<SessionRecord>,
    pub metrics: Metrics,
    pub config: AppConfig,
    pub initial_messages: Vec<Message>,
}

impl Agent {
    pub async fn new(
        cfg: AppConfig,
        skills: SkillRegistry,
        mcp: McpClient,
    ) -> anyhow::Result<Self> {
        let provider_cfg = crate::config::resolve_provider(cfg.provider.clone())?;
        let provider = build(provider_cfg)?;
        let planner = Planner::new();
        let tool_router = ToolRouter::new(skills, mcp);
        let memory = if !cfg.memory.directory.is_empty() || !cfg.memory.file.is_empty() {
            Some(JsonlMemoryStore::new(cfg.memory.file.clone()))
        } else {
            None
        };

        let mut agent = Self {
            planner,
            tool_router,
            provider,
            memory,
            session: None,
            metrics: Metrics::default(),
            config: cfg.clone(),
            initial_messages: Vec::new(),
        };

        if cfg.session.save {
            agent.session = Some(SessionRecord::new(cfg, Vec::new()));
        }

        Ok(agent)
    }

    /// 過去のセッションを読み込み、会話を継続する。
    pub fn load_session(&mut self, session: SessionRecord) {
        self.initial_messages = session.messages.clone();
        self.metrics = session.metrics.clone();
        self.config = session.config.clone();
        self.session = Some(session);
    }

    fn build_system_prompt(&self) -> String {
        let cfg = &self.config;
        let mut parts = Vec::new();

        if !cfg.system_prompt.is_empty() {
            parts.push(self.planner.system_prompt(&cfg.system_prompt));
        }

        for rule_path in &cfg.rules {
            if let Ok(content) = std::fs::read_to_string(rule_path) {
                if !content.is_empty() {
                    parts.push(format!("--- Rule: {} ---\n{}", rule_path, content));
                }
            }
        }

        parts.join("\n\n")
    }

    pub async fn run(&mut self, prompt: impl Into<String>) -> anyhow::Result<Option<String>> {
        let start = std::time::Instant::now();
        let cfg = &self.config;

        let mut messages: Vec<Message> = self.initial_messages.clone();

        // 新規セッションの場合のみシステムプロンプトを先頭に追加
        if messages.is_empty() {
            let system = self.build_system_prompt();
            if !system.is_empty() {
                messages.push(Message::system(system));
            }
        }

        messages.push(Message::user(prompt.into()));

        let tools = self.tool_router.list_tools().await;
        let mut last_answer: Option<String> = None;

        let actually_streaming = cfg.stream && self.provider.supports_streaming();

        for _i in 0..cfg.max_iterations {
            self.metrics.record_iteration();
            let req = self.planner.build_request(
                &cfg.provider.model,
                messages.clone(),
                tools.clone(),
                cfg.temperature,
                cfg.max_tokens,
                cfg.max_iterations,
                cfg.stream,
            );

            let resp = self.provider.chat(req).await?;
            tracing::debug!(
                iteration = self.metrics.iterations,
                input_tokens = resp.usage.prompt_tokens,
                output_tokens = resp.usage.completion_tokens,
                "LLM response received"
            );
            self.metrics.record_llm_call(crate::models::metrics::TokenUsage {
                input_tokens: resp.usage.prompt_tokens,
                output_tokens: resp.usage.completion_tokens,
                total_tokens: resp.usage.total_tokens,
                cached_tokens: resp.usage.prompt_tokens_details.as_ref().and_then(|d| d.cached_tokens),
                reasoning_tokens: resp
                    .usage
                    .prompt_tokens_details
                    .as_ref()
                    .and_then(|d| d.reasoning_tokens),
            });

            let choice = resp.choices.into_iter().next();
            let assistant_msg = choice.map(|c| c.message).unwrap_or(Message::assistant(None, None));
            messages.push(assistant_msg.clone());

            // ストリーミング表示が行われない場合は reasoning / thinking 内容を表示する
            if !actually_streaming {
                if let Some(ref rc) = assistant_msg.reasoning_content {
                    if !rc.is_empty() {
                        print!("\n<thinking>\n{}\n</thinking>\n", rc);
                        let _ = std::io::stdout().flush();
                    }
                }
            }

            if let Some(ref content) = assistant_msg.content {
                if content.trim().len() > 0 {
                    last_answer = Some(content.clone());
                }
            }

            let tool_calls = match assistant_msg.tool_calls {
                Some(tc) if !tc.is_empty() => tc,
                _ => break,
            };

            // Tool 呼び出しを並列実行する
            let router = &self.tool_router;
            let futures: Vec<_> = tool_calls
                .iter()
                .map(|call| async move { router.execute(call).await })
                .collect();

            // ストリーミング表示が行われない場合は実行する tool_call を表示する
            if !actually_streaming {
                print!("\n");
                for call in &tool_calls {
                    let args_pretty = serde_json::to_string_pretty(&call.arguments).unwrap_or_else(|_| call.arguments.to_string());
                    print!("[tool_call: {}] {}\n", call.name, args_pretty);
                }
                let _ = std::io::stdout().flush();
            }

            let results = futures_util::future::join_all(futures).await;

            let mut has_error = false;
            for (idx, (call, result)) in tool_calls.into_iter().zip(results.into_iter()).enumerate() {
                tracing::info!(
                    tool_call_id = call.id,
                    name = call.name,
                    mcp = !self.tool_router.skills.get(&call.name).is_some(),
                    "executing tool"
                );
                match result {
                    Ok(tool_result) => {
                        if tool_result.is_mcp {
                            self.metrics.record_mcp_call();
                        } else {
                            self.metrics.record_tool_call();
                        }
                        if !actually_streaming {
                            let content_pretty = if let Ok(value) = serde_json::from_str::<serde_json::Value>(&tool_result.content) {
                                serde_json::to_string_pretty(&value).unwrap_or_else(|_| tool_result.content.clone())
                            } else {
                                tool_result.content.clone()
                            };
                            println!("[tool_result: {}] {}", tool_result.name, content_pretty);
                        }
                        messages.push(Message::tool_result(
                            tool_result.tool_call_id.clone(),
                            tool_result.name.clone(),
                            tool_result.content.clone(),
                        ));
                    }
                    Err(e) => {
                        has_error = true;
                        messages.push(Message::tool_result(
                            call.id.clone(),
                            call.name.clone(),
                            format!("error: {}", e),
                        ));
                    }
                }
                if let Some(mem) = &mut self.memory {
                    let _ = mem
                        .append(&Message::assistant(
                            Some(format!("tool_call[{}]: {}", idx, call.name)),
                            None,
                        ))
                        .await;
                }
            }

            if has_error {
                // continue loop so LLM can adapt
            }
        }

        if let Some(session) = &mut self.session {
            session.messages = messages.clone();
            session.metrics = self.metrics.clone();
            session.updated_at = chrono::Utc::now().to_rfc3339();
            session.derive_title();
            let _ = session.save();
        }

        self.metrics.elapsed_ms =
            Some(std::time::Instant::now().duration_since(start).as_millis() as u64);

        tracing::info!(
            iterations = self.metrics.iterations,
            llm_calls = self.metrics.llm_calls,
            tool_calls = self.metrics.tool_calls,
            mcp_calls = self.metrics.mcp_calls,
            elapsed_ms = self.metrics.elapsed_ms,
            "agent run finished"
        );

        Ok(last_answer)
    }
}
