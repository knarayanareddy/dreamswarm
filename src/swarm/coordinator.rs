use crate::prompts::roles::SwarmRole;
use crate::swarm::executors::{
    in_process::InProcessExecutor, ssh::SshExecutor, tmux::TmuxExecutor,
    worktree::WorktreeExecutor, TeammateExecutor, WorkerConfig,
};
use crate::swarm::mailbox::Mailbox;
use crate::swarm::result_merger::{MergeReport, ResultMerger};
use crate::swarm::task_list::{SharedTaskList, TaskStatus};
use crate::swarm::{
    MessageContent, SpawnStrategy, TeamConfig, TeamState, TeamStatus, WorkerInfo, WorkerStatus,
};
use chrono::Utc;
use std::path::PathBuf;

pub struct SwarmCoordinator {
    config: TeamConfig,
    task_list: SharedTaskList,
    mailbox: Mailbox,
    executor: Box<dyn TeammateExecutor>,
    workers: Vec<WorkerInfo>,
    state: TeamState,
    working_dir: String,
    state_dir: PathBuf,
}

impl SwarmCoordinator {
    pub fn new(config: TeamConfig, working_dir: &str, state_dir: PathBuf) -> anyhow::Result<Self> {
        let task_list = SharedTaskList::new(&config.team_name)?;
        let mailbox = Mailbox::new(state_dir.clone(), &config.team_name, "lead")?;
        let executor: Box<dyn TeammateExecutor> = match config.spawn_strategy {
            SpawnStrategy::InProcess => Box::new(InProcessExecutor::new()),
            SpawnStrategy::TmuxPane => {
                let session =
                    TmuxExecutor::current_session().unwrap_or_else(|| "dreamswarm".to_string());
                Box::new(TmuxExecutor::new(&session))
            }
            SpawnStrategy::GitWorktree => Box::new(WorktreeExecutor::new(
                PathBuf::from(working_dir),
                "dreamswarm",
            )?),
            SpawnStrategy::SSH => Box::new(SshExecutor::new(config.remote_hosts.clone())),
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
            state_dir,
        })
    }

    pub async fn spawn_worker(
        &mut self,
        name: &str,
        role: &str,
        instructions: &str,
    ) -> anyhow::Result<WorkerInfo> {
        if self.workers.len() >= self.config.max_workers {
            anyhow::bail!(
                "Maximum worker limit reached ({}/{})",
                self.workers.len(),
                self.config.max_workers
            );
        }

        // Auto-select a specialist role if the caller didn't specify one
        let swarm_role = if role == "worker" || role == "default" || role.is_empty() {
            Self::infer_role(instructions)
        } else {
            role.parse::<SwarmRole>()
                .unwrap_or(SwarmRole::GeneralWorker)
        };

        tracing::info!("Role-aware spawn: '{}' → {:?}", name, swarm_role);

        let worker_id = format!("w-{}", &uuid::Uuid::new_v4().to_string()[..6]);

        // Round-robin host selection for SSH
        let remote_host = if self.config.spawn_strategy == SpawnStrategy::SSH
            && !self.config.remote_hosts.is_empty()
        {
            let idx = self.workers.len() % self.config.remote_hosts.len();
            Some(self.config.remote_hosts[idx].clone())
        } else {
            None
        };

        let worker_config = WorkerConfig {
            id: worker_id.clone(),
            name: name.to_string(),
            role: role.to_string(),
            team_name: self.config.team_name.clone(),
            instructions: instructions.to_string(),
            model: self.config.worker_model.clone(),
            permission_mode: self.config.worker_mode.clone(),
            working_dir: self.working_dir.clone(),
            state_dir: self.state_dir.clone(),
            remote_host,
        };

        let mut worker = self.executor.spawn(&worker_config).await?;
        worker.instructions = instructions.to_string(); // Preserve for consensus audits
        self.workers.push(worker.clone());
        self.state.workers.push(worker.clone());
        self.state.updated_at = Utc::now();
        self.persist_state()?;

        tracing::info!(
            "Spawned worker '{}' (id: {}, role: {:?}, strategy: {:?})",
            name,
            worker_id,
            swarm_role,
            self.config.spawn_strategy
        );
        Ok(worker)
    }

    /// Infers the best swarm role for a worker based on task keyword heuristics.
    fn infer_role(instructions: &str) -> SwarmRole {
        let lower = instructions.to_lowercase();
        if lower.contains("security")
            || lower.contains("audit")
            || lower.contains("vulnerability")
            || lower.contains("cve")
            || lower.contains("exploit")
        {
            SwarmRole::SecurityResearcher
        } else if lower.contains("frontend")
            || lower.contains("react")
            || lower.contains("css")
            || lower.contains("ui")
            || lower.contains("html")
            || lower.contains("accessibility")
        {
            SwarmRole::FrontendEngineer
        } else if lower.contains("memory")
            || lower.contains("unsafe")
            || lower.contains("concurren")
            || lower.contains("performance")
            || lower.contains("throughput")
            || lower.contains("low-level")
        {
            SwarmRole::SystemsProgrammer
        } else {
            SwarmRole::GeneralWorker
        }
    }

    pub async fn assign_task(
        &mut self,
        worker_id: &str,
        title: &str,
        description: &str,
        dependencies: Vec<String>,
        priority: u32,
        autopilot_ctx: Option<(
            &crate::memory::MemorySystem,
            &crate::query::engine::QueryEngine,
        )>,
    ) -> anyhow::Result<()> {
        let mut final_description = description.to_string();

        if let Some((memory, qe)) = autopilot_ctx {
            if let Ok(themes) =
                crate::dream::autopilot::Autopilot::get_relevant_themes(description, memory, qe)
                    .await
            {
                final_description = crate::dream::autopilot::Autopilot::enrich_task_with_context(
                    description,
                    &themes,
                );
                let num_themes = themes.len();
                tracing::info!(
                    "Autopilot: Enriched task '{}' with {} L3 chapters",
                    title,
                    num_themes
                );
            }
        }

        let task = self
            .task_list
            .create_task(title, &final_description, dependencies, priority)?;
        self.task_list.claim_task(&task.id, worker_id)?;
        self.mailbox.send_task_assignment(
            worker_id,
            &task.id,
            &format!("{}\n\n{}", title, final_description),
        )?;
        tracing::info!(
            "Assigned task '{}' ({}) to worker '{}'",
            title,
            task.id,
            worker_id
        );
        Ok(())
    }

    pub async fn poll_updates(&mut self) -> anyhow::Result<Vec<crate::swarm::AgentMessage>> {
        let messages = self.mailbox.receive()?;
        for msg in &messages {
            match &msg.content {
                MessageContent::TaskResult { task_id, result } => {
                    tracing::info!("Worker '{}' completed task '{}'", msg.from, task_id);
                    let _ = self.task_list.update_task(
                        task_id,
                        TaskStatus::Completed,
                        Some(result.clone()),
                    );
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
        // Autonomous re-balancing: reclaim stalled tasks every poll cycle
        self.rebalance_load()?;
        Ok(messages)
    }

    /// Re-balances the task queue by reclaiming tasks that have been `Claimed`
    /// for more than 5 minutes without progressing to `InProgress`.
    fn rebalance_load(&mut self) -> anyhow::Result<()> {
        let stalled = self.check_stalled_tasks()?;
        if !stalled.is_empty() {
            tracing::warn!(
                "Re-balancer: found {} stalled task(s), returning to pending queue.",
                stalled.len()
            );
        }
        for task_id in stalled {
            let _ = self
                .task_list
                .update_task(&task_id, TaskStatus::Pending, None);
        }
        Ok(())
    }

    /// Returns the IDs of tasks that have been `Claimed` for > 5 minutes.
    fn check_stalled_tasks(&self) -> anyhow::Result<Vec<String>> {
        let stall_threshold = chrono::Duration::minutes(5);
        let now = Utc::now();
        let tasks = self.task_list.list_tasks()?;
        let stalled = tasks
            .into_iter()
            .filter(|t| {
                matches!(&t.status, TaskStatus::Claimed { .. })
                    && now.signed_duration_since(t.updated_at) > stall_threshold
            })
            .map(|t| t.id)
            .collect();
        Ok(stalled)
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
            output.push_str(&format!(
                "{} [{}] {} — {:?}\n",
                status_icon, task.id, task.title, task.status
            ));
        }

        output.push_str(&format!("\n## Workers ({})\n", self.workers.len()));
        for worker in &self.workers {
            output.push_str(&format!(
                "🐝 {} ({}) — {:?}\n",
                worker.name, worker.id, worker.status
            ));
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
        merger
            .merge(&self.workers, &self.config.merge_strategy, &target_branch)
            .await
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
        let _ = Mailbox::cleanup_team(self.state_dir.clone(), &self.config.team_name);
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
        let state_dir = self.state_dir.join("teams").join(&self.config.team_name);
        std::fs::create_dir_all(&state_dir)?;
        let state_path = state_dir.join("state.json");
        let content = serde_json::to_string_pretty(&self.state)?;
        std::fs::write(&state_path, content)?;
        Ok(())
    }

    pub fn task_list(&self) -> &SharedTaskList {
        &self.task_list
    }

    /// Creates a full cognitive snapshot of the current swarm.
    pub fn checkpoint(&self) -> anyhow::Result<PathBuf> {
        let snapshot_dir = self.state_dir.join("snapshots");
        std::fs::create_dir_all(&snapshot_dir)?;
        let path = snapshot_dir.join(format!("snapshot_{}.json", Utc::now().timestamp()));
        let content = serde_json::to_string_pretty(&self.state)?;
        std::fs::write(&path, content)?;
        tracing::info!(
            "Halt & Resume: Created cognitive snapshot at {}",
            path.display()
        );
        Ok(path)
    }

    /// Attempts to resume a swarm from a state snapshot.
    pub fn resume(state_dir: PathBuf, snapshot_path: PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(&snapshot_path)?;
        let state: TeamState = serde_json::from_str(&content)?;

        let mut coordinator = Self::new(
            state.config.clone(),
            state
                .workers
                .first()
                .map_or(".", |w| w.worktree_path.as_deref().unwrap_or(".")),
            state_dir,
        )?;

        coordinator.state = state.clone();
        coordinator.workers = state.workers;

        tracing::info!(
            "Halt & Resume: Successfully restored swarm '{}' from snapshot",
            state.config.team_name
        );
        Ok(coordinator)
    }
}
