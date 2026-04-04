use crate::runtime::config::AppConfig;
use async_trait::async_trait;
use serde_json::Value;

pub mod providers;

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: Vec<Value>,
    pub usage: Usage,
    pub stop_reason: String,
    pub model: String,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Value],
        tools: &[Value],
    ) -> anyhow::Result<CompletionResponse>;

    fn model_name(&self) -> &str;
}

pub struct QueryEngine {
    provider: Box<dyn LLMProvider>,
}

impl QueryEngine {
    pub fn new(provider_name: &str, model: &str, _config: &AppConfig) -> anyhow::Result<Self> {
        let provider: Box<dyn LLMProvider> = match provider_name {
            "anthropic" => Box::new(providers::anthropic::AnthropicProvider::new(model)?),
            _ => anyhow::bail!("Unknown provider: {}. Supported: anthropic", provider_name),
        };
        Ok(Self { provider })
    }

    pub async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Value],
        tools: &[Value],
    ) -> anyhow::Result<CompletionResponse> {
        self.provider.complete(system_prompt, messages, tools).await
    }
}
