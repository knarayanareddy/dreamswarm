// tests/swarm_integration.rs
//! Integration tests for the multi-agent swarm orchestration system.

use dreamswarm::swarm::{SharedTaskList, TaskStatus};
use tempfile::TempDir;

#[test]
fn test_task_list_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let task_list = SharedTaskList::new(tmp.path().join("tasks.json")).unwrap();

    // Add a task
    let id = task_list.add("Refactor the query engine").unwrap();
    let tasks = task_list.list_all().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].description, "Refactor the query engine");
    assert!(matches!(tasks[0].status, TaskStatus::Pending));

    // Update status
    task_list.update_status(&id, TaskStatus::InProgress).unwrap();
    let tasks = task_list.list_all().unwrap();
    assert!(matches!(tasks[0].status, TaskStatus::InProgress));

    // Complete
    task_list.update_status(&id, TaskStatus::Done).unwrap();
    let pending = task_list.list_pending().unwrap();
    assert!(pending.is_empty());
}
