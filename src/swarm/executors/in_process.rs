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
            tmux_pane_id: None,
            remote_host: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let info_clone = worker_info.clone();
        let team_name = config.team_name.clone();
        let worker_id = config.id.clone();
        let state_dir = config.state_dir.clone();
        let join_handle = tokio::spawn(async move {
            tracing::info!("In-process worker '{}' started", worker_id);
            let mut mailbox = match crate::swarm::mailbox::Mailbox::new(state_dir, &team_name, &worker_id) {
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
                                            let _ = mailbox.send(&msg.from, MessageContent::TaskResult {
                                                task_id,
                                                result: format!("Worker {} acknowledged task: {}", worker_id, instructions)
                                            });
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
