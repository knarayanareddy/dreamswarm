use crate::daemon::daily_log::{DailyLog, LogEntry, LogEntryKind};
use crate::dream::analyzer::DreamAnalyzer;
use crate::dream::collector::ObservationCollector;
use crate::dream::planner::DreamPlanner;
use crate::dream::pruner::MemoryPruner;
use crate::dream::report::DreamReporter;
use crate::dream::sandbox::DreamSandbox;
use crate::dream::{DreamConfig, DreamReport, MemoryOperation, OperationKind, PruneReason};
use crate::memory::topics::Confidence;
use crate::memory::MemorySystem;
use crate::query::engine::QueryEngine;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub struct DreamEngine {
    config: DreamConfig,
    working_dir: PathBuf,
    daemon_state_dir: PathBuf,
}

impl DreamEngine {
    pub fn new(config: DreamConfig, working_dir: PathBuf, daemon_state_dir: PathBuf) -> Self {
        Self {
            config,
            working_dir,
            daemon_state_dir,
        }
    }

    pub async fn dream(
        &self,
        memory: &MemorySystem,
        query_engine: &QueryEngine,
    ) -> anyhow::Result<DreamReport> {
        let started_at = Utc::now();
        let mut errors: Vec<String> = Vec::new();
        tracing::info!("autoDream cycle starting...");
        let memory_before_hash = self.hash_memory_state(memory)?;

        let collector =
            ObservationCollector::new(self.config.clone(), self.daemon_state_dir.clone());
        let observations = collector.collect(memory)?;
        if observations.is_empty() {
            tracing::info!("No observations - dream cycle skipped");
            return Ok(self.empty_report(started_at, &memory_before_hash));
        }

        let pruner = MemoryPruner::new(
            self.working_dir.clone(),
            self.config.prune_confidence_threshold,
        );
        let prune_ops = match pruner.analyze(memory) {
            Ok(ops) => ops,
            Err(e) => {
                errors.push(format!("Pruner error: {}", e));
                vec![]
            }
        };

        let mut sandbox = DreamSandbox::new(self.config.max_tokens, self.config.max_cost_usd);
        let snapshot_raw = memory.index.load_raw()?;
        let llm_ops = match tokio::time::timeout(
            std::time::Duration::from_secs(self.config.max_duration_secs),
            DreamAnalyzer::analyze(&observations, &snapshot_raw, &mut sandbox, query_engine),
        )
        .await
        {
            Ok(Ok(ops)) => ops,
            Ok(Err(e)) => {
                errors.push(format!("Analyzer error: {}", e));
                vec![]
            }
            Err(_) => {
                errors.push("Analyzer timeout".to_string());
                vec![]
            }
        };

        let mut all_ops = prune_ops;
        all_ops.extend(llm_ops);
        let planned_count = all_ops.len();
        let planned = DreamPlanner::plan(all_ops, &self.config);

        let snapshot = self.snapshot_memory(memory)?;
        let mut applied_count = 0;
        let mut merged_count = 0;
        let mut created_count = 0;
        let mut pruned_count = 0;
        let mut confirmed_count = 0;
        let mut contradictions_count = 0;

        for op in &planned {
            match self.apply_operation(op, memory) {
                Ok(applied) => {
                    if applied {
                        applied_count += 1;
                        match &op.kind {
                            OperationKind::Merge { .. } => merged_count += 1,
                            OperationKind::Create => created_count += 1,
                            OperationKind::Prune { reason } => {
                                pruned_count += 1;
                                if *reason == PruneReason::Contradicted {
                                    contradictions_count += 1;
                                }
                            }
                            OperationKind::Confirm { .. } => confirmed_count += 1,
                            OperationKind::Update { .. } => {}
                            OperationKind::Conflict { .. } => contradictions_count += 1,
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!(
                        "Failed to apply {:?} on {}/{}: {}",
                        op.kind, op.topic, op.subtopic, e
                    ));
                    if errors.len() > 10 {
                        self.rollback_memory(memory, &snapshot)?;
                        errors.push("Rolled back due to excessive errors".to_string());
                        break;
                    }
                }
            }
        }

        let completed_at = Utc::now();
        let memory_after_hash = self.hash_memory_state(memory)?;
        let (tokens_used, cost_used) = sandbox.usage_stats();
        let report = DreamReport {
            started_at,
            completed_at,
            duration_secs: (completed_at - started_at).num_seconds() as u64,
            observations_collected: observations.len(),
            operations_planned: planned_count,
            operations_applied: applied_count,
            entries_merged: merged_count,
            entries_created: created_count,
            entries_pruned: pruned_count,
            entries_confirmed: confirmed_count,
            contradictions_resolved: contradictions_count,
            tokens_consumed: tokens_used,
            cost_usd: cost_used,
            memory_before_hash,
            memory_after_hash,
            errors,
        };

        let daily_log = DailyLog::new(&self.daemon_state_dir)?;
        daily_log.append(&LogEntry {
            timestamp: Utc::now(),
            kind: LogEntryKind::Dream,
            content: DreamReporter::format_brief(&report),
            tools_used: vec!["MemoryWrite".into()],
            tokens_consumed: tokens_used,
            cost_usd: cost_used,
            trust_level: 1.0,
            signals_present: vec![],
        })?;

        tracing::info!("{}", DreamReporter::format_brief(&report));
        Ok(report)
    }

    fn apply_operation(&self, op: &MemoryOperation, memory: &MemorySystem) -> anyhow::Result<bool> {
        match &op.kind {
            OperationKind::Create => {
                let conf = if op.confidence >= 0.8 {
                    Confidence::Verified
                } else if op.confidence >= 0.5 {
                    Confidence::Observed
                } else {
                    Confidence::Inferred
                };
                let res = memory.writer.store(
                    &op.topic,
                    &op.subtopic,
                    &op.content,
                    Some("autoDream"),
                    conf,
                )?;
                Ok(res.stored)
            }
            OperationKind::Update { existing_path } => {
                memory.topics.append(
                    existing_path,
                    &format!("[autoDream update] {}", op.content),
                    Some("autoDream consolidation"),
                    Confidence::Observed,
                )?;
                memory.index.upsert_pointer(
                    &op.topic,
                    existing_path,
                    &op.content[..op.content.len().min(120)],
                )?;
                Ok(true)
            }
            OperationKind::Merge { source_entries } => {
                memory.writer.store(
                    &op.topic,
                    &op.subtopic,
                    &format!("[Merged by autoDream] {}", op.content),
                    Some("autoDream merge"),
                    Confidence::Verified,
                )?;
                for source in source_entries {
                    let _ = memory.topics.delete(source).ok();
                    let _ = memory.index.remove_pointer(source).ok();
                }
                Ok(true)
            }
            OperationKind::Prune { reason } => {
                let path = format!(
                    "{}/{}.md",
                    op.topic.to_lowercase().replace(' ', "-"),
                    op.subtopic.to_lowercase().replace(' ', "-")
                );
                let removed = memory.topics.delete(&path).unwrap_or(false);
                memory.index.remove_pointer(&path).ok();
                if removed {
                    tracing::debug!("Pruned {}/{} (reason: {:?})", op.topic, op.subtopic, reason);
                }
                Ok(removed)
            }
            OperationKind::Confirm { to_confidence, .. } => {
                let conf = if to_confidence == "verified" {
                    Confidence::Verified
                } else {
                    Confidence::Observed
                };
                let path = format!(
                    "{}/{}.md",
                    op.topic.to_lowercase().replace(' ', "-"),
                    op.subtopic.to_lowercase().replace(' ', "-")
                );
                memory.topics.append(
                    &path,
                    &format!("[autoDream confirmed] {}", op.content),
                    Some("autoDream confirmation"),
                    conf,
                )?;
                Ok(true)
            }
            OperationKind::Conflict {
                existing_data,
                new_data,
            } => {
                let ticket_id = format!("conflict_{}_{}", Utc::now().timestamp(), op.topic);
                let conflict_path = memory
                    .memory_dir()
                    .join("conflicts")
                    .join(format!("{}.md", ticket_id));
                let ticket_content = format!(
                    "# Knowledge Conflict: {}/{}\n\n## Reason\n{}\n\n## Existing Knowledge\n{}\n\n## New Contradicting Observation\n{}\n\n---\nStatus: Pending Mediation\nGenerated by: autoDream",
                    op.topic, op.subtopic, op.reasoning, existing_data, new_data
                );
                std::fs::write(conflict_path, ticket_content)?;
                tracing::warn!("Knowledge Conflict Ticket Generated: {}", ticket_id);
                Ok(true)
            }
        }
    }

    fn snapshot_memory(&self, memory: &MemorySystem) -> anyhow::Result<MemorySnapshot> {
        let index_content = memory.index.load_raw()?;
        let topic_files = memory
            .topics
            .list_all()?
            .iter()
            .filter_map(|path| {
                memory
                    .topics
                    .read(path)
                    .ok()
                    .flatten()
                    .map(|c| (path.clone(), c))
            })
            .collect();
        Ok(MemorySnapshot {
            index_content,
            topic_files,
        })
    }

    fn rollback_memory(
        &self,
        memory: &MemorySystem,
        snapshot: &MemorySnapshot,
    ) -> anyhow::Result<()> {
        tracing::warn!("Rolling back memory!");
        std::fs::write(
            memory.memory_dir().join("MEMORY.md"),
            &snapshot.index_content,
        )?;
        for (path, content) in &snapshot.topic_files {
            let full = memory.memory_dir().join("topics").join(path);
            if let Some(p) = full.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&full, content)?;
        }
        Ok(())
    }

    fn hash_memory_state(&self, memory: &MemorySystem) -> anyhow::Result<String> {
        let mut hasher = Sha256::new();
        let index_raw: String = memory.index.load_raw()?;
        hasher.update(index_raw.as_bytes());
        for path in memory.topics.list_all()? {
            let path: String = path;
            if let Some(c) = memory.topics.read(&path)? {
                let c: String = c;
                hasher.update(path.as_bytes());
                hasher.update(c.as_bytes());
            }
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn empty_report(&self, started_at: chrono::DateTime<Utc>, hash: &str) -> DreamReport {
        DreamReport {
            started_at,
            completed_at: Utc::now(),
            duration_secs: 0,
            observations_collected: 0,
            operations_planned: 0,
            operations_applied: 0,
            entries_merged: 0,
            entries_created: 0,
            entries_pruned: 0,
            entries_confirmed: 0,
            contradictions_resolved: 0,
            tokens_consumed: 0,
            cost_usd: 0.0,
            memory_before_hash: hash.to_string(),
            memory_after_hash: hash.to_string(),
            errors: vec![],
        }
    }
}

struct MemorySnapshot {
    index_content: String,
    topic_files: Vec<(String, String)>,
}
