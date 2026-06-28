use crate::models::chat::ChatResponse;
use crate::models::config::{ProviderConfig, ProviderKind};

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error: {0}")]
    Api(String),
    #[error("parse error: {0}")]
    Parse(String),
}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: crate::models::chat::ChatRequest)
        -> Result<ChatResponse, ProviderError>;

    /// このプロバイダーがターミナルへのストリーミング表示に対応しているか。
    /// 未対応の場合、main.rs は集約された answer を改行後に表示する。
    fn supports_streaming(&self) -> bool {
        true
    }
}

pub use openai::OpenAICompatibleProvider;
pub use gemini::GeminiProvider;

pub fn build(cfg: ProviderConfig) -> anyhow::Result<Box<dyn LlmProvider>> {
    match cfg.kind {
        ProviderKind::OpenAI
        | ProviderKind::OpenRouter
        | ProviderKind::AzureOpenAI
        | ProviderKind::Groq
        | ProviderKind::Anthropic
        | ProviderKind::Custom => Ok(Box::new(openai::OpenAICompatibleProvider::new(cfg)?)),
        ProviderKind::Gemini => Ok(Box::new(gemini::GeminiProvider::new(cfg)?)),
    }
}

mod gemini;
mod openai;
