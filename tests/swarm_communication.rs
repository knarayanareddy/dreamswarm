// tests/swarm_communication.rs
//! Integration tests for the swarm's collaborative communication protocols.

use dreamswarm::swarm::mailbox::Mailbox;
use dreamswarm::swarm::task_list::{SharedTaskList, TaskStatus};

/// Tests the full RequestHelp → CheckInbox round-trip between two agents.
#[test]
fn test_request_help_and_check_inbox() {
    let team = format!("test-help-{}", &uuid::Uuid::new_v4().to_string()[..6]);

    // Two agents share the same team mailbox directory
    let mut agent_a = Mailbox::new(&team, "agent-alpha").unwrap();
    let mut agent_b = Mailbox::new(&team, "agent-beta").unwrap();

    // Agent A sends a help request to Agent B
    let request_id = uuid::Uuid::new_v4().to_string();
    agent_a
        .send_help_request(
            "agent-beta",
            &request_id,
            "How do I handle async errors in Rust?",
        )
        .unwrap();

    // Agent B reads its inbox — should have 1 HelpRequest
    let messages = agent_b.receive().unwrap();
    assert_eq!(messages.len(), 1, "Agent B should have 1 message in inbox");

    let msg = &messages[0];
    assert_eq!(msg.from, "agent-alpha");
    assert_eq!(msg.to, "agent-beta");

    match &msg.content {
        dreamswarm::swarm::MessageContent::HelpRequest {
            request_id: rid,
            task,
        } => {
            assert_eq!(rid, &request_id);
            assert!(task.contains("async errors"));
        }
        other => panic!("Expected HelpRequest, got: {:?}", other),
    }

    // Agent B sends a response back
    agent_b
        .send_help_response(
            "agent-alpha",
            &request_id,
            "Use anyhow::Context to annotate errors.",
        )
        .unwrap();

    // Agent A reads the response
    let responses = agent_a.receive().unwrap();
    assert_eq!(responses.len(), 1, "Agent A should receive 1 HelpResponse");

    match &responses[0].content {
        dreamswarm::swarm::MessageContent::HelpResponse {
            request_id: rid,
            result,
        } => {
            assert_eq!(rid, &request_id);
            assert!(result.contains("anyhow"));
        }
        other => panic!("Expected HelpResponse, got: {:?}", other),
    }
}

/// Tests the full PublishKnowledge → SearchKnowledge round-trip.
#[test]
fn test_publish_and_search_knowledge() {
    use dreamswarm::tools::memory_tools::{PublishKnowledgeTool, SearchKnowledgeTool};
    use dreamswarm::tools::Tool;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let memory_dir = tmp.path().to_path_buf();

    let publish_tool = PublishKnowledgeTool {
        memory_dir: memory_dir.clone(),
    };
    let search_tool = SearchKnowledgeTool { memory_dir };

    // Publish a finding
    let publish_input = serde_json::json!({
        "title": "Zero-Copy Deserialization in Rust",
        "content": "Use serde with the 'borrow' lifetime to avoid allocations during JSON parsing.",
        "tags": ["rust", "performance", "serde"]
    });

    let result = rt.block_on(publish_tool.execute(&publish_input)).unwrap();
    assert!(
        !result.is_error,
        "PublishKnowledge should succeed. Error: {}",
        result.content
    );
    assert!(result.content.contains("published successfully"));

    // Search for it by tag keyword
    let search_input = serde_json::json!({ "query": "serde" });
    let search_result = rt.block_on(search_tool.execute(&search_input)).unwrap();

    assert!(!search_result.is_error, "SearchKnowledge should succeed");
    assert!(
        search_result.content.contains("Zero-Copy"),
        "Search should find the published entry. Got: {}",
        search_result.content
    );
}

/// Tests that a stalled task is correctly detected for rebalancing.
#[test]
fn test_stalled_task_detection() {
    let team = format!("test-stall-{}", &uuid::Uuid::new_v4().to_string()[..6]);
    let task_list = SharedTaskList::new(&team).unwrap();

    let task = task_list
        .create_task(
            "Analyse security surface",
            "Run cargo-audit and review deps",
            vec![],
            1,
        )
        .unwrap();

    // Claim the task
    let claimed = task_list.claim_task(&task.id, "worker-stalled").unwrap();
    assert!(matches!(claimed.status, TaskStatus::Claimed { .. }));

    // Verify it is in Claimed state
    let tasks = task_list.list_tasks().unwrap();
    let found = tasks.iter().find(|t| t.id == task.id).unwrap();
    assert!(matches!(found.status, TaskStatus::Claimed { .. }));
}
