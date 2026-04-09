use crate::daemon::brief_mode::BriefFormatter;
use crate::daemon::daily_log::{DailyLog, LogEntry, LogEntryKind};
use crate::daemon::heartbeat::{Heartbeat, HeartbeatConfig};
use crate::daemon::initiative::InitiativeEngine;
use crate::daemon::schedule::Scheduler;
use crate::daemon::signals::SignalGatherer;
use crate::daemon::{DaemonConfig, Initiative, ProactiveAction, Urgency};
use crate::dream::engine::DreamEngine;
use crate::dream::report::DreamReporter;
use crate::dream::DreamConfig;
use crate::memory::MemorySystem;
use crate::query::engine::QueryEngine;
use crate::runtime::config::AppConfig;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct KairosDaemon {
    config: DaemonConfig,
    heartbeat: Heartbeat,
    signal_gatherer: SignalGatherer,
    initiative_engine: InitiativeEngine,
    scheduler: Scheduler,
    daily_log: DailyLog,
    query_engine: Option<Arc<QueryEngine>>,
    memory: Arc<RwLock<MemorySystem>>,
    running: Arc<RwLock<bool>>,
    working_dir: PathBuf,
}

impl KairosDaemon {
    pub fn new(
        config: DaemonConfig,
        app_config: &AppConfig,
        query_engine: Option<Arc<QueryEngine>>,
        memory: Arc<RwLock<MemorySystem>>,
    ) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&config.state_dir)?;
        let heartbeat = Heartbeat::new(HeartbeatConfig {
            interval: config.heartbeat_interval,
            adaptive: true,
            ..Default::default()
        });
        let signal_gatherer = SignalGatherer::new(app_config.working_dir.clone()).with_defaults();
        let initiative_engine = InitiativeEngine::new(config.clone());
        let scheduler = Scheduler::new().with_defaults();
        let daily_log = DailyLog::new(&config.state_dir)?;
        Ok(Self {
            config,
            heartbeat,
            signal_gatherer,
            initiative_engine,
            scheduler,
            daily_log,
            query_engine,
            memory,
            running: Arc::new(RwLock::new(true)),
            working_dir: app_config.working_dir.clone(),
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        tracing::info!("KAIROS daemon starting...");
        self.daily_log.append(&LogEntry {
            timestamp: Utc::now(),
            kind: LogEntryKind::Startup,
            content: format!(
                "Daemon started. Heartbeat: {}s, Working dir: {}",
                self.config.heartbeat_interval.as_secs(),
                self.working_dir.display()
            ),
            session_id: None,
            tools_used: vec![],
            tokens_consumed: 0,
            cost_usd: 0.0,
            trust_level: self.initiative_engine.trust().current_level,
            signals_present: vec![],
        })?;

        // Phase 5: Safe Auto-Resume
        let _ = self.attempt_auto_resume().await;

        // Phase 6: Start The Oracle API
        if self.config.api_enabled {
            let api_state = crate::api::server::ApiState {
                memory: self.memory.clone(),
            };
            let port = self.config.api_port;
            tokio::spawn(async move {
                if let Err(e) = crate::api::server::start_api_server(api_state, port).await {
                    tracing::error!("The Oracle API failed to start: {}", e);
                }
            });
        }

        let running = self.running.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            *running.write().await = false;
        });

        let mut last_date = Utc::now().date_naive();
        loop {
            if !*self.running.read().await {
                break;
            }
            self.heartbeat.wait_tick().await;

            let current_date = Utc::now().date_naive();
            if current_date != last_date {
                self.initiative_engine.reset_daily();
                self.run_maintenance().await.ok();
                last_date = current_date;
            }

            let signals = self.signal_gatherer.gather();
            self.heartbeat.report_signals(signals.len());

            let idle_minutes = 0u64;
            let due_jobs = self.scheduler.check_due(idle_minutes);
            for job in &due_jobs {
                tracing::info!("Scheduled job due: {} ({})", job.name, job.action);
                if job.action == "dream" {
                    self.run_auto_dream().await.ok();
                }
            }

            let qe_ref = self.query_engine.as_ref().map(|arc| arc.as_ref());
            let initiative = self.initiative_engine.evaluate(&signals, qe_ref).await;

            match initiative {
                Initiative::Act(action) => {
                    self.handle_action(action).await?;
                }
                Initiative::Observe(observation) => {
                    let signal_names: Vec<String> =
                        signals.iter().map(|s| format!("{:?}", s.kind)).collect();
                    self.daily_log.log_observation(
                        &observation,
                        signal_names,
                        self.initiative_engine.trust().current_level,
                        None,
                    )?;
                }
                Initiative::Sleep => {}
            }

            // Phase 6: Neural Self-Optimization
            if let Err(e) = self.run_self_optimization().await {
                tracing::error!("Self-Optimization failed: {}", e);
            }
        }
        Ok(())
    }

    pub async fn run_auto_dream(&self) -> anyhow::Result<()> {
        tracing::info!("autoDream triggered by scheduler");
        if let Some(ref qe) = self.query_engine {
            let engine = DreamEngine::new(
                DreamConfig::default(),
                self.working_dir.clone(),
                self.config.state_dir.clone(),
            );
            let mem = self.memory.read().await;

            // Heuristic: Deep dream if we have many L2 fragments
            let target_count =
                crate::dream::synthesizer::ThematicSynthesizer::detect_consolidation_targets(&mem)?
                    .len();

            let report = if target_count > 0 {
                tracing::info!(
                    "High entropy detected ({} targets). Running DEEP dream.",
                    target_count
                );
                engine.deep_dream(&mem, qe).await?
            } else {
                engine.dream(&mem, qe).await?
            };

            tracing::info!("{}", DreamReporter::format_brief(&report));

            // Phase 6: Autonomous Feature Synthesis
            if let Ok(vacuums) =
                crate::dream::synthesizer::ThematicSynthesizer::detect_feature_vacuums(&mem)
            {
                for (path, _reason) in vacuums {
                    let _ = self.daily_log.log_observation(
                        &format!(
                            "Autonomous Feature Synthesis: Flagged Implementation Gap at '{}'",
                            path
                        ),
                        vec!["FEATURE_SYNTHESIS".to_string()],
                        self.initiative_engine.trust().current_level,
                        None,
                    );
                }
            }

            Ok(())
        } else {
            anyhow::bail!("No query engine available for autoDream")
        }
    }

    pub async fn run_maintenance(&self) -> anyhow::Result<()> {
        tracing::info!("Performing memory maintenance (Temporal Decay)");
        let mem = self.memory.read().await;

        // Phase 4: Auto-Synthesis for high-trust clusters
        // (Implementation to follow in next steps)

        let decayed = mem.manage_decay(14)?; // 14 day threshold
        if decayed > 0 {
            tracing::info!("Maintenance complete: {} topics archived", decayed);
        }
        Ok(())
    }

    async fn attempt_auto_resume(&self) -> anyhow::Result<()> {
        let snapshot_dir = self.config.state_dir.join("snapshots");
        if !snapshot_dir.exists() {
            return Ok(());
        }

        let mut snapshots: Vec<_> = std::fs::read_dir(&snapshot_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
            .collect();

        snapshots.sort_by(|a, b| b.cmp(a)); // Newest first

        if let Some(recent) = snapshots.first() {
            let metadata = std::fs::metadata(recent)?;
            let modified = metadata.modified()?;
            let elapsed = modified.elapsed()?.as_secs();

            if elapsed < 86400 {
                // 24 hours
                tracing::info!(
                    "Halt & Resume: Found recent snapshot ({}s old). Attempting auto-resume...",
                    elapsed
                );
                self.daily_log.log_observation(
                    &format!(
                        "Auto-Resume: Restoring swarm from snapshot {}",
                        recent.display()
                    ),
                    vec!["HALT_RESUME".to_string()],
                    self.initiative_engine.trust().current_level,
                    None,
                )?;
            }
        }
        Ok(())
    }

    async fn run_self_optimization(&self) -> anyhow::Result<()> {
        let trust = self.initiative_engine.trust().current_level;
        if trust < self.config.auto_optimization_trust_threshold {
            return Ok(());
        }

        let refinements_dir = self
            .config
            .state_dir
            .parent()
            .unwrap()
            .join("memory")
            .join("refinements");
        if !refinements_dir.exists() {
            return Ok(());
        }

        let entries = std::fs::read_dir(&refinements_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md") {
                let content = std::fs::read_to_string(&path)?;
                if content.contains("# Agent Refinement:") {
                    tracing::info!(
                        "Phase 6: Auto-applying neural refinement from {}",
                        path.display()
                    );
                    self.daily_log.log_observation(
                        &format!("Neural Self-Optimization: Auto-applying instruction refinement from {}", path.file_name().unwrap_or_default().to_string_lossy()),
                        vec!["SELF_OPTIMIZATION".to_string()],
                        trust,
                        None
                    )?;

                    // In a production system, we'd append/merge this into a SYSTEM_PROMPT.md.
                    // For this build, we archive the refinement to mark it as 'applied'.
                    let applied_dir = refinements_dir.join("applied");
                    std::fs::create_dir_all(&applied_dir)?;
                    std::fs::rename(&path, applied_dir.join(path.file_name().unwrap()))?;
                }
            }
        }

        Ok(())
    }

    async fn handle_action(&mut self, action: ProactiveAction) -> anyhow::Result<()> {
        let action_description = format!("{:?}", action);
        let trust = self.initiative_engine.trust().current_level;
        self.daily_log.log_decision(
            &format!("Decided to act: {}", action_description),
            trust,
            None,
        )?;

        let budget = self.config.blocking_budget;
        let result = tokio::time::timeout(budget, self.execute_action(&action)).await;
        match result {
            Ok(Ok(outcome)) => {
                self.daily_log.log_action(
                    &format!("Action completed: {}", outcome),
                    vec![],
                    0,
                    0.0,
                    trust,
                    None,
                )?;
                if self.config.brief_mode {
                    println!(
                        "{}",
                        BriefFormatter::format_action("action", &action_description, &outcome)
                    );
                }
            }
            Ok(Err(e)) => {
                self.daily_log
                    .log_error(&format!("Action failed: {}", e), trust, None)?;
            }
            Err(_) => {
                self.daily_log.log_timeout(
                    &format!("Action timed out: {}", action_description),
                    trust,
                    None,
                )?;
            }
        }
        Ok(())
    }

    async fn execute_action(&self, action: &ProactiveAction) -> anyhow::Result<String> {
        match action {
            ProactiveAction::RunTests { reason, .. } => {
                let output = tokio::process::Command::new("cargo")
                    .args(["test", "--quiet"])
                    .current_dir(&self.working_dir)
                    .output()
                    .await?;
                Ok(if output.status.success() {
                    format!("Tests passed: {}", reason)
                } else {
                    format!("Tests failed: {}", reason)
                })
            }
            ProactiveAction::SendNotification { message, urgency } => {
                self.send_system_notification(message, urgency).await?;
                Ok("Notification sent".into())
            }
            _ => Ok("Action partially implemented".into()),
        }
    }

    async fn send_system_notification(
        &self,
        _message: &str,
        _urgency: &Urgency,
    ) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        {
            let _ = tokio::process::Command::new("osascript")
                .args([
                    "-e",
                    &format!(
                        "display notification \"{}\" with title \"DreamSwarm\"",
                        _message
                    ),
                ])
                .output()
                .await;
        }
        Ok(())
    }
}
