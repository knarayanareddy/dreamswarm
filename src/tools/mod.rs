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
pub mod git;
pub mod grep_tool;
pub mod js_tool;
pub mod memory_tools;
pub mod monitor_tool;
pub mod push_notification;
pub mod python_tool;
pub mod rust_debug;
pub mod swarm_tools;

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

    pub fn get_tool(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.name().eq_ignore_ascii_case(name))
            .map(|t| t.as_ref())
    }

    pub fn get_all_schemas(&self) -> Vec<Value> {
        self.tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.input_schema(),
                })
            })
            .collect()
    }

    pub fn default_phase1(
        memory: std::sync::Arc<tokio::sync::RwLock<crate::memory::MemorySystem>>,
        mailbox: Option<std::sync::Arc<tokio::sync::RwLock<crate::swarm::mailbox::Mailbox>>>,
    ) -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(file_read::FileReadTool));
        registry.register(Box::new(file_write::FileWriteTool));
        registry.register(Box::new(bash_tool::BashTool));
        registry.register(Box::new(grep_tool::GrepTool {
            memory: memory.clone(),
        }));
        registry.register(Box::new(ask_user::AskUserTool));
        registry.register(Box::new(git::GitBranchTool));
        registry.register(Box::new(git::GitCommitTool));

        if let Some(mbox) = mailbox {
            registry.register(Box::new(swarm_tools::RequestHelpTool {
                mailbox: mbox.clone(),
            }));
            registry.register(Box::new(swarm_tools::CheckInboxTool { mailbox: mbox }));
        }

        let mem_dir = memory
            .try_read()
            .map(|m| m.memory_dir().clone())
            .unwrap_or_default();

        registry.register(Box::new(python_tool::PythonExecuteTool));
        registry.register(Box::new(js_tool::JSExecuteTool));
        registry.register(Box::new(rust_debug::RustCheckTool));
        registry.register(Box::new(rust_debug::DebuggerTool));
        registry.register(Box::new(rust_debug::TraceAnalyzerTool));
        registry.register(Box::new(memory_tools::PublishKnowledgeTool {
            memory_dir: mem_dir.clone(),
        }));
        registry.register(Box::new(memory_tools::SearchKnowledgeTool {
            memory_dir: mem_dir,
        }));

        registry
    }

    pub fn default_phase5(
        memory: std::sync::Arc<tokio::sync::RwLock<crate::memory::MemorySystem>>,
        query_engine: std::sync::Arc<crate::query::engine::QueryEngine>,
        working_dir: &str,
        mailbox: Option<std::sync::Arc<tokio::sync::RwLock<crate::swarm::mailbox::Mailbox>>>,
    ) -> Self {
        let mut registry = Self::default_phase1(memory.clone(), mailbox);

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
