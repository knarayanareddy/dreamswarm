use super::{TeammateExecutor, WorkerConfig};
use crate::swarm::{MessageContent, SpawnStrategy, WorkerInfo, WorkerStatus};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

pub struct InProcessExecutor {
    workers: Arc<RwLock<HashMap<String, WorkerHandle>>>,
    app_config: crate::runtime::config::AppConfig,
}

struct WorkerHandle {
    join_handle: tokio::task::JoinHandle<()>,
    cancel_token: CancellationToken,
    _info: WorkerInfo,
}

impl Default for InProcessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl InProcessExecutor {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            app_config: crate::runtime::config::AppConfig::default(),
        }
    }

    pub fn with_config(app_config: crate::runtime::config::AppConfig) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            app_config,
        }
    }
}

#[async_trait]
impl TeammateExecutor for InProcessExecutor {
    async fn spawn(&self, config: &WorkerConfig) -> anyhow::Result<WorkerInfo> {
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();

        let worker_info = WorkerInfo {
            id: config.id.clone(),
            name: config.name.clone(),
            role: config.role.clone(),
            status: WorkerStatus::Active,
            spawn_type: SpawnStrategy::InProcess,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            worktree_path: None,
            branch_name: None,
            instructions: String::new(),
            tmux_pane_id: None,
            remote_host: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let info_clone = worker_info.clone();
        let team_name = config.team_name.clone();
        let worker_id = config.id.clone();
        let state_dir = config.state_dir.clone();
        let mut app_config = self.app_config.clone();
        app_config.state_dir = state_dir.clone();
        app_config.working_dir = config.working_dir.clone().into();
        use crate::runtime::permissions::AgentMode;
        let requested: AgentMode = config
            .permission_mode
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid worker permission mode"))?;
        // A team request may restrict the operator's mode, but never elevate it.
        app_config.permission_mode = match (app_config.permission_mode, requested) {
            (AgentMode::Plan, _) | (_, AgentMode::Plan) => AgentMode::Plan,
            (AgentMode::ReadOnly, _) | (_, AgentMode::ReadOnly) => AgentMode::ReadOnly,
            (AgentMode::Default, AgentMode::AcceptEdits | AgentMode::BypassPermissions) => {
                AgentMode::Default
            }
            (AgentMode::AcceptEdits, AgentMode::BypassPermissions) => AgentMode::AcceptEdits,
            (_, mode) => mode,
        };
        if let Some(model) = &config.model {
            app_config.model = model.clone();
        }
        // Process-wide current directory cannot safely be changed by a worker.
        if std::fs::canonicalize(&app_config.working_dir)?
            != std::fs::canonicalize(std::env::current_dir()?)?
        {
            anyhow::bail!("In-process workers require the process working directory; use an isolated executor for another directory");
        }
        let worker_instructions = config.instructions.clone();
        let join_handle = tokio::spawn(async move {
            tracing::info!("In-process worker '{}' started", worker_id);
            let mut mailbox =
                match crate::swarm::mailbox::Mailbox::new(state_dir, &team_name, &worker_id) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Worker {} mailbox failed: {}", worker_id, e);
                        return;
                    }
                };

            loop {
                tokio::select! {
                    _ = token_clone.cancelled() => {
                        tracing::info!("Worker '{}' cancelled", worker_id);
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                        match mailbox.receive() {
                            Ok(messages) => {
                                for msg in messages {
                                    match msg.content {
                                        MessageContent::ShutdownRequest => {
                                            tracing::info!("Worker '{}' shutdown", worker_id);
                                            let _ = mailbox.send(&msg.from, MessageContent::ShutdownAck);
                                            return;
                                        }
                                        MessageContent::TaskAssignment { task_id, instructions } => {
                                            tracing::info!("Worker '{}' task: {}", worker_id, task_id);
                                            let result = tokio::select! {
                                                _ = token_clone.cancelled() => return,
                                                result = run_task(&app_config, &worker_instructions, &instructions) => result,
                                            };
                                            let content = match result {
                                                Ok(result) => MessageContent::TaskResult { task_id, result },
                                                Err(error) => MessageContent::TaskFailed { task_id, error: error.to_string() },
                                            };
                                            if let Err(error) = mailbox.send(&msg.from, content) {
                                                tracing::error!("Worker {} could not report task result: {}", worker_id, error);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Worker {} mailbox error: {}", worker_id, e);
                            }
                        }
                    }
                }
            }
        });

        let handle = WorkerHandle {
            join_handle,
            cancel_token,
            _info: info_clone,
        };

        self.workers.write().await.insert(config.id.clone(), handle);
        Ok(worker_info)
    }

    async fn is_alive(&self, worker: &WorkerInfo) -> bool {
        let workers = self.workers.read().await;
        workers
            .get(&worker.id)
            .map(|h| !h.join_handle.is_finished())
            .unwrap_or(false)
    }

    async fn send_input(&self, _worker: &WorkerInfo, _input: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn shutdown(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        let workers = self.workers.read().await;
        if let Some(handle) = workers.get(&worker.id) {
            handle.cancel_token.cancel();
        }
        Ok(())
    }

    async fn force_kill(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        let mut workers = self.workers.write().await;
        if let Some(handle) = workers.remove(&worker.id) {
            handle.cancel_token.cancel();
            handle.join_handle.abort();
            let _ = handle.join_handle.await;
        }
        Ok(())
    }

    async fn cleanup(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        self.force_kill(worker).await
    }

    fn strategy(&self) -> SpawnStrategy {
        SpawnStrategy::InProcess
    }
}

/// Execute a task through the same runtime and permission gate as an interactive agent.
async fn run_task(
    config: &crate::runtime::config::AppConfig,
    role: &str,
    instructions: &str,
) -> anyhow::Result<String> {
    use crate::runtime::{
        agent_loop::{AgentRuntime, StopReason},
        session::Session,
    };
    let memory = Arc::new(RwLock::new(crate::memory::MemorySystem::new(
        config.state_dir.join("memory"),
    )?));
    let db = crate::db::Database::new(&config.state_dir)?;
    db.migrate()?;
    let engine = crate::query::engine::QueryEngine::new(&config.provider, &config.model, config)?;
    let tools = crate::tools::ToolRegistry::default_phase1(memory, None);
    let mut runtime = AgentRuntime::new(Session::new(), engine, tools, config.clone(), db, None);
    let prompt = format!(
        "Worker instructions: {}\n\nAssigned task: {}",
        role, instructions
    );
    let result = runtime.run_turn(&prompt, |_, _| async { false }).await?;
    match result.stop_reason {
        StopReason::EndTurn => Ok(result.final_text),
        _ => anyhow::bail!("Worker stopped before finishing the task"),
    }
}
