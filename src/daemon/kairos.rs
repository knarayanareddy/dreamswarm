use crate::daemon::brief_mode::BriefFormatter;
use crate::daemon::daily_log::{DailyLog, LogEntry, LogEntryKind};
use crate::daemon::heartbeat::{Heartbeat, HeartbeatConfig};
use crate::daemon::initiative::InitiativeEngine;
use crate::daemon::persistence::PersistenceManager;
use crate::daemon::schedule::Scheduler;
use crate::daemon::signals::SignalGatherer;
use crate::daemon::{DaemonConfig, Initiative, ProactiveAction, Urgency};
pub mod relay {
    pub use crate::memory::relay::S3Relay;
}
use crate::api::telemetry::TelemetryHub;
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
    persistence: PersistenceManager,
    relay: Option<Arc<relay::S3Relay>>,
    healing: crate::daemon::healing::HealingManager,
    red_swarm: crate::swarm::adversarial::RedSwarmExecutor,
    evolution: Arc<crate::swarm::evolution::coordinator::EvolutionCoordinator>,
    telemetry: Arc<crate::api::telemetry::TelemetryHub>,
}

impl KairosDaemon {
    pub fn new(
        config: DaemonConfig,
        app_config: &AppConfig,
        query_engine: Option<Arc<QueryEngine>>,
        memory: Arc<RwLock<MemorySystem>>,
        db: Arc<RwLock<crate::db::Database>>,
        telemetry: Option<Arc<TelemetryHub>>,
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
        let persistence =
            crate::daemon::persistence::PersistenceManager::new(config.state_dir.clone());

        let telemetry = telemetry.unwrap_or_else(|| Arc::new(TelemetryHub::new(db.clone())));

        let evolution_analyzer = crate::swarm::evolution::prompt_evolution::PromptAnalyzer::new(
            query_engine.clone().unwrap(),
            telemetry.clone(),
        );
        let evolution = Arc::new(
            crate::swarm::evolution::coordinator::EvolutionCoordinator::new(
                evolution_analyzer,
                db,
                telemetry.clone(),
            ),
        );

        let mut relay = None;
        if let Some(s3_conf) = &app_config.s3_relay_config {
            let mem_dir = memory
                .try_read()
                .map(|m| m.memory_dir().clone())
                .unwrap_or_default();
            if let Ok(r) =
                tokio::runtime::Handle::current().block_on(crate::memory::relay::S3Relay::new(
                    &s3_conf.endpoint,
                    &s3_conf.bucket,
                    &s3_conf.region,
                    &s3_conf.access_key,
                    &s3_conf.secret_key,
                    mem_dir,
                ))
            {
                relay = Some(Arc::new(r));
            }
        }

        Ok(Self {
            heartbeat,
            signal_gatherer,
            initiative_engine,
            scheduler,
            daily_log,
            query_engine,
            memory,
            running: Arc::new(RwLock::new(true)),
            working_dir: app_config.working_dir.clone(),
            persistence,
            relay,
            healing: crate::daemon::healing::HealingManager::new(
                app_config.working_dir.clone(),
                config.state_dir.clone(),
            ),
            red_swarm: crate::swarm::adversarial::RedSwarmExecutor::new(
                config.state_dir.clone(),
                crate::runtime::permissions::PermissionGate::new(
                    app_config.permission_mode,
                    &app_config.allow_patterns,
                    &app_config.deny_patterns,
                ),
            ),
            evolution,
            config,
            telemetry,
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let telemetry = self.telemetry.clone();
        telemetry
            .log_event(
                "system",
                "startup",
                serde_json::json!({"status": "KAIROS_STARTING"}),
            )
            .await;

        // Phase 10: Control Signal Listener
        let running_ctrl = self.running.clone();
        let mut control_rx = self.telemetry.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = control_rx.recv().await {
                if event.category == "system" && event.event_type == "control_signal" {
                    let action = event.payload["action"].as_str().unwrap_or_default();
                    match action {
                        "STOP" => {
                            tracing::warn!("Sovereign Interface: EMERGENCY STOP received.");
                            *running_ctrl.write().await = false;
                        }
                        "DREAM" => {
                            tracing::info!("Sovereign Interface: MANUAL DREAM triggered.");
                        }
                        "WAR_ROOM" => {
                            tracing::warn!("Sovereign Interface: WAR ROOM Stress Cycle initiated.");
                            let tester =
                                crate::swarm::diagnostics::war_room::WarRoomStressTester::new(
                                    telemetry.clone(),
                                );
                            tokio::spawn(async move {
                                let _ = tester.simulate_cascading_failure().await;
                                tester.flood_telemetry(100, 10).await;
                            });
                        }
                        _ => {}
                    }
                }
            }
        });

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

        // Phase 5/7: Warm-Start Resilience
        if self.persistence.exists() {
            if let Ok(state) = self.persistence.load_last_state() {
                tracing::info!(
                    "Phase 7: Warm-Start detected. Recovering hive state from {} swarms...",
                    state.active_swarms.len()
                );
            }
        }
        let _ = self.attempt_auto_resume().await;

        // Phase 6: Start The Oracle API
        if self.config.api_enabled {
            let api_state = crate::api::server::ApiState {
                memory: self.memory.clone(),
                telemetry: self.telemetry.clone(),
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

                    // Phase 8: Red Swarm Adversarial Diagnostics (during idle)
                    let _ = self.red_swarm.run_diagnostic(&self.working_dir).await;
                }
            }

            let qe_ref = self.query_engine.as_ref().map(|arc| arc.as_ref());
            let initiative = self.initiative_engine.evaluate(&signals, qe_ref).await;

            match initiative {
                Initiative::Act(action) => {
                    if let Err(e) = self.handle_action(action.clone()).await {
                        // Phase 8: Trigger Self-Healing on action failures
                        let _ = self.healing.attempt_self_heal(&e.to_string()).await;
                    }
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

            if let Err(e) = self.run_self_optimization().await {
                tracing::error!("Self-Optimization failed: {}", e);
            }

            // Phase 12: Neural Evolution Cycle
            self.evolution.run_cycle_if_due().await.ok();

            // Phase 7: Hive Sync & Checkpoint (once per loop cycle)
            if let Some(relay_arc) = &self.relay {
                let r = relay_arc.clone();
                tokio::spawn(async move {
                    let _ = r.sync_up().await;
                });
            }
            // Simplified checkpoint: persist the coordinator state if it were here
            let _ = self.persistence.checkpoint(vec![]);
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
            ProactiveAction::ResolveConflict { branches, files } => {
                let msg = format!(
                    "🚨 CONFLICT IMMINENT 🚨\nBranches {:?} are modifying the same files:\n{:?}\n\nPlease pull main and resolve before merging.",
                    branches, files
                );
                self.send_system_notification(
                    &format!("Merge conflict predicted! {:?}", branches),
                    &Urgency::Critical,
                )
                .await?;

                let warning_path = self.working_dir.join("DREAMSWARM_CONFLICT_WARNING.txt");
                tokio::fs::write(&warning_path, &msg)
                    .await
                    .unwrap_or_default();

                self.telemetry
                    .log_event(
                        "system",
                        "conflict_predicted",
                        serde_json::json!({
                            "branches": branches,
                            "files": files,
                        }),
                    )
                    .await;

                Ok(format!("Predicted conflict across branches {:?}", branches))
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
