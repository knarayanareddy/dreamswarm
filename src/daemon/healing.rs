//! Guarded self-healing.
//!
//! The previous implementation ran in the user's checkout: it created a branch
//! in place, ran `cargo fix --allow-dirty`, checked the result, merged into a
//! hard-coded `main` and deleted the branch — ignoring every exit status and
//! returning `Ok(true)` even when nothing succeeded. That can corrupt unrelated
//! work and report a "repair" that never happened.
//!
//! Healing now:
//! 1. runs in a throwaway git worktree attached to a new branch;
//! 2. checks every command and distinguishes "nothing to fix" from failure;
//! 3. verifies with the full test suite (not just `cargo check`);
//! 4. never merges. It returns the branch and worktree so a human (or the
//!    caller) can review and merge explicitly.

use crate::tools::synthesis::ToolSynthesisEngine;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Outcome of one healing attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealOutcome {
    /// A candidate patch is ready for review.
    PatchReady {
        branch: String,
        worktree: PathBuf,
        changed_files: usize,
    },
    /// The checks already pass: there is nothing to heal.
    NothingToDo,
    /// A repair was attempted and rejected.
    Unsuccessful { reason: String },
}

impl HealOutcome {
    /// Conservative boolean: true only when a reviewable patch exists.
    pub fn succeeded(&self) -> bool {
        matches!(self, HealOutcome::PatchReady { .. })
    }
}

pub struct HealingManager {
    working_dir: PathBuf,
    state_dir: PathBuf,
    _synthesis_engine: ToolSynthesisEngine,
}

impl HealingManager {
    pub fn new(working_dir: PathBuf, state_dir: PathBuf) -> Self {
        Self {
            working_dir,
            _synthesis_engine: ToolSynthesisEngine::new(state_dir.clone()),
            state_dir,
        }
    }

    /// Attempts to build a repair candidate for `error_signal`.
    ///
    /// The user's working tree is never modified; the candidate lives in a
    /// separate worktree under the state directory.
    pub async fn attempt_self_heal(&self, error_signal: &str) -> anyhow::Result<HealOutcome> {
        tracing::warn!(
            "Self-healing requested for: '{}' (staging in an isolated worktree)",
            error_signal
        );

        if !self.is_git_repo(&self.working_dir).await {
            return Ok(HealOutcome::Unsuccessful {
                reason: "working directory is not a git repository".to_string(),
            });
        }

        let base_branch = match self.current_branch().await {
            Ok(branch) if !branch.is_empty() => branch,
            Ok(_) => {
                return Ok(HealOutcome::Unsuccessful {
                    reason: "repository is in detached HEAD state".to_string(),
                })
            }
            Err(e) => {
                return Ok(HealOutcome::Unsuccessful {
                    reason: format!("cannot determine branch: {}", e),
                })
            }
        };

        let branch = format!("heal/{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
        let worktree = self
            .state_dir
            .join("healing")
            .join(branch.replace('/', "-"));

        if let Some(parent) = worktree.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 1. Isolate. `--no-track` prevents an accidental push upstream.
        let worktree_str = worktree
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("state directory path is not valid UTF-8"))?
            .to_string();
        let created = self
            .run_owned(
                &self.working_dir,
                "git",
                vec![
                    "worktree".to_string(),
                    "add".to_string(),
                    "--no-track".to_string(),
                    "-b".to_string(),
                    branch.clone(),
                    worktree_str,
                    base_branch,
                ],
            )
            .await?;
        if !created.success {
            return Ok(HealOutcome::Unsuccessful {
                reason: format!("could not create healing worktree: {}", created.stderr),
            });
        }

        let outcome = self
            .heal_in_worktree(&worktree, &branch, error_signal)
            .await;

        match outcome {
            Ok(HealOutcome::Unsuccessful { reason }) => {
                // Leave no debris behind, and never keep a broken branch.
                self.remove_worktree(&worktree, &branch).await;
                Ok(HealOutcome::Unsuccessful { reason })
            }
            other => other,
        }
    }

