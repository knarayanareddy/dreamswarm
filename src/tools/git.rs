use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;

pub struct GitBranchTool;

#[async_trait]
impl Tool for GitBranchTool {
    fn name(&self) -> &str { "GitBranch" }
    fn description(&self) -> &str { "Create, list, or delete branches." }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["create", "list", "delete"] },
                "name": { "type": "string" }
            },
            "required": ["action"]
        })
    }
    fn risk_level(&self) -> RiskLevel { RiskLevel::Dangerous }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let action = input["action"].as_str().unwrap_or("list");
        let mut cmd = Command::new("git");
        match action {
            "create" => {
                let name = input["name"].as_str().ok_or_else(|| anyhow::anyhow!("Branch name required"))?;
                cmd.args(["checkout", "-b", name]);
            }
            "delete" => {
                let name = input["name"].as_str().ok_or_else(|| anyhow::anyhow!("Branch name required"))?;
                cmd.args(["branch", "-D", name]);
            }
            _ => { cmd.arg("branch"); }
        }
        let output = cmd.output().await?;
        Ok(ToolOutput {
            content: String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr),
            is_error: !output.status.success(),
        })
    }
}

pub struct GitCommitTool;

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str { "GitCommit" }
    fn description(&self) -> &str { "Stage all changes and create a commit." }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "The commit message" }
            },
            "required": ["message"]
        })
    }
    fn risk_level(&self) -> RiskLevel { RiskLevel::Dangerous }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let message = input["message"].as_str().unwrap_or("Automated commit by DreamSwarm");
        
        // Stage all
        Command::new("git").args(["add", "."]).output().await?;
        
        // Commit
        let output = Command::new("git").args(["commit", "-m", message]).output().await?;
        
        Ok(ToolOutput {
            content: String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr),
            is_error: !output.status.success(),
        })
    }
}
