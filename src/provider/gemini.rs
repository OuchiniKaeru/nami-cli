use super::*;
use crate::models::chat::{ChatResponse, Usage};
use crate::models::config::ProviderConfig;
use crate::models::message::Role;
use crate::models::tool::ToolCall;
use bytes::{Buf, BytesMut};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::io::Write;

pub struct GeminiProvider {
    pub client: reqwest::Client,
    pub cfg: ProviderConfig,
    pub previous_interaction_id: std::sync::Mutex<Option<String>>,
}

impl GeminiProvider {
    pub fn new(cfg: ProviderConfig) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("nami/0.1")
            .build()?;
        Ok(Self {
            client,
            cfg,
            previous_interaction_id: std::sync::Mutex::new(None),
        })
    }

    fn base_url(&self) -> &str {
        self.cfg.base_url.as_deref().unwrap_or("https://generativelanguage.googleapis.com/v1beta")
    }

    pub async fn chat(&self, request: crate::models::chat::ChatRequest) -> Result<ChatResponse, ProviderError> {
        if request.stream.unwrap_or(false) {
            self.interactions_create_stream(request).await
        } else {
            self.interactions_create(request, false).await
        }
    }

    fn build_result_content(&self, content: &str) -> serde_json::Value {
        let text = if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| content.to_string())
        } else {
            content.to_string()
        };
        serde_json::json!([{ "type": "text", "text": text }])
    }

    fn build_interaction_body(&self, request: &crate::models::chat::ChatRequest, stream: bool) -> serde_json::Value {
        let mut system_instruction = String::new();

        for msg in &request.messages {
            if let Role::System = msg.role {
                if let Some(content) = &msg.content {
                    if !system_instruction.is_empty() {
                        system_instruction.push('\n');
                        system_instruction.push('\n');
                    }
                    system_instruction.push_str(content);
                }
            }
        }

        let has_previous = self.previous_interaction_id.lock().unwrap().is_some();

        let input_field = if has_previous {
            if let Some(tool_msg) = request.messages.iter().rev().find(|m| m.role == Role::Tool) {
                let content = tool_msg.content.as_deref().unwrap_or("");
                serde_json::json!({
                    "type": "function_result",
                    "call_id": tool_msg.tool_call_id.as_deref().unwrap_or(""),
                    "name": tool_msg.name.as_deref().unwrap_or(""),
                    "result": self.build_result_content(content)
                })
            } else if let Some(user_msg) = request.messages.iter().rev().find(|m| m.role == Role::User) {
                let content = user_msg.content.as_deref().unwrap_or("");
                serde_json::json!({
                    "type": "user_input",
                    "content": [{ "type": "text", "text": content }]
                })
            } else {
                serde_json::json!([])
            }
        } else {
            let mut input_steps = Vec::new();
            for msg in &request.messages {
                match msg.role {
                    Role::System => {}
                    Role::User => {
                        if let Some(content) = &msg.content {
                            input_steps.push(serde_json::json!({
                                "type": "user_input",
                                "content": [{ "type": "text", "text": content }]
                            }));
                        }
                    }
                    Role::Assistant => {
                        let mut content_blocks = Vec::new();
                        if let Some(text) = &msg.content {
                            content_blocks.push(serde_json::json!({ "type": "text", "text": text }));
                        }
                        if !content_blocks.is_empty() {
                            input_steps.push(serde_json::json!({
                                "type": "model_output",
                                "content": content_blocks
                            }));
                        }
                    }
                    Role::Tool => {
                        if let Some(content) = &msg.content {
                            input_steps.push(serde_json::json!({
                                "type": "function_result",
                                "call_id": msg.tool_call_id.as_deref().unwrap_or(""),
                                "name": msg.name.as_deref().unwrap_or(""),
                                "result": self.build_result_content(content)
                            }));
                        }
                    }
                }
            }
            serde_json::json!(input_steps)
        };

        let mut body = serde_json::json!({
            "model": request.model,
            "input": input_field,
            "generation_config": {
                "temperature": request.temperature.unwrap_or(0.2),
                "max_output_tokens": request.max_tokens.unwrap_or(1024),
            },
            "store": true,
            "stream": stream
        });

        if !system_instruction.is_empty() {
            body["system_instruction"] = serde_json::json!(system_instruction);
        }

        if let Some(tools) = &request.tools {
            if !tools.is_empty() {
                let declarations: Vec<serde_json::Value> = tools.iter().map(|t| {
                    let mut tool = serde_json::json!({
                        "type": "function",
                        "name": t.name,
                    });
                    if let Some(desc) = &t.description {
                        tool["description"] = serde_json::json!(desc);
                    }
                    if let Some(schema) = &t.input_schema {
                        let mut params = schema.clone();
                        if let Some(obj) = params.as_object_mut() {
                            obj.remove("$schema");
                        }
                        tool["parameters"] = params;
                    }
                    tool
                }).collect();
                body["tools"] = serde_json::json!(declarations);
            }
        }

        if let Some(prev) = self.previous_interaction_id.lock().unwrap().as_ref() {
            body["previous_interaction_id"] = serde_json::json!(prev);
        }

        body
    }

    async fn interactions_create(&self, request: crate::models::chat::ChatRequest, stream: bool) -> Result<ChatResponse, ProviderError> {
        let api_key = match &self.cfg.api_key {
            Some(k) => k,
            None => return Err(ProviderError::Api("gemini api_key is required".into())),
        };

        let body = self.build_interaction_body(&request, stream);
        let url = format!("{}/interactions", self.base_url().trim_end_matches('/'));

        let resp = self
            .client
            .post(url)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .header("Api-Revision", "2026-05-20")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ProviderError::Api(format!("status={} body={}", status, text)));
        }

        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| ProviderError::Parse(format!("interactions response parse error: {}", e)))?;

        let id = v.get("id").and_then(|x| x.as_str()).map(|s| s.to_string());
        if let Some(id) = &id {
            *self.previous_interaction_id.lock().unwrap() = Some(id.clone());
        }

        self.parse_interaction_response(&v, request.model)
    }

    async fn interactions_create_stream(&self, request: crate::models::chat::ChatRequest) -> Result<ChatResponse, ProviderError> {
        let api_key = match &self.cfg.api_key {
            Some(k) => k,
            None => return Err(ProviderError::Api("gemini api_key is required".into())),
        };

        let body = self.build_interaction_body(&request, true);
        let url = format!("{}/interactions", self.base_url().trim_end_matches('/'));

        let resp = self
            .client
            .post(url)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .header("Api-Revision", "2026-05-20")
            .json(&body)
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!("status={} body={}", status, text)));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = BytesMut::new();

        let mut text = String::new();
        let mut reasoning = String::new();
        let mut usage = Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            prompt_tokens_details: None,
        };

        let mut current_function_call: Option<PartialFunctionCall> = None;
        let mut partial_calls: HashMap<String, PartialFunctionCall> = HashMap::new();
        let mut printed_text = false;
        let mut reasoning_started = false;

        loop {
            let mut done = false;
            while let Some(data) = next_sse_data(&mut buffer) {
                if data == "[DONE]" {
                    done = true;
                    break;
                }
                let v: serde_json::Value = match serde_json::from_str(&data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                match v.get("event_type").and_then(|x| x.as_str()) {
                    Some("interaction.created") | Some("interaction.in_progress") => {}
                    Some("interaction.requires_action") => {
                        if let Some(ref call) = current_function_call {
                            if !call.name.is_empty() {
                                partial_calls.insert(call.id.clone(), call.clone());
                            }
                            current_function_call = None;
                        }
                        done = true;
                    }
                    Some("interaction.completed") => {
                        if let Some(u) = v.get("interaction").and_then(|i| i.get("usage")).and_then(|u| parse_usage(u)) {
                            usage = u;
                        }
                        done = true;
                    }
                    Some("step.start") => {
                        if let Some(step) = v.get("step") {
                            if let Some("function_call") = step.get("type").and_then(|x| x.as_str()) {
                                let id = step.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                                let name = step.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                                current_function_call = Some(PartialFunctionCall {
                                    id: id.clone(),
                                    name: name.clone(),
                                    arguments: String::new(),
                                });
                            }
                        }
                    }
                    Some("step.delta") => {
                        if let Some(delta) = v.get("delta") {
                            match delta.get("type").and_then(|x| x.as_str()) {
                                Some("text") => {
                                    if let Some(t) = delta.get("text").and_then(|x| x.as_str()) {
                                        text.push_str(t);
                                        print!("{}", t);
                                        let _ = std::io::stdout().flush();
                                        printed_text = true;
                                    }
                                }
                                Some("thought_summary") => {
                                    if let Some(t) = delta.get("content").and_then(|c| c.get("text")).and_then(|x| x.as_str()) {
                                        reasoning.push_str(t);
                                        if !reasoning_started {
                                            print!("\n<thinking>\n");
                                            reasoning_started = true;
                                        }
                                        print!("{}", t);
                                        let _ = std::io::stdout().flush();
                                    }
                                }
                                Some("thought_signature") => {}
                                Some("arguments") | Some("arguments_delta") => {
                                    if let Some(args) = delta.get("partial_arguments").and_then(|x| x.as_str())
                                        .or_else(|| delta.get("arguments").and_then(|x| x.as_str()))
                                    {
                                        if let Some(ref mut call) = current_function_call {
                                            call.arguments.push_str(args);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some("step.stop") => {
                        if let Some(ref call) = current_function_call {
                            if !call.name.is_empty() {
                                let args_pretty = if let Ok(value) = serde_json::from_str::<serde_json::Value>(&call.arguments) {
                                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| call.arguments.clone())
                                } else {
                                    call.arguments.clone()
                                };
                                print!("\n[tool_call: {}] {}\n", call.name, args_pretty);
                                let _ = std::io::stdout().flush();
                                partial_calls.insert(call.id.clone(), call.clone());
                            }
                            current_function_call = None;
                        }
                    }
                    _ => {}
                }
            }

            if done {
                break;
            }

            match stream.next().await {
                Some(Ok(chunk)) => buffer.extend_from_slice(&chunk),
                Some(Err(e)) => return Err(e.into()),
                None => break,
            }
        }

        if reasoning_started {
            println!("\n</thinking>");
        } else if printed_text {
            println!();
        }

        let tool_calls: Vec<ToolCall> = partial_calls
            .into_values()
            .map(|p| ToolCall {
                id: p.id,
                name: p.name,
                arguments: serde_json::from_str(&p.arguments).unwrap_or(serde_json::Value::Null),
            })
            .collect();

        Ok(ChatResponse {
            id: uuid::Uuid::new_v4().to_string(),
            object: "interaction".into(),
            created: chrono::Utc::now().timestamp() as u64,
            model: request.model,
            choices: vec![crate::models::chat::ChatChoice {
                index: 0,
                message: crate::models::Message {
                    role: Role::Assistant,
                    content: if text.is_empty() { None } else { Some(text) },
                    tool_call_id: None,
                    name: None,
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                    reasoning_content: if reasoning.is_empty() { None } else { Some(reasoning) },
                },
                finish_reason: Some("stop".into()),
            }],
            usage,
            extra: HashMap::new(),
        })
    }

    fn parse_interaction_response(&self, v: &serde_json::Value, model: String) -> Result<ChatResponse, ProviderError> {
        let steps = v.get("steps").and_then(|x| x.as_array()).cloned().unwrap_or_default();

        let mut text = String::new();
        let mut reasoning = String::new();
        let mut tool_calls = Vec::new();

        for step in &steps {
            let step_type = step.get("type").and_then(|x| x.as_str());
            match step_type {
                Some("model_output") => {
                    if let Some(content) = step.get("content").and_then(|x| x.as_array()) {
                        for block in content {
                            if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                                text.push_str(t);
                            }
                        }
                    }
                }
                Some("thought") => {
                    if let Some(summary) = step.get("summary").and_then(|x| x.as_array()) {
                        for block in summary {
                            if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                                reasoning.push_str(t);
                            }
                        }
                    }
                }
                Some("function_call") => {
                    let id = step.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let name = step.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let arguments = step.get("arguments").cloned().unwrap_or(serde_json::Value::Null);
                    tool_calls.push(ToolCall { id, name, arguments });
                }
                _ => {}
            }
        }

        let usage = v.get("usage").and_then(parse_usage).unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            prompt_tokens_details: None,
        });

        Ok(ChatResponse {
            id: v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            object: "interaction".into(),
            created: chrono::Utc::now().timestamp() as u64,
            model,
            choices: vec![crate::models::chat::ChatChoice {
                index: 0,
                message: crate::models::Message {
                    role: Role::Assistant,
                    content: if text.is_empty() { None } else { Some(text) },
                    tool_call_id: None,
                    name: None,
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                    reasoning_content: if reasoning.is_empty() { None } else { Some(reasoning) },
                },
                finish_reason: Some("stop".into()),
            }],
            usage,
            extra: HashMap::new(),
        })
    }
}

#[async_trait::async_trait]
impl LlmProvider for GeminiProvider {
    async fn chat(&self, request: crate::models::chat::ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.chat(request).await
    }

    fn supports_streaming(&self) -> bool {
        true
    }
}

fn parse_usage(u: &serde_json::Value) -> Option<Usage> {
    Some(Usage {
        prompt_tokens: u.get("total_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
        completion_tokens: u.get("total_output_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
        total_tokens: u.get("total_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
        prompt_tokens_details: None,
    })
}

#[derive(Default, Clone)]
struct PartialFunctionCall {
    id: String,
    name: String,
    arguments: String,
}

fn next_sse_line(buffer: &mut BytesMut) -> Option<String> {
    let text = String::from_utf8_lossy(buffer);
    let newline_pos = text.find('\n')?;
    let line = text[..newline_pos].trim_end().to_string();
    let consumed = newline_pos + 1;
    buffer.advance(consumed);
    Some(line)
}

fn next_sse_data(buffer: &mut BytesMut) -> Option<String> {
    loop {
        let line = next_sse_line(buffer)?;
        if line.is_empty() {
            continue;
        }
        if line.starts_with("data: ") {
            return Some(line[6..].to_string());
        }
    }
}
