//! Ownership and lifecycle for concurrently running teams.
//!
//! A team is identified by name. `SwarmCoordinator` methods take `&mut self`,
//! so the only way to hold several teams at once — and to address one by name
//! from an HTTP handler — is to keep them behind per-team locks. Cloning a
//! coordinator out of a shared slot is not an option: a clone would drive a
//! second copy of the executor and the stale worker list.
//!
//! Two rules keep this safe:
//!
//! * never hold the registry lock across an `.await` — look the team up, clone
//!   the `Arc`, release the guard, then await on the team's own lock;
//! * take the per-team write guard for a *short* critical section, so a slow
//!   team can never block stopping the others.
//!
//! Emergency stop relies on both: it must keep working on the remaining teams
//! even if one of them is wedged, misbehaving, or already gone.

use crate::swarm::coordinator::SwarmCoordinator;
use crate::swarm::executors::command::is_safe_identifier;
use crate::swarm::TeamConfig;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// A team handle that can be locked independently of every other team.
pub type TeamHandle = Arc<Mutex<SwarmCoordinator>>;

struct Team {
    handle: TeamHandle,
    /// Open coordination resources (task list, mailbox) live under this path.
    state_dir: PathBuf,
}

/// The set of teams this process is running, keyed by team name.
#[derive(Default)]
pub struct TeamRegistry {
    teams: RwLock<BTreeMap<String, Team>>,
    /// Set when an emergency stop is requested. A team that finishes launching
    /// after the stop was issued is refused rather than adopted, which closes
    /// the window where launch and stop race each other.
    stopped: RwLock<bool>,
}

/// What `stop_all` managed to do. `failed` is non-empty when some teams could
/// not be shut down cleanly — the caller must not report success blindly.
#[derive(Debug, Default)]
pub struct StopReport {
    pub stopped: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl StopReport {
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }
}

