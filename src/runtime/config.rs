use crate::runtime::permissions::AgentMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RoutingPolicy {
    Performance,  // Always high-tier models
    Cost,         // Dynamic complexity estimation
    Resilient,    // Aggressive fallback to secondary/local
    ProviderLock, // Respect 'provider' field strictly
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekConfig {
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub endpoint: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3RelayConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    #[serde(skip_serializing)] // never persist secrets to disk
    pub access_key: String,
    #[serde(skip_serializing)]
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

    /// Returns the path to the persisted config file.
    pub fn config_file_path(state_dir: &std::path::Path) -> PathBuf {
        state_dir.join("config.toml")
    }

    /// Load config from ~/.dreamswarm/config.toml, falling back to defaults.
    pub fn load_from_toml(state_dir: &std::path::Path) -> Self {
        let path = Self::config_file_path(state_dir);
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match toml::from_str::<Self>(&content) {
                    Ok(cfg) => {
                        tracing::info!("Loaded config from {:?}", path);
                        return cfg;
                    }
                    Err(e) => tracing::warn!("Config parse error (using defaults): {}", e),
                },
                Err(e) => tracing::warn!("Config read error (using defaults): {}", e),
            }
        }
        Self::default()
    }

    /// Persist current config to ~/.dreamswarm/config.toml.
    pub fn save_to_toml(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.state_dir)?;
        let path = Self::config_file_path(&self.state_dir);
        let content = toml::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("TOML serialization error: {}", e))?;
        std::fs::write(&path, content)?;
        tracing::info!("Config saved to {:?}", path);
        Ok(())
    }
}
