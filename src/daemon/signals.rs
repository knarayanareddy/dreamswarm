use chrono::{DateTime, Utc};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub kind: SignalKind,
    pub source: String,
    pub description: String,
    pub timestamp: DateTime<Utc>,
    pub severity: SignalSeverity,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SignalKind {
    FileChanged,
    FileCreated,
    FileDeleted,
    GitPush,
    GitPROpened,
    GitPRUpdated,
    GitPRMerged,
    CIFailed,
    CIPassed,
    BuildError,
    TestFailure,
    IdleTimeout,
    CronTrigger,
    DependencyUpdate,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SignalSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

pub trait SignalSource: Send + Sync {
    fn poll(&mut self) -> Vec<Signal>;
    fn name(&self) -> &str;
}

pub struct SignalGatherer {
    watchers: Vec<Box<dyn SignalSource + Send + Sync>>,
    working_dir: PathBuf,
}

impl SignalGatherer {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            watchers: Vec::new(),
            working_dir,
        }
    }

    pub fn with_defaults(mut self) -> Self {
        self.watchers
            .push(Box::new(FileWatcher::new(self.working_dir.clone())));
        self.watchers
            .push(Box::new(GitWatcher::new(self.working_dir.clone())));
        self.watchers.push(Box::new(IdleWatcher::new()));
        self
    }

    pub fn add_source(&mut self, source: Box<dyn SignalSource + Send + Sync>) {
        self.watchers.push(source);
    }

    pub fn gather(&mut self) -> Vec<Signal> {
        let mut all_signals = Vec::new();
        for watcher in &mut self.watchers {
            let signals = watcher.poll();
            if !signals.is_empty() {
                tracing::debug!("{} produced {} signals", watcher.name(), signals.len());
            }
            all_signals.extend(signals);
        }
        all_signals.sort_by(|a, b| b.severity.cmp(&a.severity));
        all_signals
    }
}

pub struct FileWatcher {
    working_dir: PathBuf,
    receiver: Option<Mutex<mpsc::Receiver<notify::Result<Event>>>>,
    _watcher: Option<RecommendedWatcher>,
}

impl FileWatcher {
    pub fn new(working_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )
        .ok();

        let mut fw = Self {
            working_dir: working_dir.clone(),
            receiver: Some(Mutex::new(rx)),
            _watcher: watcher,
        };

        if let Some(ref mut w) = fw._watcher {
            let _ = w.watch(&working_dir, RecursiveMode::Recursive);
        }
        fw
    }

    fn should_ignore(&self, path: &std::path::Path) -> bool {
        let path_str = path.to_string_lossy();
        let ignore_patterns = [
            ".git/",
            "node_modules/",
            "target/",
            "__pycache__/",
            ".dreamswarm/",
            ".DS_Store",
            "*.swp",
            "*.swo",
            "*~",
        ];
        ignore_patterns.iter().any(|pattern| {
            if pattern.starts_with('*') {
                path_str.ends_with(&pattern[1..])
            } else {
                path_str.contains(pattern)
            }
        })
    }
}

impl SignalSource for FileWatcher {
    fn poll(&mut self) -> Vec<Signal> {
        let mut signals = Vec::new();
        if let Some(ref rx_mutex) = self.receiver {
            if let Ok(rx) = rx_mutex.lock() {
                while let Ok(event_result) = rx.try_recv() {
                    if let Ok(event) = event_result {
                        for path in &event.paths {
                            if self.should_ignore(path) {
                                continue;
                            }
                            let relative = path
                                .strip_prefix(&self.working_dir)
                                .unwrap_or(path)
                                .to_string_lossy()
                                .to_string();
                            let (kind, description) = match event.kind {
                                EventKind::Create(_) => (
                                    SignalKind::FileCreated,
                                    format!("File created: {}", relative),
                                ),
                                EventKind::Modify(_) => (
                                    SignalKind::FileChanged,
                                    format!("File modified: {}", relative),
                                ),
                                EventKind::Remove(_) => (
                                    SignalKind::FileDeleted,
                                    format!("File deleted: {}", relative),
                                ),
                                _ => continue,
                            };
                            signals.push(Signal {
                                kind,
                                source: "file_watcher".to_string(),
                                description,
                                timestamp: Utc::now(),
                                severity: SignalSeverity::Info,
                                metadata: serde_json::json!({ "path": relative }),
                            });
                        }
                    }
                }
            }
        }
        signals.dedup_by(|a, b| a.metadata == b.metadata && a.kind == b.kind);
        signals
    }
    fn name(&self) -> &str {
        "file_watcher"
    }
}

pub struct GitWatcher {
    working_dir: PathBuf,
    last_head: Option<String>,
}

impl GitWatcher {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            last_head: None,
        }
    }
    fn get_head(&self) -> Option<String> {
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.working_dir)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout)
                        .ok()
                        .map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
    }
}

impl SignalSource for GitWatcher {
    fn poll(&mut self) -> Vec<Signal> {
        let mut signals = Vec::new();
        if let Some(current_head) = self.get_head() {
            if let Some(ref last) = self.last_head {
                if last != &current_head {
                    signals.push(Signal {
                        kind: SignalKind::GitPush,
                        source: "git_watcher".to_string(),
                        description: format!(
                            "HEAD changed: {}..{}",
                            &last[..7],
                            &current_head[..7]
                        ),
                        timestamp: Utc::now(),
                        severity: SignalSeverity::Info,
                        metadata: serde_json::json!({ "old_head": last, "new_head": current_head }),
                    });
                }
            }
            self.last_head = Some(current_head);
        }
        signals
    }
    fn name(&self) -> &str {
        "git_watcher"
    }
}

pub struct IdleWatcher {
    last_user_activity: DateTime<Utc>,
    idle_reported: bool,
    idle_threshold: Duration,
}

impl Default for IdleWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl IdleWatcher {
    pub fn new() -> Self {
        Self {
            last_user_activity: Utc::now(),
            idle_reported: false,
            idle_threshold: Duration::from_secs(300),
        }
    }
    pub fn record_activity(&mut self) {
        self.last_user_activity = Utc::now();
        self.idle_reported = false;
    }
}

impl SignalSource for IdleWatcher {
    fn poll(&mut self) -> Vec<Signal> {
        let idle_duration = Utc::now()
            .signed_duration_since(self.last_user_activity)
            .to_std()
            .unwrap_or_default();
        if idle_duration >= self.idle_threshold && !self.idle_reported {
            self.idle_reported = true;
            vec![Signal {
                kind: SignalKind::IdleTimeout,
                source: "idle_watcher".to_string(),
                description: format!("User idle for {} seconds", idle_duration.as_secs()),
                timestamp: Utc::now(),
                severity: SignalSeverity::Info,
                metadata: serde_json::json!({ "idle_seconds": idle_duration.as_secs() }),
            }]
        } else {
            vec![]
        }
    }
    fn name(&self) -> &str {
        "idle_watcher"
    }
}
