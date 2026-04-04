# Tool Authoring Guide 🛠️
Every capability is a permission-gated tool implementing the `Tool` trait.

## The Tool Trait
Implement `src/tools/my_tool.rs`:
```rust
use crate::tools::{Tool, ToolOutput};
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "MyTool" }
    fn description(&self) -> &str { "Clear, concise description for the LLM." }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "param": { "type": "string" }
            },
            "required": ["param"]
        })
    }
    fn risk_level(&self) -> RiskLevel { RiskLevel::Safe }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
       // Logic here
    }
}
```

## Step-by-Step Walkthrough
1. **Define the Schema**: Make parameters explicit and documented. The LLM uses these descriptions to know when and how to call your tool.
2. **Assign Risk Level**:
   - `Safe`: Read-only, no side effects.
   - `Moderate`: Writes to files, minor state changes.
   - `Dangerous`: Executes shell commands, large side effects.
   - `Critical`: Accesses credentials, network, or high-privilege resources.
3. **Register the Tool**: Add to `src/tools/mod.rs` and the `ToolRegistry`.
4. **Write Tests**: Success, error, and edge cases.

See [examples/custom_tool](../../examples/custom_tool/README.md) for a complete template.
