use crate::runtime::permissions::RiskLevel;
use crate::swarm::mailbox::Mailbox;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct RequestHelpTool {
    pub mailbox: Arc<RwLock<Mailbox>>,
}

#[async_trait]
impl Tool for RequestHelpTool {
    fn name(&self) -> &str {
        "RequestHelp"
    }
    fn description(&self) -> &str {
        "Request help from another agent. Use this to delegate tasks or ask for specialized knowledge."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "to": { "type": "string", "description": "The name of the agent to help (e.g. 'coder', 'architect')" },
                "task": { "type": "string", "description": "The specific task or question you need help with" }
            },
            "required": ["to", "task"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let to = input["to"].as_str().unwrap_or_default();
        let task = input["task"].as_str().unwrap_or_default();
        let request_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

        let mailbox = self.mailbox.write().await;
        mailbox.send_help_request(to, &request_id, task)?;

        Ok(ToolOutput {
            content: format!("Help request sent to '{}' with ID: {}. You will need to use CheckInbox later to see the response.", to, request_id),
            is_error: false,
        })
    }
}

pub struct CheckInboxTool {
    pub mailbox: Arc<RwLock<Mailbox>>,
}

#[async_trait]
impl Tool for CheckInboxTool {
    fn name(&self) -> &str {
        "CheckInbox"
    }
    fn description(&self) -> &str {
        "Check your mailbox for any new messages, help requests, or responses from other agents."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }
    async fn execute(&self, _input: &Value) -> anyhow::Result<ToolOutput> {
        let mut mailbox = self.mailbox.write().await;
        let messages = mailbox.receive()?;

        if messages.is_empty() {
            return Ok(ToolOutput {
                content: "Your inbox is empty.".to_string(),
                is_error: false,
            });
        }

        let content = serde_json::to_string_pretty(&messages)?;
        Ok(ToolOutput {
            content,
            is_error: false,
        })
    }
}
