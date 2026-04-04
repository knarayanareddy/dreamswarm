use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;

// Submodules for Phase 1 tools
pub mod file_read;
pub mod file_write;
pub mod bash_tool;
pub mod grep_tool;
pub mod ask_user;

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
        format!("{}", self.name())
    }
    
    fn describe_call(&self, input: &Value) -> String {
        format!("Execute {} with input: {}", self.name(), input)
    }
}

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
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
}
