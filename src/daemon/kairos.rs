use crate::daemon::brief_mode::BriefFormatter;
use crate::daemon::daily_log::{DailyLog, LogEntryKind};
use crate::daemon::heartbeat::{Heartbeat, HeartbeatConfig};
use crate::daemon::initiative::InitiativeEngine;
use crate::daemon::schedule::Scheduler;
use crate::daemon::signals::SignalGatherer;
use crate::daemon::{DaemonConfig, Initiative, ProactiveAction, Urgency};
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
    running: Arc<RwLock<bool>>,
    working_dir: PathBuf,
}

impl KairosDaemon {
    pub fn new(
        config: DaemonConfig,
        app_config: &AppConfig,
        query_engine: Option<Arc<QueryEngine>>,
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
            running: Arc::new(RwLock::new(true)),
            working_dir: app_config.working_dir.clone(),
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        tracing::info!("KAIROS daemon starting...");
        self.daily_log.append(&crate::daemon::daily_log::LogEntry {
            timestamp: Utc::now(),
            kind: LogEntryKind::Startup,
            content: format!("Daemon started. Heartbeat: {}s, Budget: {}s, Working dir: {}", self.config.heartbeat_interval.as_secs(), self.config.blocking_budget.as_secs(), self.working_dir.display()),
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
            if !*self.running.read().await { break; }
            self.heartbeat.wait_tick().await;

            let current_date = Utc::now().date_naive();
            if current_date != last_date {
                self.initiative_engine.reset_daily();
                last_date = current_date;
                tracing::info!("New day - daily counters reset");
            }

            let signals = self.signal_gatherer.gather();
            self.heartbeat.report_signals(signals.len());

            let idle_minutes = 0u64; 
            let due_jobs = self.scheduler.check_due(idle_minutes);
            for job in &due_jobs {
                tracing::info!("Scheduled job due: {} ({})", job.name, job.action);
            }

            let qe_ref = self.query_engine.as_ref().map(|arc| arc.as_ref());
            let initiative = self.initiative_engine.evaluate(&signals, qe_ref).await;

            match initiative {
                Initiative::Act(action) => { self.handle_action(action).await?; }
                Initiative::Observe(observation) => {
                    let signal_names: Vec<String> = signals.iter().map(|s| format!("{:?}", s.kind)).collect();
                    self.daily_log.log_observation(&observation, signal_names, self.initiative_engine.trust().current_level)?;
                    if self.config.brief_mode {
                        tracing::debug!("{}", BriefFormatter::format_observation(&observation));
                    }
                }
                Initiative::Sleep => {}
            }
        }
        tracing::info!("KAIROS daemon shutting down...");
        self.daily_log.append(&crate::daemon::daily_log::LogEntry {
            timestamp: Utc::now(),
            kind: LogEntryKind::Shutdown,
            content: format!("Daemon stopped after {} ticks", self.heartbeat.tick_count()),
            tools_used: vec![],
            tokens_consumed: 0,
            cost_usd: 0.0,
            trust_level: self.initiative_engine.trust().current_level,
            signals_present: vec![],
        })?;
        Ok(())
    }

    async fn handle_action(&mut self, action: ProactiveAction) -> anyhow::Result<()> {
        let action_description = format!("{:?}", action);
        let trust = self.initiative_engine.trust().current_level;
        tracing::info!("Executing proactive action: {}", &action_description[..action_description.len().min(100)]);
        self.daily_log.log_decision(&format!("Decided to act: {}", &action_description[..action_description.len().min(200)]), trust)?;

        let budget = self.config.blocking_budget;
        let result = tokio::time::timeout(budget, self.execute_action(&action)).await;
        match result {
            Ok(Ok(outcome)) => {
                tracing::info!("Action completed: {}", &outcome[..outcome.len().min(100)]);
                self.daily_log.log_action(&format!("Action completed: {}", outcome), vec![], 0, 0.0, trust)?;
                if self.config.brief_mode {
                    println!("{}", BriefFormatter::format_action("action", &action_description, &outcome));
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Action failed: {}", e);
                self.daily_log.log_error(&format!("Action failed: {}", e), trust)?;
            }
            Err(_) => {
                tracing::warn!("Action timed out after {}s: {}", budget.as_secs(), &action_description[..action_description.len().min(80)]);
                self.daily_log.log_timeout(&format!("Action timed out after {}s: {}", budget.as_secs(), action_description), trust)?;
            }
        }
        Ok(())
    }

    async fn execute_action(&self, action: &ProactiveAction) -> anyhow::Result<String> {
        match action {
            ProactiveAction::RunTests { reason, .. } => {
                let output = tokio::process::Command::new("cargo").args(["test", "--quiet"]).current_dir(&self.working_dir).output().await?;
                if output.status.success() {
                    Ok(format!("Tests passed. Reason: {}", reason))
                } else {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Ok(format!("Tests failed. Reason: {}. Output: {}", reason, &stdout[..stdout.len().min(200)]))
                }
            }
            ProactiveAction::SendNotification { message, urgency } => {
                let result = self.send_system_notification(message, urgency).await;
                Ok(format!("Notification status: {:?}", result))
            }
            ProactiveAction::CustomAction { description, .. } => {
                Ok(format!("Custom action executed: {}", description))
            }
            _ => Ok(format!("Action {:?} not fully implemented in mock", action))
        }
    }

    async fn send_system_notification(&self, message: &str, _urgency: &Urgency) -> anyhow::Result<()> {
        if !self.config.notifications_enabled { return Ok(()); }
        #[cfg(target_os = "macos")]
        {
            let _ = tokio::process::Command::new("osascript").args(["-e", &format!("display notification \"{}\" with title \"DreamSwarm KAIROS\"", message.replace('"', "\\\""))]).output().await;
        }
        #[cfg(target_os = "linux")]
        {
            let _ = tokio::process::Command::new("notify-send").args(["DreamSwarm KAIROS", message]).output().await;
        }
        Ok(())
    }

    pub fn trust(&self) -> &crate::daemon::trust::TrustSystem { self.initiative_engine.trust() }
    pub fn daily_log(&self) -> &DailyLog { &self.daily_log }
}
