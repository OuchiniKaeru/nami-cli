use super::*;
use crate::models::chat::{ChatResponse, Usage};
use crate::models::config::ProviderConfig;
use crate::models::message::{Message, Role};
use std::collections::HashMap;

pub struct GeminiProvider {
    pub client: reqwest::Client,
    pub cfg: ProviderConfig,
}

impl GeminiProvider {
    pub fn new(cfg: ProviderConfig) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("nami/0.1")
            .build()?;
        Ok(Self { client, cfg })
    }

    fn base_url(&self) -> &str {
        self.cfg.base_url.as_deref().unwrap_or("https://generativelanguage.googleapis.com/v1beta")
    }

    pub async fn chat(&self, request: crate::models::chat::ChatRequest) -> Result<ChatResponse, ProviderError> {
        let api_key = match &self.cfg.api_key {
            Some(k) => k,
            None => return Err(ProviderError::Api("gemini api_key is required".into())),
        };

        let model = request.model;
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url().trim_end_matches('/'),
            model,
            api_key
        );

        let contents = request
            .messages
            .into_iter()
            .map(|m| gemini_content(m))
            .collect::<Vec<_>>();

        let body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "temperature": request.temperature.unwrap_or(0.2),
                "maxOutputTokens": request.max_tokens.unwrap_or(1024),
            }
        });

        let resp = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ProviderError::Api(format!("status={} body={}", status, text)));
        }

        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| ProviderError::Parse(format!("gemini response parse error: {}", e)))?;

        let choice = gemini_to_chat_choice(&v).ok_or_else(|| {
            ProviderError::Parse("failed to map gemini response to chat choice".into())
        })?;

        let usage = gemini_to_usage(&v).unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            prompt_tokens_details: None,
        });

        Ok(ChatResponse {
            id: v.get("usageMetadata").map(|_| uuid::Uuid::new_v4().to_string()).unwrap_or_default(),
            object: "chat.completion".into(),
            created: chrono::Utc::now().timestamp() as u64,
            model,
            choices: vec![choice],
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
}

fn gemini_content(message: Message) -> serde_json::Value {
    let role = match message.role {
        Role::System => "user",
        Role::User => "user",
        Role::Assistant => "model",
        Role::Tool => "user",
    };

    let mut obj = serde_json::json!({
        "role": role,
    });

    let parts = if let Some(content) = message.content {
        vec![serde_json::json!({"text": content})]
    } else {
        vec![]
    };

    obj["parts"] = serde_json::json!(parts);
    obj
}

fn gemini_to_chat_choice(v: &serde_json::Value) -> Option<crate::models::chat::ChatChoice> {
    let candidates = v.get("candidates")?.as_array()?;
    let first = candidates.first()?;
    let content = first.get("content")?;
    let parts = content.get("parts")?.as_array()?;

    let mut text = String::new();
    for p in parts {
        if let Some(t) = p.get("text").and_then(|x| x.as_str()) {
            text.push_str(t);
        }
    }

    Some(crate::models::chat::ChatChoice {
        index: 0,
        message: crate::models::Message::assistant(Some(text), None),
        finish_reason: first.get("finishReason").and_then(|x| x.as_str()).map(|s| s.into()),
    })
}

fn gemini_to_usage(v: &serde_json::Value) -> Option<Usage> {
    let usage = v.get("usageMetadata")?;
    let prompt = usage.get("promptTokenCount").and_then(|x| x.as_u64()).unwrap_or(0);
    let candidates_tokens = usage.get("candidatesTokenCount").and_then(|x| x.as_u64()).unwrap_or(0);
    let total = usage.get("totalTokenCount").and_then(|x| x.as_u64()).unwrap_or(prompt + candidates_tokens);

    Some(Usage {
        prompt_tokens: prompt,
        completion_tokens: candidates_tokens,
        total_tokens: total,
        prompt_tokens_details: None,
    })
}

