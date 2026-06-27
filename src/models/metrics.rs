use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cost {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
    pub currency: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub iterations: u32,
    pub llm_calls: u32,
    pub tool_calls: u32,
    pub mcp_calls: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
}

impl Metrics {
    pub fn record_llm_call(&mut self, usage: TokenUsage) {
        self.llm_calls += 1;
        self.usage = Some(usage);
    }

    pub fn record_tool_call(&mut self) {
        self.tool_calls += 1;
    }

    pub fn record_mcp_call(&mut self) {
        self.mcp_calls += 1;
    }

    pub fn record_iteration(&mut self) {
        self.iterations += 1;
    }
}
