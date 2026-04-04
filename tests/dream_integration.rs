// tests/dream_integration.rs
//! Integration tests for the autoDream memory consolidation engine.

use dreamswarm::dream::engine::DreamEngine;
use dreamswarm::dream::DreamConfig;
use tempfile::TempDir;

#[test]
fn test_dream_config_defaults() {
    let config = DreamConfig::default();
    // Verify sane defaults — ensures DreamConfig is properly initialized
    assert!(config.max_duration_secs > 0, "Dream cycle must have a timeout");
    assert!(config.max_tokens > 0, "Dream cycle must have a token budget");
    assert!(config.lookback_days > 0, "Dream cycle must look back at least 1 day");
}

#[test]
fn test_dream_engine_constructs() {
    let tmp = TempDir::new().unwrap();
    let config = DreamConfig::default();
    let working_dir = tmp.path().join("project");
    let daemon_state_dir = tmp.path().join("state");
    std::fs::create_dir_all(&working_dir).unwrap();
    std::fs::create_dir_all(&daemon_state_dir).unwrap();

    // Verify that DreamEngine can be constructed without panicking
    let _engine = DreamEngine::new(config, working_dir, daemon_state_dir);
}
