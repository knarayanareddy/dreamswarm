// tests/memory_integration.rs
//! Integration tests for the 3-layer memory system.

use dreamswarm::memory::topics::Confidence;
use dreamswarm::memory::MemorySystem;
use tempfile::TempDir;

#[test]
fn test_full_memory_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let memory = MemorySystem::new(tmp.path().to_path_buf()).unwrap();

    // 1. Store a memory via the writer (enforces write discipline)
    let result = memory
        .writer
        .store(
            "Auth",
            "oauth",
            "The auth service uses OAuth2 with PKCE.\n\
            Code expiry is 60 seconds, and refresh token rotation is secretly enabled.",
            Some("src/auth/oauth_handler.rs:142"),
            Confidence::Verified,
        )
        .unwrap();
    assert!(result.stored);

    // 2. Store another memory
    memory
        .writer
        .store(
            "Database",
            "migrations",
            "Always run migrations before deploying the API. The migration tool \
            is custom — use `cargo run --bin migrate` not a framework tool.",
            Some("ops/deploy.md"),
            Confidence::Verified,
        )
        .unwrap();

    // 3. Index should have pointers, NOT full content
    let index = memory.index.load_raw().unwrap();
    assert!(index.contains("auth/oauth.md"));
    assert!(index.contains("database/migrations.md"));
    assert!(!index.contains("refresh token rotation"));

    // 4. Topic file should have full content
    let topic = memory.topics.read("auth/oauth.md").unwrap().unwrap();
    assert!(topic.contains("PKCE"));
    assert!(topic.contains("verified"));

    // 5. Search should work
    let results = memory.search.search("PKCE refresh", 5).unwrap();
    assert!(!results.is_empty());
    assert!(results[0].topic_path.contains("oauth"));

    // 6. Load for context injection
    let ctx = memory
        .loader
        .load_for_turn(Some("How does auth work?"))
        .unwrap();
    assert!(ctx.index_tokens > 0);
    assert!(!ctx.topics.is_empty());

    // 7. Irrelevant query should only load index
    let ctx2 = memory.loader.load_for_turn(Some("pizza recipe")).unwrap();
    assert!(ctx2.index_tokens > 0);
    assert!(ctx2.topics.is_empty());

    // 8. Derivable (pure code) content should NOT be stored
    let code_result = memory
        .writer
        .store(
            "Code",
            "main",
            "fn main() {\n    let x = 42;\n    println!(\"{}\", x);\n}\n",
            None,
            Confidence::Observed,
        )
        .unwrap();
    assert!(!code_result.stored);

    // 9. Archive a transcript turn
    memory
        .transcripts
        .archive_turn(
            "session-abc",
            1,
            "user",
            "Fix the OAuth bug",
            &["FileRead".to_string(), "FileWrite".to_string()],
            500,
        )
        .unwrap();
    let transcripts = memory.transcripts.list_transcripts().unwrap();
    assert!(!transcripts.is_empty());
}

#[test]
fn test_memory_index_capacity_limit() {
    let tmp = TempDir::new().unwrap();
    let memory = MemorySystem::new(tmp.path().to_path_buf()).unwrap();

    // Fill the index with many entries
    for i in 0..210 {
        let _ = memory.writer.store(
            &format!("Topic{}", i / 10),
            &format!("subtopic{}", i),
            &format!(
                "This is test entry number {} with enough content to be stored",
                i
            ),
            None,
            Confidence::Observed,
        );
    }

    // Index should stay within bounds
    let index = memory.index.load_raw().unwrap();
    assert!(index.len() < 100_000); // sanity size check
}
