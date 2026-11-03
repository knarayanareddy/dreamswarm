//! Process-level teardown for teams that outlive the request that started them.
//!
//! `register_team_for_cleanup` and its friends used to be the only cleanup
//! path, but nothing ever called them, and `cleanup_all_teams` guessed at the
//! state directory (`~/.dreamswarm`) instead of using the configured one. A
//! Ctrl-C therefore left tmux panes, worktrees and inboxes behind.
//!
//! The registry owns the real handles, so shutdown now goes through it. What
//! remains here is the parts the registry deliberately does not do: sweeping
//! tmux panes and worktrees that belong to teams whose coordinator is already
//! gone.

use std::path::PathBuf;
use tokio::process::Command;

/// Best-effort removal of worktrees and panes left behind by finished teams.
///
/// `teams` are the names that were running before shutdown. It is
/// intentionally forgiving: every step logs and continues, because this runs
/// during process exit and a panic here would skip the remaining cleanup.
pub async fn sweep_leftovers(teams: &[String], state_dir: &PathBuf) {
    for team in teams {
        tracing::info!("Sweeping leftovers for team '{}'", team);
        remove_team_inboxes(state_dir, team);
        remove_team_state(state_dir, team);
    }
    prune_worktrees().await;
    kill_orphan_panes().await;
}

fn remove_team_inboxes(state_dir: &PathBuf, team_name: &str) {
    let inboxes = state_dir.join("teams").join(team_name).join("inboxes");
    if inboxes.exists() {
        if let Err(e) = std::fs::remove_dir_all(&inboxes) {
            tracing::warn!("Could not remove inboxes for '{}': {}", team_name, e);
        }
    }
}

fn remove_team_state(state_dir: &PathBuf, team_name: &str) {
    let team_dir = state_dir.join("teams").join(team_name);
    if team_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&team_dir) {
            tracing::warn!("Could not remove state for '{}': {}", team_name, e);
        }
    }
}

/// Prunes worktree metadata, then removes the worktrees directory itself.
async fn prune_worktrees() {
    let worktrees_dir = std::env::current_dir()
        .unwrap_or_default()
        .join(".dreamswarm-worktrees");
    if !worktrees_dir.exists() {
        return;
    }

    let pruned = Command::new("git")
        .args(["worktree", "prune"])
        .output()
        .await;
    match pruned {
        Ok(output) if !output.status.success() => tracing::warn!(
            "git worktree prune failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(e) => tracing::warn!("Could not run git worktree prune: {}", e),
        _ => {}
    }

    if let Err(e) = std::fs::remove_dir_all(&worktrees_dir) {
        tracing::warn!("Could not remove worktree directory: {}", e);
    }
}

/// Kills the tmux panes worker spawns create.
///
/// Only panes whose title marks them as workers are touched; the operator's own
/// panes in the same session are left alone.
async fn kill_orphan_panes() {
    let listed = Command::new("tmux")
        .args(["list-panes", "-a", "-F", "#{pane_id} #{pane_title}"])
        .output()
        .await;

    let Ok(listed) = listed else {
        // No tmux, or not inside a session: nothing to sweep.
        return;
    };

    for line in String::from_utf8_lossy(&listed.stdout).lines() {
        let is_worker_pane = line.starts_with("Worker:") || line.contains(" Worker:");
        if !is_worker_pane && !line.contains("dreamswarm") {
            continue;
        }
        if let Some(pane_id) = line.split_whitespace().next() {
            let killed = Command::new("tmux")
                .args(["kill-pane", "-t", pane_id])
                .output()
                .await;
            match killed {
                Ok(output) if !output.status.success() => tracing::warn!(
                    "Could not kill pane {}: {}",
                    pane_id,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
                Err(e) => tracing::warn!("Could not kill pane {}: {}", pane_id, e),
                _ => tracing::debug!("Killed leftover pane {}", pane_id),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sweep_is_safe_when_nothing_exists() {
        let dir = tempfile::tempdir().unwrap();
        // A missing state dir and no tmux panes must not panic or hang.
        sweep_leftovers(&["ghost-team".to_string()], &dir.path().to_path_buf()).await;
    }

    #[tokio::test]
    async fn sweep_removes_team_state_under_the_given_dir() {
        let dir = tempfile::tempdir().unwrap();
        let team_dir = dir.path().join("teams").join("alpha");
        std::fs::create_dir_all(team_dir.join("inboxes")).unwrap();
        std::fs::write(team_dir.join("state.json"), "{}").unwrap();

        sweep_leftovers(&["alpha".to_string()], &dir.path().to_path_buf()).await;

        assert!(
            !team_dir.exists(),
            "leftover state should be removed for the configured directory"
        );
    }
}
