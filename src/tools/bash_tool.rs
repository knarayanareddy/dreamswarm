use super::{Tool, ToolOutput};
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;

pub struct BashTool;

#[derive(Debug)]
pub enum SecurityVerdict {
    Allow,
    Deny(String),
    EscalateRisk(RiskLevel),
}

struct BashSecurityChain;

impl BashSecurityChain {
    fn validate(command: &str) -> SecurityVerdict {
        let blocklist = [
            r"rm\s+(-[rfR]+\s+)?/\s*$",
            r"rm\s+(-[rfR]+\s+)?\*",
            r":\(\)\{.*\|.*&\s*\};\s*:",
            r"mkfs\.",
            r"dd\s+.*of=/dev/",
            r">\s*/dev/sd",
            r"chmod\s+(-R\s+)?777\s+/",
            r"curl.*\|\s*(ba)?sh",
            r"wget.*\|\s*(ba)?sh",
            r"eval\s+\$\(",
        ];
        for pattern in &blocklist {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(command) {
                    return SecurityVerdict::Deny(format!("Command matches blocklist pattern: {}", pattern));
                }
            }
        }
        SecurityVerdict::EscalateRisk(RiskLevel::Dangerous)
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command in the user's shell."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                }
            },
            "required": ["command"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Dangerous
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let command = input["command"].as_str().unwrap_or_default();
        
        match BashSecurityChain::validate(command) {
            SecurityVerdict::Deny(reason) => {
                return Ok(ToolOutput { content: format!("Security blocked: {}", reason), is_error: true });
            }
            _ => {}
        }

        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .output()
            .await?;

        let is_error = !output.status.success();
        let mut content = String::from_utf8_lossy(&output.stdout).to_string();
        if is_error {
            content.push_str(&String::from_utf8_lossy(&output.stderr));
        }

        Ok(ToolOutput { content, is_error })
    }
}
