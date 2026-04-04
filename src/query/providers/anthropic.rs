use crate::query::engine::{CompletionResponse, LLMProvider, Usage};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicProvider {
    pub fn new(model: &str) -> anyhow::Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!(
                "ANTHROPIC_API_KEY not set. Export it: export ANTHROPIC_API_KEY=sk-ant-..."
            ))?;
            
        Ok(Self {
            client: Client::new(),
            api_key,
            model: model.to_string(),
            base_url: std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com".to_string()),
        })
    }

    fn estimate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let (input_price, output_price) = if self.model.contains("opus") {
            (15.0, 75.0)
        } else if self.model.contains("sonnet") {
            (3.0, 15.0)
        } else if self.model.contains("haiku") {
            (0.25, 1.25)
        } else {
            (3.0, 15.0) // Default to sonnet pricing
        };
        
        (input_tokens as f64 * input_price / 1_000_000.0)
            + (output_tokens as f64 * output_price / 1_000_000.0)
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Value],
        tools: &[Value],
    ) -> anyhow::Result<CompletionResponse> {
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 16384,
            "system": system_prompt,
            "messages": messages,
        });

        if !tools.is_empty() {
            body["tools"] = Value::Array(tools.to_vec());
        }

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
            
        let status = response.status();
        let response_text = response.text().await?;
        
        if !status.is_success() {
            anyhow::bail!("Anthropic API error ({}): {}", status, response_text);
        }

        let response_json: Value = serde_json::from_str(&response_text)?;
        
        let usage_json = &response_json["usage"];
        let input_tokens = usage_json["input_tokens"].as_u64().unwrap_or(0);
        let output_tokens = usage_json["output_tokens"].as_u64().unwrap_or(0);
        let cache_read = usage_json["cache_read_input_tokens"].as_u64().unwrap_or(0);
        let cache_creation = usage_json["cache_creation_input_tokens"].as_u64().unwrap_or(0);
        
        let usage = Usage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
            cost_usd: self.estimate_cost(input_tokens, output_tokens),
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
        };

        let content = response_json["content"].as_array().cloned().unwrap_or_default();
        let stop_reason = response_json["stop_reason"].as_str().unwrap_or("end_turn").to_string();
        let model = response_json["model"].as_str().unwrap_or(&self.model).to_string();

        Ok(CompletionResponse {
            content,
            usage,
            stop_reason,
            model,
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
