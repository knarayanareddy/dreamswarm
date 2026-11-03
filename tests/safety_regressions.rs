use dreamswarm::db::Database;
use dreamswarm::query::engine::QueryEngine;
use dreamswarm::runtime::agent_loop::AgentRuntime;
use dreamswarm::runtime::config::{AppConfig, DeepSeekConfig};
use dreamswarm::runtime::permissions::{AgentMode, Permission, PermissionGate, RiskLevel};
use dreamswarm::runtime::session::{ContentBlock, MessageContent, Session};
use dreamswarm::tools::{bash_tool::BashTool, file_write::FileWriteTool, Tool, ToolRegistry};
use serde_json::json;

#[test]
fn real_signatures_do_not_allow_shell_chains_or_env_writes() {
    let config = AppConfig::default();
    let gate = PermissionGate::new(
        AgentMode::Default,
        &config.allow_patterns,
        &config.deny_patterns,
    );
    let bash = BashTool;
    let signature = bash.command_signature(&json!({"command": "ls; touch /tmp/unwanted"}));
    assert_eq!(
        gate.check("Bash", RiskLevel::Dangerous, &signature),
        Permission::Ask
    );
    assert_eq!(
        gate.check(
            "Bash",
            RiskLevel::Dangerous,
            &bash.command_signature(&json!({"command":"cargo test"}))
        ),
        Permission::Allow
    );
    let write = FileWriteTool;
    for path in [".env", "./.env.local", "/tmp/project/.env"] {
        let sig = write.command_signature(&json!({"path": path}));
        assert!(matches!(
            gate.check("FileWrite", RiskLevel::Moderate, &sig),
            Permission::Deny(_)
        ));
    }
}

#[test]
fn permission_modes_and_pattern_literals() {
    assert!("invalid".parse::<AgentMode>().is_err());
    assert_eq!(
        "bypass-permissions".parse::<AgentMode>().unwrap(),
        AgentMode::BypassPermissions
    );
    for mode in [AgentMode::ReadOnly, AgentMode::Plan] {
        let gate = PermissionGate::new(mode, &[], &[]);
        assert!(matches!(
            gate.check("Bash", RiskLevel::Dangerous, "ls"),
            Permission::Deny(_)
        ));
    }
    let gate = PermissionGate::new(AgentMode::AcceptEdits, &[], &["FileWrite(.env*)".into()]);
    assert!(matches!(
        gate.check("FileWrite", RiskLevel::Moderate, ".env"),
        Permission::Deny(_)
    ));
    assert_eq!(
        gate.check("FileWrite", RiskLevel::Moderate, "xenv"),
        Permission::Allow
    );
}

#[test]
fn config_roundtrip_does_not_save_secrets() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig {
        state_dir: tmp.path().to_path_buf(),
        model: "test-model".into(),
        deepseek_config: Some(DeepSeekConfig {
            api_key: "super-secret".into(),
            model: "deepseek".into(),
        }),
        ..Default::default()
    };
    config.save_to_toml().unwrap();
    let contents = std::fs::read_to_string(tmp.path().join("config.toml")).unwrap();
    assert!(!contents.contains("super-secret"));
    assert!(!serde_json::to_string(&config)
        .unwrap()
        .contains("super-secret"));
    let loaded = AppConfig::load_from_toml(tmp.path());
    assert_eq!(loaded.model, "test-model");
    assert!(loaded.deepseek_config.unwrap().api_key.is_empty());
}

#[tokio::test]
async fn tool_use_is_preserved_before_result() {
    let tmp = tempfile::tempdir().unwrap();
    let config = AppConfig {
        state_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let db = Database::new(&config.state_dir).unwrap();
    db.migrate().unwrap();
    let engine = QueryEngine::new("mock", &config.model, &config).unwrap();
    let mut tools = ToolRegistry::new();
    tools.register(Box::new(BashTool));
    let mut agent = AgentRuntime::new(Session::new(), engine, tools, config, db, None);
    agent
        .run_turn("run command", |_, _| async { false })
        .await
        .unwrap();
    assert!(
        matches!(&agent.session.messages[1].content, MessageContent::Mixed(blocks) if matches!(&blocks[0], ContentBlock::ToolUse { .. }))
    );
    assert!(
        matches!(&agent.session.messages[2].content, MessageContent::Mixed(blocks) if matches!(&blocks[0], ContentBlock::ToolResult { is_error: true, .. }))
    );
}
