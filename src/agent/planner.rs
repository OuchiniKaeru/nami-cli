use crate::models::{
    chat::{ChatRequest, StreamOptions},
    message::Message,
};

pub struct Planner;

impl Planner {
    pub fn new() -> Self {
        Self
    }

    pub fn build_request(
        &self,
        model: &str,
        messages: Vec<Message>,
        tools: Vec<crate::models::tool::Tool>,
        temperature: f32,
        max_tokens: u32,
        _max_iterations: u32,
        stream: bool,
    ) -> ChatRequest {
        ChatRequest {
            model: model.to_string(),
            messages,
            temperature: Some(temperature),
            max_tokens: Some(max_tokens),
            stream: Some(stream),
            tools: if tools.is_empty() { None } else { Some(tools) },
            stream_options: if stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
            extra: Default::default(),
        }
    }

    pub fn system_prompt(&self, base: &str) -> String {
        format!("{}\n\nYou are a careful assistant.", base.trim_end())
    }
}