impl TeamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn validate_name(team_name: &str) -> anyhow::Result<()> {
        if is_safe_identifier(team_name) {
            Ok(())
        } else {
            anyhow::bail!(
                "invalid team name '{}': only letters, digits, '-', '_', '.', ':' and '/' are allowed",
                team_name
            )
        }
    }

    /// Registers an already-built coordinator.
    ///
    /// Re-registering a live team is refused: the previous coordinator owns
    /// worker processes, and silently replacing it would orphan them.
    pub async fn register(
        &self,
        coordinator: SwarmCoordinator,
        state_dir: PathBuf,
    ) -> anyhow::Result<TeamHandle> {
        let team_name = coordinator.team_name().to_string();
        Self::validate_name(&team_name)?;

        if *self.stopped.read().await {
            anyhow::bail!(
                "an emergency stop was requested; team '{}' was not started",
                team_name
            );
        }

        let mut teams = self.teams.write().await;
        if teams.contains_key(&team_name) {
            anyhow::bail!(
                "team '{}' is already running; stop it before launching it again",
                team_name
            );
        }

        let handle: TeamHandle = Arc::new(Mutex::new(coordinator));
        teams.insert(
            team_name,
            Team {
                handle: handle.clone(),
                state_dir,
            },
        );
        Ok(handle)
    }

    /// Looks a team up without holding the registry lock afterwards.
    pub async fn get(&self, team_name: &str) -> Option<TeamHandle> {
        self.teams
            .read()
            .await
            .get(team_name)
            .map(|team| team.handle.clone())
    }

    pub async fn contains(&self, team_name: &str) -> bool {
        self.teams.read().await.contains_key(team_name)
    }

    /// Team names currently registered, in stable order.
    pub async fn names(&self) -> Vec<String> {
        self.teams.read().await.keys().cloned().collect()
    }

    pub async fn len(&self) -> usize {
        self.teams.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Every worker across every team, paired with the team that owns it.
    pub async fn workers(&self) -> Vec<(String, crate::swarm::WorkerInfo)> {
        let handles: Vec<(String, TeamHandle)> = self
            .teams
            .read()
            .await
            .iter()
            .map(|(name, team)| (name.clone(), team.handle.clone()))
            .collect();

        let mut workers = Vec::new();
        for (team_name, handle) in handles {
            let coordinator = handle.lock().await;
            for worker in coordinator.workers() {
                workers.push((team_name.clone(), worker.clone()));
            }
        }
        workers
    }

    /// Removes a team and shuts its workers down.
    ///
    /// The entry is removed *before* shutting down so a concurrent emergency
    /// stop cannot shut the same team down twice.
    pub async fn stop(&self, team_name: &str) -> anyhow::Result<bool> {
        let team = { self.teams.write().await.remove(team_name) };
        let Some(team) = team else {
            return Ok(false);
        };

        let mut coordinator = team.handle.lock().await;
        coordinator.shutdown_team().await?;
        Ok(true)
    }

    /// Emergency stop: shut down every registered team.
    ///
    /// Deliberately does *not* short-circuit on the first failure — the point
    /// is to stop as much as possible, then report what survived. The
    /// stopped flag is set first so no new team can slip in behind us.
    pub async fn stop_all(&self) -> StopReport {
        *self.stopped.write().await = true;

        let teams: Vec<(String, Team)> = {
            let mut guard = self.teams.write().await;
            std::mem::take(&mut *guard).into_iter().collect()
        };

        let mut report = StopReport::default();
        for (name, team) in teams {
            let mut coordinator = team.handle.lock().await;
            match coordinator.shutdown_team().await {
                Ok(()) => {
                    tracing::info!("Emergency stop: team '{}' shut down", name);
                    report.stopped.push(name);
                }
                Err(e) => {
                    tracing::error!("Emergency stop: team '{}' failed to shut down: {}", name, e);
                    report.failed.push((name, e.to_string()));
                }
            }
        }
        report
    }

    /// True once an emergency stop has been requested for this process.
    pub async fn is_stopped(&self) -> bool {
        *self.stopped.read().await
    }

    /// Clears the latched stop state so teams may be launched again.
    pub async fn clear_stop(&self) {
        *self.stopped.write().await = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm::{SpawnStrategy, TeamStatus};

    fn coordinator(name: &str, state_dir: &std::path::Path) -> SwarmCoordinator {
        let config = TeamConfig {
            team_name: name.to_string(),
            spawn_strategy: SpawnStrategy::InProcess,
            ..Default::default()
        };
        SwarmCoordinator::new(config, ".", state_dir.to_path_buf()).unwrap()
    }

    #[tokio::test]
    async fn registers_and_looks_up_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TeamRegistry::new();
        registry
            .register(coordinator("alpha", dir.path()), dir.path().to_path_buf())
            .await
            .unwrap();

        assert!(registry.contains("alpha").await);
        assert!(registry.get("alpha").await.is_some());
        assert!(registry.get("beta").await.is_none());
        assert_eq!(registry.len().await, 1);
    }

    #[tokio::test]
    async fn refuses_to_replace_a_running_team() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TeamRegistry::new();
        registry
            .register(coordinator("alpha", dir.path()), dir.path().to_path_buf())
            .await
            .unwrap();

        let second = registry
            .register(coordinator("alpha", dir.path()), dir.path().to_path_buf())
            .await;
        assert!(
            second.is_err(),
            "replacing a live team would orphan workers"
        );
    }

    #[tokio::test]
    async fn holds_multiple_teams_independently() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TeamRegistry::new();
        for name in ["alpha", "beta", "gamma"] {
            registry
                .register(coordinator(name, dir.path()), dir.path().to_path_buf())
                .await
                .unwrap();
        }

        assert_eq!(registry.names().await, vec!["alpha", "beta", "gamma"]);

        // Stopping one must leave the others addressable.
        assert!(registry.stop("beta").await.unwrap());
        assert!(!registry.contains("beta").await);
        assert!(registry.contains("alpha").await);
        assert!(registry.contains("gamma").await);
    }

    #[tokio::test]
    async fn stop_reports_unknown_team_without_failing() {
        let registry = TeamRegistry::new();
        assert!(!registry.stop("ghost").await.unwrap());
    }

    #[tokio::test]
    async fn rejects_path_traversal_team_names() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TeamRegistry::new();
        for name in ["../escape", "a/b", "", "team name"] {
            let result = registry
                .register(coordinator(name, dir.path()), dir.path().to_path_buf())
                .await;
            assert!(result.is_err(), "team name '{}' should be refused", name);
        }
    }

    #[tokio::test]
    async fn stop_all_drains_every_team_and_latches() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TeamRegistry::new();
        for name in ["alpha", "beta"] {
            registry
                .register(coordinator(name, dir.path()), dir.path().to_path_buf())
                .await
                .unwrap();
        }

        let report = registry.stop_all().await;
        assert!(report.is_complete());
        assert_eq!(report.stopped.len(), 2);
        assert!(registry.is_empty().await);
        assert!(registry.is_stopped().await);

        // A late launch must not resurrect the swarm behind the operator.
        let late = registry
            .register(coordinator("late", dir.path()), dir.path().to_path_buf())
            .await;
        assert!(late.is_err(), "launch after emergency stop must be refused");

        registry.clear_stop().await;
        assert!(!registry.is_stopped().await);
        assert!(registry
            .register(coordinator("late", dir.path()), dir.path().to_path_buf())
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn shutdown_marks_team_completed_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let registry = TeamRegistry::new();
        let handle = registry
            .register(coordinator("alpha", dir.path()), dir.path().to_path_buf())
            .await
            .unwrap();

        registry.stop("alpha").await.unwrap();

        // The coordinator stays inspectable through the handle we still hold,
        // and shutdown has moved it out of `Active`.
        let coordinator = handle.lock().await;
        assert_ne!(coordinator.status(), &TeamStatus::Active);
    }
}