    async fn heal_in_worktree(
        &self,
        worktree: &Path,
        branch: &str,
        _error_signal: &str,
    ) -> anyhow::Result<HealOutcome> {
        // 2. Verify the baseline first: if it is already green there is no
        //    repair to make, and running `cargo fix` would be gratuitous.
        let baseline = self.run(worktree, "cargo", &["check", "--quiet"]).await?;
        if baseline.success {
            return Ok(HealOutcome::NothingToDo);
        }

        // 3. Produce a candidate patch. Failures here are real failures; the
        //    old code ignored them and claimed success.
        let fmt = self.run(worktree, "cargo", &["fmt"]).await?;
        if !fmt.success {
            return Ok(HealOutcome::Unsuccessful {
                reason: format!("cargo fmt failed: {}", fmt.stderr),
            });
        }

        let fix = self
            .run(
                worktree,
                "cargo",
                &["fix", "--allow-dirty", "--allow-staged"],
            )
            .await?;
        if !fix.success {
            return Ok(HealOutcome::Unsuccessful {
                reason: format!("cargo fix failed: {}", fix.stderr),
            });
        }

        // 4. Verify with the test suite; `cargo check` alone accepted code that
        //    compiled but behaved incorrectly.
        let tests = self.run(worktree, "cargo", &["test", "--quiet"]).await?;
        if !tests.success {
            return Ok(HealOutcome::Unsuccessful {
                reason: "candidate patch did not pass the test suite".to_string(),
            });
        }

        let diff = self
            .run(worktree, "git", &["status", "--porcelain"])
            .await?;
        let changed_files = diff
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        if changed_files == 0 {
            return Ok(HealOutcome::Unsuccessful {
                reason: "tests pass but no changes were produced".to_string(),
            });
        }

        let commit = self
            .run(
                worktree,
                "git",
                &[
                    "commit",
                    "-am",
                    "chore: automated healing candidate [dreamswarm]",
                ],
            )
            .await?;
        if !commit.success {
            return Ok(HealOutcome::Unsuccessful {
                reason: format!("could not commit candidate: {}", commit.stderr),
            });
        }

        tracing::info!(
            "Healing candidate ready on branch '{}' ({} files). Merge remains a human decision.",
            branch,
            changed_files
        );
        Ok(HealOutcome::PatchReady {
            branch: branch.to_string(),
            worktree: worktree.to_path_buf(),
            changed_files,
        })
    }

    async fn remove_worktree(&self, worktree: &Path, branch: &str) {
        let path = worktree.to_string_lossy().to_string();
        let _ = self
            .run_owned(
                &self.working_dir,
                "git",
                vec![
                    "worktree".to_string(),
                    "remove".to_string(),
                    "--force".to_string(),
                    path,
                ],
            )
            .await;
        let _ = self
            .run(&self.working_dir, "git", &["branch", "-D", branch])
            .await;
    }

    async fn current_branch(&self) -> anyhow::Result<String> {
        let result = self
            .run(&self.working_dir, "git", &["branch", "--show-current"])
            .await?;
        if !result.success {
            anyhow::bail!("{}", result.stderr);
        }
        Ok(result.stdout.trim().to_string())
    }

    async fn is_git_repo(&self, dir: &Path) -> bool {
        matches!(
            self.run(dir, "git", &["rev-parse", "--is-inside-work-tree"]).await,
            Ok(result) if result.success
        )
    }

    async fn run(&self, dir: &Path, program: &str, args: &[&str]) -> anyhow::Result<CommandResult> {
        self.run_owned(
            dir,
            program,
            args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        )
        .await
    }

    async fn run_owned(
        &self,
        dir: &Path,
        program: &str,
        args: Vec<String>,
    ) -> anyhow::Result<CommandResult> {
        let output = Command::new(program)
            .args(args.iter())
            .current_dir(dir)
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("failed to run {} {:?}: {}", program, args, e))?;
        Ok(CommandResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

struct CommandResult {
    success: bool,
    stdout: String,
    stderr: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn non_git_directory_is_not_healed() {
        let dir = tempdir().unwrap();
        let manager = HealingManager::new(dir.path().to_path_buf(), dir.path().to_path_buf());
        let outcome = manager.attempt_self_heal("compiler error").await.unwrap();
        assert_eq!(
            outcome,
            HealOutcome::Unsuccessful {
                reason: "working directory is not a git repository".to_string()
            }
        );
        assert!(!outcome.succeeded());
    }

    #[tokio::test]
    async fn detached_head_is_reported_not_patched() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let manager = HealingManager::new(repo.clone(), dir.path().join("state"));

        let setups: Vec<Vec<&str>> = vec![
            vec!["init"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
        ];
        for args in setups {
            let output = Command::new("git")
                .args(&args)
                .current_dir(&repo)
                .output()
                .await
                .unwrap();
            assert!(output.status.success());
        }
        std::fs::write(repo.join("a.txt"), "x").unwrap();
        let commits: Vec<Vec<&str>> = vec![vec!["add", "a.txt"], vec!["commit", "-m", "init"]];
        for args in commits {
            let output = Command::new("git")
                .args(&args)
                .current_dir(&repo)
                .output()
                .await
                .unwrap();
            assert!(output.status.success());
        }
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .await
            .unwrap();
        let sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
        let checkout = Command::new("git")
            .args(["checkout", &sha])
            .current_dir(&repo)
            .output()
            .await
            .unwrap();
        assert!(checkout.status.success());

        let outcome = manager.attempt_self_heal("anything").await.unwrap();
        assert!(matches!(outcome, HealOutcome::Unsuccessful { .. }));
        // The user's checkout must still be on the same commit.
        let verify = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .await
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&verify.stdout).trim(), sha);
    }
}
