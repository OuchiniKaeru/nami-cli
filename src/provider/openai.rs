use bytes::{Buf, BytesMut};
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::models::chat::{ChatRequest, ChatResponse};
use crate::models::config::ProviderConfig;

pub struct OpenAICompatibleProvider {
    pub client: reqwest::Client,
    pub cfg: ProviderConfig,
}

impl OpenAICompatibleProvider {
    pub fn new(cfg: ProviderConfig) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("nami/0.1")
            .build()?;
        Ok(Self { client, cfg })
    }

    fn base_url(&self) -> String {
        if let Some(url) = &self.cfg.base_url {
            return url.clone();
        }
        match self.cfg.kind {
            crate::models::config::ProviderKind::OpenRouter => "https://openrouter.ai/api/v1",
            crate::models::config::ProviderKind::Groq => "https://api.groq.com/openai/v1",
            crate::models::config::ProviderKind::Anthropic => "https://api.anthropic.com/v1",
            _ => "https://api.openai.com/v1",
        }
        .to_string()
    }

    fn ensure_api_key(&self) -> Result<String, ProviderError> {
        let key = self.cfg.api_key.as_deref().unwrap_or("").trim();
        if key.is_empty() {
            return Err(ProviderError::Api(format!(
                "api_key is required for provider: {:?}",
                self.cfg.kind
            )));
        }
        Ok(format!("Bearer {}", key))
    }

    fn chat_url(&self) -> String {
        let base_owned = self.base_url();
        let base = base_owned.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{}/chat/completions", base)
        } else {
            format!("{}/v1/chat/completions", base)
        }
    }

    fn chat_body(&self, request: &ChatRequest, stream: bool) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(openai_message).collect::<Vec<_>>(),
            "temperature": request.temperature.unwrap_or(0.2),
            "max_tokens": request.max_tokens.unwrap_or(1024),
            "stream": stream,
            "tools": request.tools.as_ref().map(|tools|
                tools.iter().map(openai_tool).collect::<Vec<_>>()
            ),
        });
        if let Some(opts) = &request.stream_options {
            body["stream_options"] = serde_json::json!({
                "include_usage": opts.include_usage,
            });
        }
        body
    }

    /// OpenAI互換 chat/completions エンドポイントを叩く。
    /// Anthropic等の一部はAPI互換レイヤーでOpenAI互換を提供する前提。
    pub async fn chat(
        &self,
        request: ChatRequest,
    ) -> Result<ChatResponse, ProviderError> {
        if request.stream.unwrap_or(false) {
            return self.stream_chat(request).await;
        }

        let url = self.chat_url();
        let body = self.chat_body(&request, false);
        let auth = self.ensure_api_key()?;

        let resp = self
            .client
            .post(url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!(
                "status={} body={}",
                status, text
            )));
        }

        let raw = resp.json::<serde_json::Value>().await?;
        let mut parsed = serde_json::from_value::<ChatResponse>(raw.clone())
            .map_err(|e| ProviderError::Parse(format!("openai response parse error: {}", e)))?;

        // OpenAI 互換 API は tool_calls 内で function.name / function.arguments を使うことがあるので正規化する。
        if let Some(choices_raw) = raw.get("choices").and_then(|c| c.as_array()) {
            for (i, choice_raw) in choices_raw.iter().enumerate() {
                if let Some(choice) = parsed.choices.get_mut(i) {
                    if let Some(tool_calls_raw) = choice_raw
                        .get("message")
                        .and_then(|m| m.get("tool_calls"))
                        .and_then(|t| t.as_array())
                    {
                        if let Some(ref mut message) = choice.message.tool_calls {
                            for (j, tc) in message.iter_mut().enumerate() {
                                if let Some(tc_raw) = tool_calls_raw.get(j) {
                                    if tc.name.is_empty() {
                                        if let Some(name) = tc_raw
                                            .get("function")
                                            .and_then(|f| f.get("name"))
                                            .and_then(|n| n.as_str())
                                        {
                                            tc.name = name.to_string();
                                        }
                                    }
                                    if tc.arguments.is_null() {
                                        if let Some(args) = tc_raw
                                            .get("function")
                                            .and_then(|f| f.get("arguments"))
                                            .and_then(|a| a.as_str())
                                        {
                                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
                                                tc.arguments = v;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(parsed)
    }

    /// SSE ストリームを読み、最終的な ChatResponse に集約する。
    async fn stream_chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = self.chat_url();
        let body = self.chat_body(&request, true);
        let auth = self.ensure_api_key()?;

        let resp = self
            .client
            .post(url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!(
                "status={} body={}",
                status, text
            )));
        }

        let mut stream = resp.bytes_stream();
        let mut content = String::new();
        let mut partial_calls: BTreeMap<usize, PartialToolCall> = BTreeMap::new();
        let mut buffer = BytesMut::new();
        let mut usage: Option<crate::models::chat::Usage> = None;
        let mut reasoning = String::new();
        let mut reasoning_started = false;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.extend_from_slice(&chunk);

            while let Some(event) = next_sse_event(&mut buffer) {
                let event_text = String::from_utf8_lossy(&event)
                    .replace("\r\n", "\n")
                    .replace('\r', "\n");

                for line in event_text.lines() {
                    let line = line.trim();
                    if line == "[DONE]" || line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };
                    let data = data.trim();
                    if data == "[DONE]" {
                        continue;
                    }

                    let v: serde_json::Value = serde_json::from_str(data)
                        .map_err(|e| ProviderError::Parse(format!("stream chunk parse error: {}", e)))?;

                    if let Some(u) = v.get("usage") {
                        if let Ok(parsed) =
                            serde_json::from_value::<crate::models::chat::Usage>(u.clone())
                        {
                            usage = Some(parsed);
                        }
                    }

                    if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
                        for choice in choices {
                            let delta = choice.get("delta");
                            if let Some(c) = delta
                                .and_then(|d| d.get("content"))
                                .and_then(|c| c.as_str())
                            {
                                content.push_str(c);
                                print!("{}", c);
                                let _ = std::io::stdout().flush();
                            }

                            if let Some(r) = delta
                                .and_then(|d| d.get("reasoning_content"))
                                .and_then(|r| r.as_str())
                                .or_else(|| {
                                    delta.and_then(|d| d.get("thinking")).and_then(|r| r.as_str())
                                })
                                .or_else(|| {
                                    delta.and_then(|d| d.get("reasoning")).and_then(|r| r.as_str())
                                })
                            {
                                if !reasoning_started {
                                    print!("\n<thinking>\n");
                                    reasoning_started = true;
                                }
                                reasoning.push_str(r);
                                print!("{}", r);
                                let _ = std::io::stdout().flush();
                            }

                            if let Some(tcs) = delta
                                .and_then(|d| d.get("tool_calls"))
                                .and_then(|t| t.as_array())
                            {
                                for tc in tcs {
                                    let idx = tc
                                        .get("index")
                                        .and_then(|i| i.as_u64())
                                        .unwrap_or(0) as usize;
                                    let entry = partial_calls.entry(idx).or_default();
                                    if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                        entry.id = id.to_string();
                                    }
                                    if let Some(name) = tc
                                        .get("function")
                                        .and_then(|f| f.get("name"))
                                        .and_then(|n| n.as_str())
                                        .filter(|n| !n.is_empty())
                                    {
                                        if entry.name.is_empty() {
                                            entry.name = name.to_string();
                                            print!("\n[tool_call: {}] ", name);
                                            let _ = std::io::stdout().flush();
                                        }
                                    }
                                    if let Some(args) = tc
                                        .get("function")
                                        .and_then(|f| f.get("arguments"))
                                        .and_then(|a| a.as_str())
                                    {
                                        entry.arguments.push_str(args);
                                        print!("{}", args);
                                        let _ = std::io::stdout().flush();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if reasoning_started {
            println!("\n</thinking>");
        } else if !content.is_empty() || !partial_calls.is_empty() {
            println!();
        }

        let tool_calls: Vec<crate::models::tool::ToolCall> = partial_calls
            .into_values()
            .map(|p| crate::models::tool::ToolCall {
                id: p.id,
                name: p.name,
                arguments: serde_json::from_str(&p.arguments).unwrap_or(serde_json::Value::Null),
            })
            .filter(|tc| !tc.id.is_empty() || !tc.name.is_empty())
            .collect();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(ChatResponse {
            id: uuid::Uuid::new_v4().to_string(),
            object: "chat.completion".into(),
            created: now,
            model: request.model,
            choices: vec![crate::models::chat::ChatChoice {
                index: 0,
                message: crate::models::Message::assistant(
                    if content.is_empty() { None } else { Some(content) },
                    if tool_calls.is_empty() { None } else { Some(tool_calls) },
                ),
                finish_reason: Some("stop".into()),
            }],
            usage: usage.unwrap_or(crate::models::chat::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                prompt_tokens_details: None,
            }),
            extra: Default::default(),
        })
    }
}

#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn next_sse_event(buffer: &mut BytesMut) -> Option<BytesMut> {
    let crlf_pos = buffer.windows(4).position(|w| w == b"\r\n\r\n");
    let lf_pos = buffer.windows(2).position(|w| w == b"\n\n");
    let (pos, delimiter_len) = match (crlf_pos, lf_pos) {
        (Some(c), Some(l)) if c + 2 <= l => (c, 4),
        (Some(c), None) => (c, 4),
        (None, Some(l)) => (l, 2),
        (Some(_), Some(l)) => (l, 2),
        (None, None) => return None,
    };
    let event = buffer.split_to(pos);
    buffer.advance(delimiter_len);
    Some(event)
}

#[async_trait::async_trait]
impl LlmProvider for OpenAICompatibleProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.chat(request).await
    }
}

fn openai_message(msg: &crate::models::Message) -> serde_json::Value {
    let mut value = serde_json::json!({
        "role": msg.role.to_string(),
        "content": msg.content,
    });

    if let Some(tool_calls) = &msg.tool_calls {
        let calls: Vec<serde_json::Value> = tool_calls.iter().map(openai_tool_call).collect();
        value["tool_calls"] = serde_json::Value::Array(calls);
    }

    if let Some(tool_call_id) = &msg.tool_call_id {
        value["tool_call_id"] = serde_json::Value::String(tool_call_id.clone());
        value["name"] = serde_json::Value::String(msg.name.clone().unwrap_or_default());
    }

    value
}

fn openai_tool_call(tc: &crate::models::tool::ToolCall) -> serde_json::Value {
    serde_json::json!({
        "id": tc.id,
        "type": "function",
        "function": {
            "name": tc.name,
            "arguments": tc.arguments.to_string()
        }
    })
}

fn openai_tool(tool: &crate::models::Tool) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema.as_ref().unwrap_or(&serde_json::json!({}))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::tool::ToolCall;
    use crate::models::Role;

    #[test]
    fn openai_tool_format_uses_function_wrapper() {
        let tool = crate::models::Tool {
            name: "read_file".into(),
            description: Some("Read a file".into()),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } }
            })),
        };
        let json = openai_tool(&tool);
        assert_eq!(json["type"], "function");
        assert_eq!(json["function"]["name"], "read_file");
        assert!(json["function"]["parameters"].get("properties").is_some());
    }

    #[test]
    fn openai_message_serializes_tool_call_with_function_schema() {
        let msg = crate::models::Message {
            role: Role::Assistant,
            content: None,
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_1".into(),
                name: "shell".into(),
                arguments: serde_json::json!({ "command": "echo hi" }),
            }]),
            reasoning_content: None,
        };
        let json = openai_message(&msg);
        let calls = json["tool_calls"].as_array().unwrap();
        assert_eq!(calls[0]["id"], "call_1");
        assert_eq!(calls[0]["type"], "function");
        assert_eq!(calls[0]["function"]["name"], "shell");
    }

    #[test]
    fn sse_event_splitting_handles_crlf_and_lf() {
        let mut buf = BytesMut::from("event: msg\ndata: hello\r\n\r\ndata: world\n\n");
        let ev1 = next_sse_event(&mut buf).unwrap();
        assert!(String::from_utf8_lossy(&ev1).contains("hello"));
        let ev2 = next_sse_event(&mut buf).unwrap();
        assert!(String::from_utf8_lossy(&ev2).contains("world"));
    }
}
