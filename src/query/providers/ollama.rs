//! Ollama Local LLM provider
use crate::query::engine::{CompletionResponse, LLMProvider, Usage};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

pub struct OllamaProvider {
    client: Client,
    endpoint: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(endpoint: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Value],
        _tools: &[Value],
    ) -> anyhow::Result<CompletionResponse> {
        // Ollama uses an OpenAI-compatible /v1/chat/completions endpoint in recent versions,
        // or its native /api/chat. We use /api/chat for better compatibility with all Ollama versions.
        
        let mut ollama_messages: Vec<Value> = vec![serde_json::json!({
            "role": "system",
            "content": system_prompt
        })];

        for msg in messages {
            ollama_messages.push(msg.clone());
        }

        let body = serde_json::json!({
            "model": self.model,
            "messages": ollama_messages,
            "stream": false,
        });

        let response = self
            .client
            .post(format!("{}/api/chat", self.endpoint))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Ollama Error: {}", response.text().await?));
        }

        let response_json: Value = response.json().await?;
        let message = &response_json["message"];
        
        let mut content: Vec<Value> = Vec::new();
        if let Some(text) = message["content"].as_str() {
            content.push(serde_json::json!({ "type": "text", "text": text }));
        }

        Ok(CompletionResponse {
            content,
            usage: Usage {
                input_tokens: response_json["prompt_eval_count"].as_u64().unwrap_or(0),
                output_tokens: response_json["eval_count"].as_u64().unwrap_or(0),
                total_tokens: 0, // Ollama doesn't always provide total sum
                cost_usd: 0.0,   // Local execution is zero-cost
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
            stop_reason: "done".to_string(),
            model: self.model.clone(),
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
