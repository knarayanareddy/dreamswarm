use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;

// Submodules for all phases
pub mod ask_user;
pub mod bash_tool;
pub mod daemon_status;
pub mod dream_trigger;
pub mod file_read;
pub mod file_write;
pub mod grep_tool;
pub mod monitor_tool;
pub mod push_notification;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    fn risk_level(&self) -> RiskLevel;

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput>;

    fn command_signature(&self, _input: &Value) -> String {
        self.name().to_lowercase().replace(' ', ":")
    }

    fn describe_call(&self, input: &Value) -> String {
        format!("Execute {} with input: {}", self.name(), input)
    }
}

pub struct ToolRegistry {
    pub tools: Vec<Box<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn default_phase1() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(file_read::FileReadTool));
        registry.register(Box::new(file_write::FileWriteTool));
        registry.register(Box::new(bash_tool::BashTool));
        registry.register(Box::new(grep_tool::GrepTool));
        registry.register(Box::new(ask_user::AskUserTool));
        registry
    }

    pub fn default_phase5(
        memory: std::sync::Arc<tokio::sync::RwLock<crate::memory::MemorySystem>>,
        query_engine: std::sync::Arc<crate::query::engine::QueryEngine>,
        working_dir: &str,
    ) -> Self {
        let mut registry = Self::default_phase1();

        // Phase 4 Tools
        let daemon_state_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".dreamswarm")
            .join("daemon");
        registry.register(Box::new(daemon_status::DaemonStatusTool::new(
            daemon_state_dir.clone(),
        )));
        registry.register(Box::new(push_notification::PushNotificationTool));
        registry.register(Box::new(monitor_tool::MonitorTool));

        // Phase 5 Tool
        registry.register(Box::new(dream_trigger::DreamTriggerTool::new(
            memory,
            query_engine,
            working_dir,
            daemon_state_dir,
        )));

        registry
    }
}
