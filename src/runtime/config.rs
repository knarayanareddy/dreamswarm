use crate::runtime::permissions::AgentMode;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub model: String,
    pub provider: String,
    pub permission_mode: AgentMode,
    pub allow_patterns: Vec<String>,
    pub deny_patterns: Vec<String>,
    pub working_dir: PathBuf,
    pub state_dir: PathBuf,
    pub s3_relay_config: Option<S3RelayConfig>,
    pub routing_policy: RoutingPolicy,
    pub deepseek_config: Option<DeepSeekConfig>,
    pub ollama_config: Option<OllamaConfig>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum RoutingPolicy {
    Performance,  // Always high-tier models
    Cost,         // Dynamic complexity estimation
    Resilient,    // Aggressive fallback to secondary/local
    ProviderLock, // Respect 'provider' field strictly
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeepSeekConfig {
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OllamaConfig {
    pub endpoint: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct S3RelayConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            provider: "anthropic".to_string(),
            permission_mode: AgentMode::Default,
            allow_patterns: vec![
                "Bash(git status)".into(),
                "Bash(ls*)".into(),
                "FileRead(*)".into(),
                "Bash(cargo check)".into(),
                "Bash(cargo test)".into(),
            ],
            deny_patterns: vec![
                "Bash(rm -rf *)".into(),
                "Bash(sudo *)".into(),
                "Bash(curl * | bash)".into(),
                "FileWrite(.env*)".into(),
            ],
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            state_dir: home.join(".dreamswarm"),
            s3_relay_config: None,
            routing_policy: RoutingPolicy::Resilient,
            deepseek_config: None,
            ollama_config: Some(OllamaConfig {
                endpoint: "http://localhost:11434".to_string(),
                model: "llama3.1:8b".to_string(),
            }),
        }
    }
}

impl AppConfig {
    pub fn new(model: String, provider: String, permission_mode: String) -> Self {
        let mut config = Self::default();
        config.model = model;
        config.provider = provider;
        config.permission_mode = permission_mode.parse().unwrap_or(AgentMode::Default);
        config
    }
}
