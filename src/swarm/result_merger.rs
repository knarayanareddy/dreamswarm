use crate::swarm::{MergeStrategy, WorkerInfo};
use tokio::process::Command;

pub struct ResultMerger {
    repo_root: String,
}

#[derive(Debug)]
pub struct MergeReport {
    pub strategy: MergeStrategy,
    pub workers_merged: Vec<String>,
    pub conflicts: Vec<MergeConflict>,
    pub success: bool,
    pub summary: String,
}

#[derive(Debug)]
pub struct MergeConflict {
    pub worker_name: String,
    pub branch: String,
    pub conflicting_files: Vec<String>,
}

impl ResultMerger {
    pub fn new(repo_root: &str) -> Self {
        Self {
            repo_root: repo_root.to_string(),
        }
    }

    pub async fn merge(
        &self,
        workers: &[WorkerInfo],
        strategy: &MergeStrategy,
        target_branch: &str,
    ) -> anyhow::Result<MergeReport> {
        let completed_workers: Vec<&WorkerInfo> = workers
            .iter()
            .filter(|w| w.branch_name.is_some())
            .collect();
        if completed_workers.is_empty() {
            return Ok(MergeReport {
                strategy: strategy.clone(),
                workers_merged: vec![],
                conflicts: vec![],
                success: true,
                summary: "No worker branches to merge.".to_string(),
            });
        }

        match strategy {
            MergeStrategy::CherryPick => self.cherry_pick_merge(&completed_workers, target_branch).await,
            MergeStrategy::Sequential => self.sequential_merge(&completed_workers, target_branch).await,
            MergeStrategy::LeadReview | MergeStrategy::Manual => self.generate_review_diffs(&completed_workers, target_branch).await,
            MergeStrategy::OctopusMerge => self.octopus_merge(&completed_workers, target_branch).await,
        }
    }

    async fn cherry_pick_merge(&self, workers: &[&WorkerInfo], target_branch: &str) -> anyhow::Result<MergeReport> {
        let mut merged = Vec::new();
        let mut conflicts = Vec::new();
        let _ = self.git_cmd(&["checkout", target_branch]).await?;

        for worker in workers {
            let branch = worker.branch_name.as_ref().unwrap();
            let rev_list = self.git_cmd(&["rev-list", "--reverse", &format!("{}..{}", target_branch, branch)]).await?;
            let mut worker_conflicts = Vec::new();

            for hash in rev_list.lines() {
                let hash = hash.trim();
                if hash.is_empty() { continue; }
                let result = Command::new("git")
                    .args(["cherry-pick", "--no-commit", hash])
                    .current_dir(&self.repo_root)
                    .output()
                    .await?;

                if !result.status.success() {
                    let status = self.git_cmd(&["diff", "--name-only", "--diff-filter=U"]).await?;
                    let files: Vec<String> = status.lines().map(|l| l.trim().to_string()).collect();
                    worker_conflicts.extend(files.clone());
                    let _ = self.git_cmd(&["cherry-pick", "--abort"]).await.ok();
                    conflicts.push(MergeConflict {
                        worker_name: worker.name.clone(),
                        branch: branch.clone(),
                        conflicting_files: files,
                    });
                } else {
                    let _ = self.git_cmd(&["commit", "--no-edit", "-m", &format!("[dreamswarm:{}] cherry-pick from {}", worker.name, &hash[..7])]).await?;
                }
            }
            if worker_conflicts.is_empty() {
                merged.push(worker.name.clone());
            }
        }

        let success = conflicts.is_empty();
        let summary = if success {
            format!("Successfully merged {} workers via cherry-pick.", merged.len())
        } else {
            format!("Merged {} workers, {} had conflicts.", merged.len(), conflicts.len())
        };

        Ok(MergeReport {
            strategy: MergeStrategy::CherryPick,
            workers_merged: merged,
            conflicts,
            success,
            summary,
        })
    }

    async fn sequential_merge(&self, workers: &[&WorkerInfo], target_branch: &str) -> anyhow::Result<MergeReport> {
        let mut merged = Vec::new();
        let mut conflicts = Vec::new();
        let _ = self.git_cmd(&["checkout", target_branch]).await?;

        for worker in workers {
            let branch = worker.branch_name.as_ref().unwrap();
            let result = Command::new("git")
                .args(["merge", "--no-ff", "-m", &format!("[dreamswarm] Merge worker '{}' ({})", worker.name, branch), branch])
                .current_dir(&self.repo_root)
                .output()
                .await?;

            if result.status.success() {
                merged.push(worker.name.clone());
            } else {
                let status = self.git_cmd(&["diff", "--name-only", "--diff-filter=U"]).await?;
                let files: Vec<String> = status.lines().map(|l| l.trim().to_string()).collect();
                let _ = self.git_cmd(&["merge", "--abort"]).await.ok();
                conflicts.push(MergeConflict {
                    worker_name: worker.name.clone(),
                    branch: branch.clone(),
                    conflicting_files: files,
                });
            }
        }
        Ok(MergeReport {
            strategy: MergeStrategy::Sequential,
            workers_merged: merged,
            conflicts,
            success: true,
            summary: "Sequential merge complete.".to_string(),
        })
    }

    async fn octopus_merge(&self, workers: &[&WorkerInfo], target_branch: &str) -> anyhow::Result<MergeReport> {
        let _ = self.git_cmd(&["checkout", target_branch]).await?;
        let branches: Vec<&str> = workers.iter().filter_map(|w| w.branch_name.as_deref()).collect();
        let mut args = vec!["merge", "--no-ff", "-m", "[dreamswarm] Octopus merge"];
        args.extend(branches.iter());

        let result = Command::new("git").args(&args).current_dir(&self.repo_root).output().await?;
        if result.status.success() {
            Ok(MergeReport {
                strategy: MergeStrategy::OctopusMerge,
                workers_merged: workers.iter().map(|w| w.name.clone()).collect(),
                conflicts: vec![],
                success: true,
                summary: format!("Octopus merge of {} branches succeeded.", workers.len()),
            })
        } else {
            let _ = self.git_cmd(&["merge", "--abort"]).await.ok();
            self.sequential_merge(workers, target_branch).await
        }
    }

    async fn generate_review_diffs(&self, workers: &[&WorkerInfo], target_branch: &str) -> anyhow::Result<MergeReport> {
        let mut summary = String::from("## Worker Diffs for Review\n\n");
        for worker in workers {
            let branch = worker.branch_name.as_ref().unwrap();
            let diff = self.git_cmd(&["diff", "--stat", target_branch, branch]).await?;
            let commit_log = self.git_cmd(&["log", "--oneline", &format!("{}..{}", target_branch, branch)]).await?;
            summary.push_str(&format!("### Worker: {} ({})\n", worker.name, branch));
            summary.push_str(&format!("Commits:\n```\n{}\n```\n", commit_log));
            summary.push_str(&format!("Changes:\n```\n{}\n```\n\n", diff));
        }
        Ok(MergeReport {
            strategy: MergeStrategy::LeadReview,
            workers_merged: vec![],
            conflicts: vec![],
            success: true,
            summary,
        })
    }

    async fn git_cmd(&self, args: &[&str]) -> anyhow::Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_root)
            .output()
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git {} failed: {}", args.join(" "), stderr);
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
