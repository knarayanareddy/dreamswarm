use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;

pub struct RustCheckTool;

#[async_trait]
impl Tool for RustCheckTool {
    fn name(&self) -> &str {
        "RustCheck"
    }
    fn description(&self) -> &str {
        "Run cargo check with JSON output and return structured error analysis for faster debugging."
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
        let output = Command::new("cargo")
            .args(["check", "--message-format=json"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut error_summary = String::new();

        for line in stdout.lines() {
            if let Ok(msg) = serde_json::from_str::<Value>(line) {
                if msg["reason"] == "compiler-message" && msg["message"]["level"] == "error" {
                    let text = msg["message"]["rendered"]
                        .as_str()
                        .unwrap_or("Unknown compiler error");
                    error_summary.push_str(&format!("---\n{}\n", text));
                }
            }
        }

        if error_summary.is_empty() {
            Ok(ToolOutput {
                content: "All check passed. No compilation errors found.".to_string(),
                is_error: false,
            })
        } else {
            Ok(ToolOutput {
                content: format!(
                    "Build Failed with the following structured errors:\n\n{}",
                    error_summary
                ),
                is_error: true,
            })
        }
    }
}

pub struct DebuggerTool;

#[async_trait]
impl Tool for DebuggerTool {
    fn name(&self) -> &str {
        "DebuggerExecute"
    }

    fn description(&self) -> &str {
        "Run a command under a debugger (lldb/gdb) to capture backtraces and crash state."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command to run (e.g. 'cargo test')" },
                "debugger": { "type": "string", "enum": ["lldb", "gdb"], "default": "lldb" }
            },
            "required": ["command"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Moderate
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let cmd_str = input["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing command"))?;
        let debugger = input["debugger"].as_str().unwrap_or("lldb");

        let _script = if debugger == "lldb" {
            format!("run\nbt\nquit")
        } else {
            format!("run\nbacktrace\nquit")
        };

        let output = Command::new(debugger)
            .args(["--batch", "-o", "run", "-o", "bt", "-o", "quit", "--"])
            .args(cmd_str.split_whitespace())
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(ToolOutput {
            content: format!("Debugger Output:\n{}\n\nErrors:\n{}", stdout, stderr),
            is_error: !output.status.success(),
        })
    }
}

pub struct TraceAnalyzerTool;

#[async_trait]
impl Tool for TraceAnalyzerTool {
    fn name(&self) -> &str {
        "TraceAnalyzer"
    }

    fn description(&self) -> &str {
        "Parse a stack trace to identify the most likely source code culprit."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "trace": { "type": "string", "description": "The raw stack trace or debugger output" }
            },
            "required": ["trace"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let trace = input["trace"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing trace"))?;
        let mut findings = vec![];

        // Simple heuristic: look for lines with line numbers and file paths
        for line in trace.lines() {
            if line.contains(".rs:") || line.contains(" at ") {
                findings.push(line.trim().to_string());
            }
        }

        if findings.is_empty() {
            Ok(ToolOutput {
                content: "No obvious source-line culprits found in the trace.".to_string(),
                is_error: false,
            })
        } else {
            Ok(ToolOutput {
                content: format!("Potential Culprits:\n{}", findings.join("\n")),
                is_error: false,
            })
        }
    }
}
