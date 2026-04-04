use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        let base_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dreamswarm")
            .join("teams")
            .join(Self::sanitize_name(team_name))
            .join("tasks");
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
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
        let mut task = self.read_task(task_id)?;
        match &task.status {
            TaskStatus::Pending => {
                // Check dependencies
                for dep_id in &task.dependencies {
                    let dep = self.read_task(dep_id)?;
                    if dep.status != TaskStatus::Completed {
                        anyhow::bail!("Cannot claim task '{}': dependency '{}' not completed", task_id, dep_id);
                    }
                }
                task.status = TaskStatus::Claimed { by: worker_id.to_string() };
                task.assigned_to = Some(worker_id.to_string());
                task.updated_at = Utc::now();
                self.write_task(&task)?;
                Ok(task)
            }
            TaskStatus::Claimed { by } => {
                anyhow::bail!("Task '{}' already claimed by '{}'", task_id, by);
            }
            other => {
                anyhow::bail!("Task '{}' cannot be claimed (status: {:?})", task_id, other);
            }
        }
    }

    pub fn update_task(
        &self,
        task_id: &str,
        status: TaskStatus,
        result: Option<String>,
    ) -> anyhow::Result<Task> {
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
            if path.extension().map_or(false, |e| e == "json") {
                let content = std::fs::read_to_string(&path)?;
                if let Ok(task) = serde_json::from_str::<Task>(&content) {
                    tasks.push(task);
                }
            }
        }
        tasks.sort_by(|a, b| {
            b.priority.cmp(&a.priority).then(a.created_at.cmp(&b.created_at))
        });
        Ok(tasks)
    }

    pub fn all_complete(&self) -> anyhow::Result<bool> {
        let tasks = self.list_tasks()?;
        Ok(tasks.iter().all(|t| matches!(t.status, TaskStatus::Completed | TaskStatus::Failed { .. })))
    }

    pub fn stats(&self) -> anyhow::Result<TaskStats> {
        let tasks = self.list_tasks()?;
        Ok(TaskStats {
            total: tasks.len(),
            pending: tasks.iter().filter(|t| t.status == TaskStatus::Pending).count(),
            in_progress: tasks.iter().filter(|t| matches!(t.status, TaskStatus::Claimed { .. } | TaskStatus::InProgress { .. })).count(),
            completed: tasks.iter().filter(|t| t.status == TaskStatus::Completed).count(),
            failed: tasks.iter().filter(|t| matches!(t.status, TaskStatus::Failed { .. })).count(),
            blocked: tasks.iter().filter(|t| matches!(t.status, TaskStatus::Blocked { .. })).count(),
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

    fn write_task(&self, task: &Task) -> anyhow::Result<()> {
        let path = self.base_dir.join(format!("{}.json", task.id));
        let temp_path = path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(task)?;
        std::fs::write(&temp_path, &content)?;
        std::fs::rename(&temp_path, &path)?;
        Ok(())
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
