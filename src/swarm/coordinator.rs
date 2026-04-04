use chrono::Utc;
use std::path::PathBuf;
use crate::swarm::{
    MessageContent, SpawnStrategy, TeamConfig, TeamState, TeamStatus, WorkerInfo, WorkerStatus,
};
use crate::swarm::executors::{TeammateExecutor, WorkerConfig, in_process::InProcessExecutor, tmux::TmuxExecutor, worktree::WorktreeExecutor};
use crate::swarm::mailbox::Mailbox;
use crate::swarm::result_merger::{ResultMerger, MergeReport};
use crate::swarm::task_list::{SharedTaskList, TaskStatus};

pub struct SwarmCoordinator {
    config: TeamConfig,
    task_list: SharedTaskList,
    mailbox: Mailbox,
    executor: Box<dyn TeammateExecutor>,
    workers: Vec<WorkerInfo>,
    state: TeamState,
    working_dir: String,
}

impl SwarmCoordinator {
    pub fn new(config: TeamConfig, working_dir: &str) -> anyhow::Result<Self> {
        let task_list = SharedTaskList::new(&config.team_name)?;
        let mailbox = Mailbox::new(&config.team_name, "lead")?;
        let executor: Box<dyn TeammateExecutor> = match config.spawn_strategy {
            SpawnStrategy::InProcess => Box::new(InProcessExecutor::new()),
            SpawnStrategy::TmuxPane => {
                let session = TmuxExecutor::current_session().unwrap_or_else(|| "dreamswarm".to_string());
                Box::new(TmuxExecutor::new(&session))
            }
            SpawnStrategy::GitWorktree => {
                Box::new(WorktreeExecutor::new(PathBuf::from(working_dir), "dreamswarm")?)
            }
        };

        let state = TeamState {
            config: config.clone(),
            workers: vec![],
            status: TeamStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        Ok(Self {
            config,
            task_list,
            mailbox,
            executor,
            workers: vec![],
            state,
            working_dir: working_dir.to_string(),
        })
    }

    pub async fn spawn_worker(&mut self, name: &str, role: &str, instructions: &str) -> anyhow::Result<WorkerInfo> {
        if self.workers.len() >= self.config.max_workers {
            anyhow::bail!("Maximum worker limit reached ({}/{})", self.workers.len(), self.config.max_workers);
        }

        let worker_id = format!("w-{}", &uuid::Uuid::new_v4().to_string()[..6]);
        let worker_config = WorkerConfig {
            id: worker_id.clone(),
            name: name.to_string(),
            role: role.to_string(),
            team_name: self.config.team_name.clone(),
            instructions: instructions.to_string(),
            model: self.config.worker_model.clone(),
            permission_mode: self.config.worker_mode.clone(),
            working_dir: self.working_dir.clone(),
        };

        let worker = self.executor.spawn(&worker_config).await?;
        self.workers.push(worker.clone());
        self.state.workers.push(worker.clone());
        self.state.updated_at = Utc::now();
        self.persist_state()?;

        tracing::info!("Spawned worker '{}' (id: {}, strategy: {:?})", name, worker_id, self.config.spawn_strategy);
        Ok(worker)
    }

    pub async fn assign_task(
        &mut self,
        worker_id: &str,
        title: &str,
        description: &str,
        dependencies: Vec<String>,
        priority: u32,
    ) -> anyhow::Result<()> {
        let task = self.task_list.create_task(title, description, dependencies, priority)?;
        self.task_list.claim_task(&task.id, worker_id)?;
        self.mailbox.send_task_assignment(worker_id, &task.id, &format!("{}\n\n{}", title, description))?;
        tracing::info!("Assigned task '{}' ({}) to worker '{}'", title, task.id, worker_id);
        Ok(())
    }

    pub async fn poll_updates(&mut self) -> anyhow::Result<Vec<crate::swarm::AgentMessage>> {
        let messages = self.mailbox.receive()?;
        for msg in &messages {
            match &msg.content {
                MessageContent::TaskResult { task_id, result } => {
                    tracing::info!("Worker '{}' completed task '{}'", msg.from, task_id);
                    let _ = self.task_list.update_task(task_id, TaskStatus::Completed, Some(result.clone()));
                }
                MessageContent::StatusUpdate { status } => {
                    self.update_worker_status(&msg.from, status.clone());
                }
                MessageContent::ShutdownAck => {
                    self.update_worker_status(&msg.from, WorkerStatus::Completed);
                }
                _ => {}
            }
        }
        Ok(messages)
    }

    pub fn task_status(&self) -> anyhow::Result<String> {
        let stats = self.task_list.stats()?;
        let tasks = self.task_list.list_tasks()?;
        let mut output = format!("## Team: {}\n\n{}\n\n", self.config.team_name, stats);

        for task in &tasks {
            let status_icon = match &task.status {
                TaskStatus::Pending => "⬜",
                TaskStatus::Claimed { .. } => "🔵",
                TaskStatus::InProgress { .. } => "🟡",
                TaskStatus::Completed => "✅",
                TaskStatus::Failed { .. } => "❌",
                TaskStatus::Blocked { .. } => "🚫",
            };
            output.push_str(&format!("{} [{}] {} — {:?}\n", status_icon, task.id, task.title, task.status));
        }

        output.push_str(&format!("\n## Workers ({})\n", self.workers.len()));
        for worker in &self.workers {
            output.push_str(&format!("🐝 {} ({}) — {:?}\n", worker.name, worker.id, worker.status));
        }
        Ok(output)
    }

    pub fn is_complete(&self) -> anyhow::Result<bool> {
        self.task_list.all_complete()
    }

    pub async fn merge_results(&self) -> anyhow::Result<MergeReport> {
        let merger = ResultMerger::new(&self.working_dir);
        let output = tokio::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&self.working_dir)
            .output()
            .await?;
        let target_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        merger.merge(&self.workers, &self.config.merge_strategy, &target_branch).await
    }

    pub async fn shutdown_team(&mut self) -> anyhow::Result<()> {
        tracing::info!("Shutting down team '{}'", self.config.team_name);
        for worker in &self.workers {
            if worker.status != WorkerStatus::Completed {
                let _ = self.mailbox.send_shutdown(&worker.id);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        for worker in &self.workers {
            if self.executor.is_alive(worker).await {
                tracing::warn!("Force-killing worker '{}'", worker.name);
                let _ = self.executor.force_kill(worker).await;
            }
            let _ = self.executor.cleanup(worker).await;
        }
        let _ = Mailbox::cleanup_team(&self.config.team_name);
        self.state.status = TeamStatus::Completed;
        self.state.updated_at = Utc::now();
        self.persist_state()?;
        Ok(())
    }

    fn update_worker_status(&mut self, worker_id: &str, status: WorkerStatus) {
        for worker in &mut self.workers {
            if worker.id == worker_id {
                worker.status = status.clone();
                worker.updated_at = Utc::now();
            }
        }
        for worker in &mut self.state.workers {
            if worker.id == worker_id {
                worker.status = status.clone();
                worker.updated_at = Utc::now();
            }
        }
    }

    fn persist_state(&self) -> anyhow::Result<()> {
        let state_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dreamswarm")
            .join("teams")
            .join(&self.config.team_name);
        std::fs::create_dir_all(&state_dir)?;
        let state_path = state_dir.join("state.json");
        let content = serde_json::to_string_pretty(&self.state)?;
        std::fs::write(&state_path, content)?;
        Ok(())
    }

    pub fn task_list(&self) -> &SharedTaskList {
        &self.task_list
    }
}
