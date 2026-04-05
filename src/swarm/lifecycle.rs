use crate::swarm::mailbox::Mailbox;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use tokio::process::Command;

static REGISTERED_TEAMS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

pub fn register_team_for_cleanup(team_name: &str) {
    let mut teams = REGISTERED_TEAMS.lock().unwrap();
    if teams.is_none() {
        *teams = Some(HashSet::new());
        register_signal_handlers();
    }
    teams.as_mut().unwrap().insert(team_name.to_string());
    tracing::info!("Registered team '{}' for session cleanup", team_name);
}

pub fn unregister_team(team_name: &str) {
    let mut teams = REGISTERED_TEAMS.lock().unwrap();
    if let Some(ref mut set) = *teams {
        set.remove(team_name);
    }
}

fn register_signal_handlers() {
    let _ = ctrlc::set_handler(move || {
        eprintln!("\nReceived interrupt signal — cleaning up teams...");
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            cleanup_all_teams().await;
        });
        std::process::exit(0);
    });
}

pub async fn cleanup_all_teams() {
    let teams: Vec<String> = {
        let guard = REGISTERED_TEAMS.lock().unwrap();
        match &*guard {
            Some(set) => set.iter().cloned().collect(),
            None => vec![],
        }
    };

    for team_name in &teams {
        tracing::info!("Cleaning up team '{}'", team_name);
        kill_team_tmux_panes(team_name).await;
        cleanup_team_worktrees(team_name).await;
        cleanup_team_directory(team_name);
        let _ = Mailbox::cleanup_team(team_name);
    }
}

async fn kill_team_tmux_panes(_team_name: &str) {
    let output = Command::new("tmux")
        .args(["list-panes", "-a", "-F", "#{pane_id} #{pane_title}"])
        .output()
        .await;
    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("Worker:") || line.contains("dreamswarm") {
                if let Some(pane_id) = line.split_whitespace().next() {
                    let _ = Command::new("tmux")
                        .args(["kill-pane", "-t", pane_id])
                        .output()
                        .await;
                }
            }
        }
    }
}

async fn cleanup_team_worktrees(_team_name: &str) {
    let worktrees_dir = std::env::current_dir()
        .unwrap_or_default()
        .join(".dreamswarm-worktrees");
    if worktrees_dir.exists() {
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .output()
            .await;
        let _ = std::fs::remove_dir_all(&worktrees_dir);
    }
}

fn cleanup_team_directory(team_name: &str) {
    let team_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".dreamswarm")
        .join("teams")
        .join(team_name);
    if team_dir.exists() {
        let _ = std::fs::remove_dir_all(&team_dir);
    }
}

pub fn list_active_teams() -> Vec<String> {
    let teams_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".dreamswarm")
        .join("teams");
    if !teams_dir.exists() {
        return vec![];
    }
    std::fs::read_dir(&teams_dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}
