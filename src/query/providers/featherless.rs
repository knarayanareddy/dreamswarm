use crate::query::engine::{CompletionResponse, LLMProvider, Usage};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

pub struct FeatherlessProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl FeatherlessProvider {
    pub fn new(model: &str) -> anyhow::Result<Self> {
        let api_key = std::env::var("FEATHERLESS_API_KEY").map_err(|_| {
            anyhow::anyhow!(
                "FEATHERLESS_API_KEY not set. Export it: export FEATHERLESS_API_KEY=..."
            )
        })?;
        Ok(Self {
            client: Client::new(),
            api_key,
            model: model.to_string(),
            base_url: "https://api.featherless.ai/v1".to_string(),
        })
    }

    fn estimate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        // Featherless pricing varies by model, but often around $0.20-0.50 per 1M tokens
        // Defaulting to a conservative estimate.
        (input_tokens as f64 * 0.30 / 1_000_000.0) + (output_tokens as f64 * 0.60 / 1_000_000.0)
    }

    fn convert_response(&self, response_json: &Value) -> anyhow::Result<CompletionResponse> {
        // Featherless follows the OpenAI response format
        let choice = &response_json["choices"][0];
        let message = &choice["message"];
        let mut content: Vec<Value> = Vec::new();

        if let Some(text) = message["content"].as_str() {
            if !text.is_empty() {
                content.push(serde_json::json!({ "type": "text", "text": text }));
            }
        }

        let usage_json = &response_json["usage"];
        let input_tokens = usage_json["prompt_tokens"].as_u64().unwrap_or(0);
        let output_tokens = usage_json["completion_tokens"].as_u64().unwrap_or(0);

        Ok(CompletionResponse {
            content,
            usage: Usage {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
                cost_usd: self.estimate_cost(input_tokens, output_tokens),
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
            stop_reason: choice["finish_reason"]
                .as_str()
                .unwrap_or("stop")
                .to_string(),
            model: self.model.clone(),
        })
    }
}

#[async_trait]
impl LLMProvider for FeatherlessProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Value],
        _tools: &[Value],
    ) -> anyhow::Result<CompletionResponse> {
        // Build system message
        let mut oai_messages: Vec<Value> = vec![serde_json::json!({
            "role": "system",
            "content": system_prompt
        })];

        // Simplified conversion for now (standard OpenAI format)
        for msg in messages {
            oai_messages.push(msg.clone());
        }

        let body = serde_json::json!({
            "model": self.model,
            "messages": oai_messages,
            "max_tokens": 4096,
        });

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Featherless API error {}: {}",
                status,
                text
            ));
        }

        let response_json: Value = response.json().await?;
        self.convert_response(&response_json)
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
