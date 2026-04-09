use std::path::PathBuf;
use crate::runtime::permissions::AgentMode;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub model: String,
    pub provider: String,
    pub permission_mode: AgentMode,
    pub allow_patterns: Vec<String>,
    pub deny_patterns: Vec<String>,
    pub working_dir: PathBuf,
    pub state_dir: PathBuf,
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
        }
    }
}

impl AppConfig {
    pub fn new(model: String, provider: String, permission_mode: String) -> Self {
        let mut config = Self::default();
        config.model = model;
        config.provider = provider;
        config.permission_mode = AgentMode::from_str(&permission_mode);
        config
    }
}

