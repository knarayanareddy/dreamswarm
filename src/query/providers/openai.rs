//! OpenAI API provider — compatible with GPT-4o, o1, o3 and any OpenAI-compatible endpoint.

use crate::query::engine::{CompletionResponse, LLMProvider, Usage};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use uuid::Uuid;

pub struct OpenAIProvider {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAIProvider {
    pub fn new(model: &str) -> anyhow::Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set. Export it: export OPENAI_API_KEY=sk-..."))?;
        Ok(Self {
            client: Client::new(),
            api_key,
            model: model.to_string(),
            base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
        })
    }

    /// Convert Anthropic-style tool schemas to OpenAI function calling format.
    fn convert_tools(tools: &[Value]) -> Vec<Value> {
        tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool["name"],
                        "description": tool["description"],
                        "parameters": tool["input_schema"]
                    }
                })
            })
            .collect()
    }

    /// Estimate cost in USD per million tokens.
    fn estimate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let (input_price, output_price) = if self.model.contains("gpt-4o-mini") {
            (0.15, 0.60)
        } else if self.model.contains("gpt-4o") {
            (2.50, 10.0)
        } else if self.model.contains("gpt-4-turbo") {
            (10.0, 30.0)
        } else if self.model.contains("o1") || self.model.contains("o3") {
            (15.0, 60.0)
        } else {
            (2.50, 10.0) // Default to gpt-4o pricing
        };
        (input_tokens as f64 * input_price / 1_000_000.0)
            + (output_tokens as f64 * output_price / 1_000_000.0)
    }

    /// Convert OpenAI response format to our unified `CompletionResponse`.
    fn convert_response(&self, response_json: &Value) -> anyhow::Result<CompletionResponse> {
        let choice = &response_json["choices"][0];
        let message = &choice["message"];
        let mut content: Vec<Value> = Vec::new();

        // Add text content
        if let Some(text) = message["content"].as_str() {
            if !text.is_empty() {
                content.push(serde_json::json!({ "type": "text", "text": text }));
            }
        }

        // Convert tool calls to Anthropic-compatible format
        if let Some(tool_calls) = message["tool_calls"].as_array() {
            for tc in tool_calls {
                let function = &tc["function"];
                let args: Value = serde_json::from_str(
                    function["arguments"].as_str().unwrap_or("{}"),
                )
                .unwrap_or(serde_json::json!({}));
                content.push(serde_json::json!({
                    "type": "tool_use",
                    "id": tc["id"].as_str().unwrap_or(&Uuid::new_v4().to_string()),
                    "name": function["name"],
                    "input": args
                }));
            }
        }

        let usage_json = &response_json["usage"];
        let input_tokens = usage_json["prompt_tokens"].as_u64().unwrap_or(0);
        let output_tokens = usage_json["completion_tokens"].as_u64().unwrap_or(0);
        let usage = Usage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
            cost_usd: self.estimate_cost(input_tokens, output_tokens),
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };

        let stop_reason = choice["finish_reason"]
            .as_str()
            .unwrap_or("stop")
            .to_string();

        Ok(CompletionResponse {
            content,
            usage,
            stop_reason,
            model: self.model.clone(),
        })
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Value],
        tools: &[Value],
    ) -> anyhow::Result<CompletionResponse> {
        // Build system message
        let mut oai_messages: Vec<Value> = vec![serde_json::json!({
            "role": "system",
            "content": system_prompt
        })];

        // Convert Anthropic-format messages to OpenAI format
        for msg in messages {
            let role = msg["role"].as_str().unwrap_or("user");
            match msg["content"].clone() {
                Value::String(text) => {
                    oai_messages.push(serde_json::json!({ "role": role, "content": text }));
                }
                Value::Array(blocks) => {
                    // Flatten content blocks into OpenAI format
                    for block in &blocks {
                        match block["type"].as_str() {
                            Some("text") => {
                                oai_messages.push(serde_json::json!({
                                    "role": role,
                                    "content": block["text"]
                                }));
                            }
                            Some("tool_use") => {
                                oai_messages.push(serde_json::json!({
                                    "role": "assistant",
                                    "tool_calls": [{
                                        "id": block["id"],
                                        "type": "function",
                                        "function": {
                                            "name": block["name"],
                                            "arguments": block["input"].to_string()
                                        }
                                    }]
                                }));
                            }
                            Some("tool_result") => {
                                oai_messages.push(serde_json::json!({
                                    "role": "tool",
                                    "tool_call_id": block["tool_use_id"],
                                    "content": block["content"]
                                }));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": oai_messages,
            "max_tokens": 8096,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::json!(Self::convert_tools(tools));
            body["tool_choice"] = serde_json::json!("auto");
        }

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
            return Err(anyhow::anyhow!("OpenAI API error {}: {}", status, text));
        }

        let response_json: Value = response.json().await?;
        self.convert_response(&response_json)
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
