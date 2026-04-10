//! DeepSeek API provider (OpenAI-compatible)
use crate::query::engine::{CompletionResponse, LLMProvider, Usage};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

pub struct DeepSeekProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl DeepSeekProvider {
    pub fn new(model: &str, api_key: Option<String>) -> anyhow::Result<Self> {
        let api_key = api_key
            .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())
            .ok_or_else(|| anyhow::anyhow!("DEEPSEEK_API_KEY not set"))?;

        Ok(Self {
            client: Client::new(),
            api_key,
            model: model.to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
        })
    }

    fn estimate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        // DeepSeek-V3/R1 pricing (Estimated at $0.14/$0.28 per 1M tokens)
        (input_tokens as f64 * 0.14 / 1_000_000.0) + (output_tokens as f64 * 0.28 / 1_000_000.0)
    }

    fn convert_response(&self, response_json: &Value) -> anyhow::Result<CompletionResponse> {
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
                cache_read_tokens: usage_json["prompt_cache_hit_tokens"].as_u64().unwrap_or(0),
                cache_creation_tokens: usage_json["prompt_cache_miss_tokens"].as_u64().unwrap_or(0),
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
impl LLMProvider for DeepSeekProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Value],
        _tools: &[Value],
    ) -> anyhow::Result<CompletionResponse> {
        let mut ds_messages: Vec<Value> = vec![serde_json::json!({
            "role": "system",
            "content": system_prompt
        })];

        for msg in messages {
            ds_messages.push(msg.clone());
        }

        let body = serde_json::json!({
            "model": self.model,
            "messages": ds_messages,
            "stream": false,
        });

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "DeepSeek Error: {}",
                response.text().await?
            ));
        }

        let response_json: Value = response.json().await?;
        self.convert_response(&response_json)
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
