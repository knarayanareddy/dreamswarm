//! Galactic Model Router: Multi-provider orchestration and fallback.
use crate::query::engine::{CompletionResponse, LLMProvider};
use crate::runtime::config::{AppConfig, RoutingPolicy};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

pub struct ModelRouter {
    providers: HashMap<String, Box<dyn LLMProvider>>,
    policy: RoutingPolicy,
    hierarchy: Vec<String>,
}

impl ModelRouter {
    pub fn new(config: &AppConfig) -> Self {
        let mut providers: HashMap<String, Box<dyn LLMProvider>> = HashMap::new();

        // 1. Anthropic (Primary)
        if let Ok(p) = crate::query::providers::anthropic::AnthropicProvider::new(&config.model) {
            providers.insert("anthropic".to_string(), Box::new(p));
        }

        // 2. OpenAI (Secondary)
        if let Ok(p) = crate::query::providers::openai::OpenAIProvider::new("gpt-4o") {
            providers.insert("openai".to_string(), Box::new(p));
        }

        // 3. DeepSeek (Cost Optimization)
        if let Some(ds_conf) = &config.deepseek_config {
            if let Ok(p) = crate::query::providers::deepseek::DeepSeekProvider::new(
                &ds_conf.model,
                Some(ds_conf.api_key.clone()),
            ) {
                providers.insert("deepseek".to_string(), Box::new(p));
            }
        }

        // 4. Ollama (Local Resilience)
        if let Some(ol_conf) = &config.ollama_config {
            let p = crate::query::providers::ollama::OllamaProvider::new(
                &ol_conf.endpoint,
                &ol_conf.model,
            );
            providers.insert("ollama".to_string(), Box::new(p));
        }

        Self {
            providers,
            policy: config.routing_policy.clone(),
            hierarchy: vec![
                "anthropic".into(),
                "openai".into(),
                "deepseek".into(),
                "ollama".into(),
            ],
        }
    }

    /// Estimating task complexity based on prompt content.
    fn estimate_complexity(&self, system_prompt: &str, messages: &[Value]) -> Complexity {
        let text = format!("{} {:?}", system_prompt, messages).to_lowercase();

        if text.contains("architect")
            || text.contains("refactor")
            || text.contains("security audit")
        {
            Complexity::High
        } else if text.contains("summarize")
            || text.contains("cleanup")
            || text.contains("formatting")
        {
            Complexity::Low
        } else {
            Complexity::Medium
        }
    }

    fn select_provider(&self, complexity: Complexity) -> String {
        match self.policy {
            RoutingPolicy::Cost => match complexity {
                Complexity::Low => "deepseek".to_string(),
                _ => "anthropic".to_string(),
            },
            RoutingPolicy::Performance => "anthropic".to_string(),
            _ => "anthropic".to_string(), // Default to primary
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Complexity {
    Low,
    Medium,
    High,
}

#[async_trait]
impl LLMProvider for ModelRouter {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Value],
        tools: &[Value],
    ) -> anyhow::Result<CompletionResponse> {
        let complexity = self.estimate_complexity(system_prompt, messages);
        let target = self.select_provider(complexity);

        // Attempt hierarchy fallback starting from target or primary
        let start_idx = self
            .hierarchy
            .iter()
            .position(|r| r == &target)
            .unwrap_or(0);

        for i in start_idx..self.hierarchy.len() {
            let provider_name = &self.hierarchy[i];
            if let Some(provider) = self.providers.get(provider_name) {
                match provider.complete(system_prompt, messages, tools).await {
                    Ok(resp) => return Ok(resp),
                    Err(e) => {
                        tracing::warn!(
                            "Galactic Router: Provider '{}' failed: {}. Falling back...",
                            provider_name,
                            e
                        );
                        continue;
                    }
                }
            }
        }

        anyhow::bail!("Galactic Router: All cognitive providers exhausted hierarchy.")
    }

    fn model_name(&self) -> &str {
        "galactic-router"
    }
}
