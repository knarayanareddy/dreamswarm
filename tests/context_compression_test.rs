// tests/context_compression_test.rs
//! Integration tests for the 3-stage context compression pipeline.

use dreamswarm::context::cache_tracker::CacheTracker;
use dreamswarm::context::micro_compact::MicroCompact;
use dreamswarm::context::token_counter::TokenCounter;

#[test]
fn test_micro_compact_trims_large_outputs() {
    let compactor = MicroCompact::new();
    let large_output = "output line\n".repeat(500);
    let mut messages = vec![
        serde_json::json!({
            "role": "assistant",
            "content": [{ "type": "tool_use", "id": "tool-1", "name": "Bash", "input": {"command": "ls -la"} }]
        }),
        serde_json::json!({
            "role": "user",
            "content": [{ "type": "tool_result", "tool_use_id": "tool-1", "content": large_output, "is_error": false }]
        }),
    ];

    let before = TokenCounter::estimate_messages(&messages);
    let saved = compactor.compact(&mut messages, 10);
    let after = TokenCounter::estimate_messages(&messages);

    assert!(saved > 0);
    assert!(after < before);
}

#[test]
fn test_cache_tracker_sticky_flags() {
    let mut tracker = CacheTracker::new();

    tracker.activate_flag("memory_system");
    assert!(tracker
        .active_flags()
        .contains(&"memory_system".to_string()));

    // Sticky: once activated, the flag cannot be deactivated
    assert!(!tracker.deactivate_flag("memory_system"));
    assert!(tracker
        .active_flags()
        .contains(&"memory_system".to_string()));
}

#[test]
fn test_token_counter_estimates() {
    // ~44 chars / 4 ≈ 11 tokens
    let text = "The quick brown fox jumps over the lazy dog.";
    let tokens = TokenCounter::estimate(text);
    assert!((8..=15).contains(&tokens));

    // Empty string should be 0
    assert_eq!(TokenCounter::estimate(""), 0);
}
