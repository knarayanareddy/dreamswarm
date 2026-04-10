use crate::runtime::config::AppConfig;
use async_trait::async_trait;
use serde_json::Value;

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

pub struct MockProvider {
    model: String,
}

#[async_trait]
impl LLMProvider for MockProvider {
    async fn complete(
        &self,
        _system_prompt: &str,
        messages: &[Value],
        _tools: &[Value],
    ) -> anyhow::Result<CompletionResponse> {
        let last_message_content = messages
            .last()
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
            .and_then(|a| a.first());
        let last_message_type = last_message_content
            .and_then(|b| b.get("type"))
            .and_then(|t| t.as_str())
            .unwrap_or("");
        let last_message_text = last_message_content
            .and_then(|b| b.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("");

        // 1. Handle Tool Result (The "Observation")
        if last_message_type == "tool_result" {
            return Ok(CompletionResponse {
                content: vec![serde_json::json!({
                    "type": "text",
                    "text": "I've successfully executed the Bash command! The directory listing shows multiple files including Cargo.toml, src/, and tests/. How else can I assist with your DreamSwarm project?"
                })],
                usage: Usage::default(),
                stop_reason: "end_turn".to_string(),
                model: self.model.clone(),
            });
        }

        // 2. Handle Tool Trigger (The "Input")
        if last_message_text.to_lowercase().contains("run command") {
            return Ok(CompletionResponse {
                content: vec![serde_json::json!({
                    "type": "tool_use",
                    "id": "mock-tool-123",
                    "name": "Bash",
                    "input": { "command": "ls -F" }
                })],
                usage: Usage::default(),
                stop_reason: "tool_use".to_string(),
                model: self.model.clone(),
            });
        }

        Ok(CompletionResponse {
            content: vec![serde_json::json!({
                "type": "text",
                "text": "I am a Mock Agent. I am running locally without an API key. Try saying 'run command' to see me execute a tool!"
            })],
            usage: Usage::default(),
            stop_reason: "end_turn".to_string(),
            model: self.model.clone(),
        })
    }
    fn model_name(&self) -> &str {
        &self.model
    }
}

pub struct QueryEngine {
    provider: Box<dyn LLMProvider>,
}

impl QueryEngine {
    pub fn new(provider_name: &str, _model: &str, config: &AppConfig) -> anyhow::Result<Self> {
        let provider: Box<dyn LLMProvider> = match provider_name {
            "galactic" | "router" => Box::new(crate::query::router::ModelRouter::new(config)),
            "anthropic" => Box::new(crate::query::providers::anthropic::AnthropicProvider::new(
                &config.model,
            )?),
            "openai" => Box::new(crate::query::providers::openai::OpenAIProvider::new(
                &config.model,
            )?),
            "mock" => Box::new(MockProvider {
                model: config.model.clone(),
            }),
            _ => anyhow::bail!(
                "Unknown provider: {}. Supported: galactic, anthropic, openai, mock",
                provider_name
            ),
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
