# Example: Building a Custom Tool
This example shows how to create a custom DreamSwarm tool from scratch.

## What We're Building
A `GitDiffTool` that shows the current uncommitted changes in the working directory. This is a safe, read-only tool—good for a first contribution.

## Step 1: Create the Tool File
Create `src/tools/git_diff.rs`:

```rust
use crate::tools::{Tool, ToolOutput};
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;

/// Shows uncommitted changes in the git working directory.
/// 
/// This is a read-only tool—it never modifies the repository.
/// Useful for reviewing what's changed before committing or
/// understanding the current state of the codebase.
pub struct GitDiffTool;

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str { "GitDiff" }

    fn description(&self) -> &str {
        "Show the current uncommitted changes in the git repository. \
        Returns a unified diff of all modified files. Use this to review \
        what has changed before committing, or to understand the current \
        state of the working directory."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "staged_only": {
                    "type": "boolean",
                    "description": "If true, show only staged changes (default: false)"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let staged = input.get("staged_only").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut cmd = Command::new("git");
        cmd.arg("diff");
        if staged {
            cmd.arg("--staged");
        }

        let output = cmd.output().await?;
        Ok(ToolOutput {
            content: String::from_utf8_lossy(&output.stdout).to_string(),
            is_error: !output.status.success(),
        })
    }
}
```

## Step 2: Register the Tool
Add `pub mod git_diff;` to `src/tools/mod.rs` and register it in the `ToolRegistry`.

## Step 3: Test
Run `cargo test git_diff` to verify your implementation.
