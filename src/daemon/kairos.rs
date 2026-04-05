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
            tools_used: vec![],
            tokens_consumed: 0,
            cost_usd: 0.0,
            trust_level: self.initiative_engine.trust().current_level,
            signals_present: vec![],
        })?;

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
                    )?;
                }
                Initiative::Sleep => {}
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
            let report = engine.dream(&mem, qe).await?;
            tracing::info!("{}", DreamReporter::format_brief(&report));
            Ok(())
        } else {
            anyhow::bail!("No query engine available for autoDream")
        }
    }

    async fn handle_action(&mut self, action: ProactiveAction) -> anyhow::Result<()> {
        let action_description = format!("{:?}", action);
        let trust = self.initiative_engine.trust().current_level;
        self.daily_log
            .log_decision(&format!("Decided to act: {}", action_description), trust)?;

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
                    .log_error(&format!("Action failed: {}", e), trust)?;
            }
            Err(_) => {
                self.daily_log
                    .log_timeout(&format!("Action timed out: {}", action_description), trust)?;
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
        message: &str,
        _urgency: &Urgency,
    ) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        {
            let _ = tokio::process::Command::new("osascript")
                .args([
                    "-e",
                    &format!(
                        "display notification \"{}\" with title \"DreamSwarm\"",
                        message
                    ),
                ])
                .output()
                .await;
        }
        Ok(())
    }
}
