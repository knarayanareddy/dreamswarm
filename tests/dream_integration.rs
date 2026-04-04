// tests/dream_integration.rs
//! Integration tests for the autoDream memory consolidation engine.

use dreamswarm::dream::DreamEngine;
use dreamswarm::memory::MemorySystem;
use tempfile::TempDir;

#[tokio::test]
async fn test_dream_engine_runs_cycle() {
    let tmp = TempDir::new().unwrap();
    let memory = MemorySystem::new(tmp.path().to_path_buf()).unwrap();

    // Seed some memory for the engine to consolidate
    use dreamswarm::memory::topics::Confidence;
    memory
        .writer
        .store(
            "Test",
            "fact",
            "The test suite passes on stable Rust 1.77.0 and above.",
            None,
            Confidence::Verified,
        )
        .unwrap();

    let engine = DreamEngine::new(memory, tmp.path().to_path_buf());

    // In a real environment this would call the LLM; in tests it runs without a key
    // and should complete the cycle without panicking.
    let result = engine.run_cycle_dry().await;
    assert!(result.is_ok(), "Dream cycle should complete without error");
}
