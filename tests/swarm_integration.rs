// tests/swarm_integration.rs
//! Integration tests for the multi-agent swarm orchestration system.

use dreamswarm::swarm::task_list::{SharedTaskList, TaskStatus};

#[test]
fn test_task_list_roundtrip() {
    // SharedTaskList::new takes a team name string and stores under ~/.dreamswarm/
    let team_name = format!("test-team-{}", uuid::Uuid::new_v4().to_string()[..6].to_string());
    let task_list = SharedTaskList::new(&team_name).unwrap();

    // Create a task
    let task = task_list
        .create_task("Refactor the query engine", "Move providers to separate crate", vec![], 1)
        .unwrap();

    assert_eq!(task.title, "Refactor the query engine");
    assert!(matches!(task.status, TaskStatus::Pending));

    // List tasks — should have 1
    let tasks = task_list.list_tasks().unwrap();
    assert_eq!(tasks.len(), 1);

    // Claim the task (transitions Pending -> Claimed)
    let claimed = task_list.claim_task(&task.id, "worker-1").unwrap();
    assert!(matches!(claimed.status, TaskStatus::Claimed { .. }));

    // Mark it in-progress
    let in_progress = task_list
        .update_task(&task.id, TaskStatus::InProgress { by: "worker-1".to_string() }, None)
        .unwrap();
    assert!(matches!(in_progress.status, TaskStatus::InProgress { .. }));

    // Complete it
    task_list
        .update_task(&task.id, TaskStatus::Completed, Some("Refactor done".to_string()))
        .unwrap();

    // all_complete should now be true
    assert!(task_list.all_complete().unwrap());

    // Stats sanity check
    let stats = task_list.stats().unwrap();
    assert_eq!(stats.total, 1);
    assert_eq!(stats.completed, 1);
    assert_eq!(stats.pending, 0);
}
