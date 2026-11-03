use crate::swarm::file_lock::{write_atomic, FileLock};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub assigned_to: Option<String>,
    pub dependencies: Vec<String>,
    pub priority: u32,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Claimed { by: String },
    InProgress { by: String },
    Blocked { reason: String },
    Completed,
    Failed { error: String },
}

pub struct SharedTaskList {
    base_dir: PathBuf,
}

#[derive(Debug)]
pub struct TaskStats {
    pub total: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub failed: usize,
    pub blocked: usize,
}

impl SharedTaskList {
    pub fn new(team_name: &str) -> anyhow::Result<Self> {
        let root = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dreamswarm");
        Self::with_state_dir(&root, team_name)
    }

    /// Creates a task list under an explicit state directory.
    ///
    /// The daemon honours `--directory`/config state paths, so tasks must not
    /// always land in `~/.dreamswarm`.
    pub fn with_state_dir(state_dir: impl AsRef<Path>, team_name: &str) -> anyhow::Result<Self> {
        let base_dir = state_dir
            .as_ref()
            .join("teams")
            .join(Self::sanitize_name(team_name))
            .join("tasks");
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn create_task(
        &self,
        title: &str,
        description: &str,
        dependencies: Vec<String>,
        priority: u32,
    ) -> anyhow::Result<Task> {
        let task = Task {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            title: title.to_string(),
            description: description.to_string(),
            status: TaskStatus::Pending,
            assigned_to: None,
            dependencies,
            priority,
            result: None,
            error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };
        self.write_task(&task)?;
        Ok(task)
    }

    pub fn claim_task(&self, task_id: &str, worker_id: &str) -> anyhow::Result<Task> {
        self.try_claim_task(task_id, worker_id)
    }

    pub fn update_task(
        &self,
        task_id: &str,
        status: TaskStatus,
        result: Option<String>,
    ) -> anyhow::Result<Task> {
        let path = self.base_dir.join(format!("{}.json", task_id));
        let _lock = FileLock::for_target(&path)?;
        let mut task = self.read_task(task_id)?;
        task.status = status.clone();
        task.updated_at = Utc::now();
        if let Some(r) = result {
            task.result = Some(r);
        }
        if matches!(status, TaskStatus::Completed | TaskStatus::Failed { .. }) {
            task.completed_at = Some(Utc::now());
        }
        self.write_task(&task)?;
        Ok(task)
    }

    pub fn list_tasks(&self) -> anyhow::Result<Vec<Task>> {
        let mut tasks = Vec::new();
        if !self.base_dir.exists() {
            return Ok(tasks);
        }
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let content = std::fs::read_to_string(&path)?;
                if let Ok(task) = serde_json::from_str::<Task>(&content) {
                    tasks.push(task);
                }
            }
        }
        tasks.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.created_at.cmp(&b.created_at))
        });
        Ok(tasks)
    }

    pub fn all_complete(&self) -> anyhow::Result<bool> {
        let tasks = self.list_tasks()?;
        Ok(tasks
            .iter()
            .all(|t| matches!(t.status, TaskStatus::Completed | TaskStatus::Failed { .. })))
    }

    pub fn stats(&self) -> anyhow::Result<TaskStats> {
        let tasks = self.list_tasks()?;
        Ok(TaskStats {
            total: tasks.len(),
            pending: tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Pending)
                .count(),
            in_progress: tasks
                .iter()
                .filter(|t| {
                    matches!(
                        t.status,
                        TaskStatus::Claimed { .. } | TaskStatus::InProgress { .. }
                    )
                })
                .count(),
            completed: tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Completed)
                .count(),
            failed: tasks
                .iter()
                .filter(|t| matches!(t.status, TaskStatus::Failed { .. }))
                .count(),
            blocked: tasks
                .iter()
                .filter(|t| matches!(t.status, TaskStatus::Blocked { .. }))
                .count(),
        })
    }

    fn read_task(&self, task_id: &str) -> anyhow::Result<Task> {
        let path = self.base_dir.join(format!("{}.json", task_id));
        if !path.exists() {
            anyhow::bail!("Task not found: {}", task_id);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Writes a task atomically.
    ///
    /// The previous fixed `<id>.json.tmp` name meant two claimants could
    /// truncate each other's temp file and publish a half-written task.
    fn write_task(&self, task: &Task) -> anyhow::Result<()> {
        let path = self.base_dir.join(format!("{}.json", task.id));
        let content = serde_json::to_string_pretty(task)?;
        write_atomic(&path, content.as_bytes())
    }

    /// Claims a pending task under an exclusive per-task lock.
    ///
    /// `claim_task` is a read/check/write cycle; without the lock two workers
    /// polling at the same time could both observe `Pending` and both write a
    /// `Claimed` record, so the same task was handed out twice.
    pub fn try_claim_task(&self, task_id: &str, worker_id: &str) -> anyhow::Result<Task> {
        let path = self.base_dir.join(format!("{}.json", task_id));
        let _lock = FileLock::for_target(&path)?;

        let mut task = self.read_task(task_id)?;
        if !matches!(task.status, TaskStatus::Pending) {
            anyhow::bail!(
                "Task '{}' cannot be claimed (status: {:?})",
                task_id,
                task.status
            );
        }
        for dep_id in &task.dependencies {
            let dep = self.read_task(dep_id)?;
            if dep.status != TaskStatus::Completed {
                anyhow::bail!(
                    "Cannot claim task '{}': dependency '{}' not completed",
                    task_id,
                    dep_id
                );
            }
        }
        task.status = TaskStatus::Claimed {
            by: worker_id.to_string(),
        };
        task.assigned_to = Some(worker_id.to_string());
        task.updated_at = Utc::now();
        self.write_task(&task)?;
        Ok(task)
    }

    fn sanitize_name(name: &str) -> String {
        name.to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect()
    }
}

impl std::fmt::Display for TaskStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tasks: {} total | {} pending | {} in-progress | {} completed | {} failed | {} blocked",
            self.total, self.pending, self.in_progress, self.completed, self.failed, self.blocked
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn list(dir: &Path) -> SharedTaskList {
        SharedTaskList::with_state_dir(dir, "atomic-team").unwrap()
    }

    #[test]
    fn state_directory_is_honoured() {
        let dir = tempdir().unwrap();
        let tasks = list(dir.path());
        assert!(tasks.base_dir().starts_with(dir.path()));
    }

    #[test]
    fn task_round_trips_atomically() {
        let dir = tempdir().unwrap();
        let tasks = list(dir.path());
        let task = tasks
            .create_task("write docs", "detail", vec![], 1)
            .unwrap();
        let loaded = tasks.list_tasks().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, task.id);
    }

    #[test]
    fn one_of_two_claimants_wins() {
        let dir = tempdir().unwrap();
        let tasks = list(dir.path());
        let task = tasks.create_task("only once", "detail", vec![], 1).unwrap();

        let first = tasks.try_claim_task(&task.id, "worker-a");
        let second = tasks.try_claim_task(&task.id, "worker-b");

        assert!(first.is_ok());
        assert!(
            second.is_err(),
            "a claimed task must not be claimable again"
        );
        let stored = tasks.list_tasks().unwrap();
        assert_eq!(stored[0].assigned_to.as_deref(), Some("worker-a"));
    }

    #[test]
    fn concurrent_claims_never_duplicate() {
        let dir = tempdir().unwrap();
        let tasks = list(dir.path());
        let task = tasks.create_task("race", "detail", vec![], 1).unwrap();
        let task_id = task.id.clone();
        let base = tasks.base_dir().to_path_buf();
        let successes = std::sync::atomic::AtomicUsize::new(0);

        std::thread::scope(|scope| {
            for worker in 0..8 {
                let base = base.clone();
                let task_id = task_id.clone();
                let successes = &successes;
                scope.spawn(move || {
                    let list = SharedTaskList {
                        base_dir: base.clone(),
                    };
                    if list
                        .try_claim_task(&task_id, &format!("w{}", worker))
                        .is_ok()
                    {
                        successes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                });
            }
        });

        assert_eq!(
            successes.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "exactly one worker may claim the task"
        );
    }

    #[test]
    fn dependency_must_complete_first() {
        let dir = tempdir().unwrap();
        let tasks = list(dir.path());
        let first = tasks.create_task("first", "detail", vec![], 1).unwrap();
        let second = tasks
            .create_task("second", "detail", vec![first.id.clone()], 1)
            .unwrap();

        assert!(tasks.try_claim_task(&second.id, "worker").is_err());
        tasks
            .update_task(&first.id, TaskStatus::Completed, None)
            .unwrap();
        assert!(tasks.try_claim_task(&second.id, "worker").is_ok());
    }
}
