use crate::daemon::process::DaemonProcess;
use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;

pub struct DaemonStatusTool {
    state_dir: std::path::PathBuf,
}

impl DaemonStatusTool {
    pub fn new(state_dir: std::path::PathBuf) -> Self {
        Self { state_dir }
    }
}

#[async_trait]
impl Tool for DaemonStatusTool {
    fn name(&self) -> &str {
        "DaemonStatus"
    }
    fn description(&self) -> &str {
        "Get the current status of the KAIROS background daemon. Shows whether \
        it's running, trust level, actions taken today, and recent log entries."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "include_log": { "type": "boolean", "description": "Include recent log entries (default: true)" },
                "log_count": { "type": "integer", "description": "Number of recent log entries (default: 10)" }
            }
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }
    fn command_signature(&self, _: &Value) -> String {
        "daemon:status".into()
    }
    fn describe_call(&self, _: &Value) -> String {
        "Check KAIROS daemon status".into()
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let include_log = input
            .get("include_log")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let log_count = input
            .get("log_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;
        let process = DaemonProcess::new(self.state_dir.clone());
        let status = process.status().await?;
        let mut output = format!(
            "## KAIROS Daemon Status\n\nRunning: {}\n",
            if status.running { "Yes" } else { "No" }
        );
        if let Some(started) = status.started_at {
            output.push_str(&format!(
                "Started: {}\n",
                started.format("%Y-%m-%d %H:%M UTC")
            ));
        }
        output.push_str(&format!(
            "Actions today: {}\nTokens today: {}\nCost today: ${:.4}\n",
            status.actions_taken, status.tokens_used_today, status.cost_today_usd
        ));
        if include_log {
            let daily_log = crate::daemon::daily_log::DailyLog::new(&self.state_dir)?;
            let entries = daily_log.read_today()?;
            let recent: Vec<_> = entries.iter().rev().take(log_count).collect();
            if !recent.is_empty() {
                output.push_str(&format!("\n## Recent Log ({} entries)\n\n", recent.len()));
                for entry in recent {
                    output.push_str(&format!(
                        "[{}] {:?}: {}\n",
                        entry.timestamp.format("%H:%M:%S"),
                        entry.kind,
                        &entry.content[..entry.content.len().min(120)]
                    ));
                }
            }
        }
        Ok(ToolOutput {
            content: output,
            is_error: false,
        })
    }
}
