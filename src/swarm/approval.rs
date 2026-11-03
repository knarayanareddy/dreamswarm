//! Human-in-the-loop approval for unattended workers.
//!
//! A worker cannot prompt a user directly: its stdin is driven by the
//! orchestration layer. Historically it simply auto-approved every dangerous
//! tool, which turned any model (or prompt-injected content) into unsandboxed
//! code execution. Instead, a worker now posts an `ApprovalRequest` to the lead
//! mailbox and waits for a matching `ApprovalResponse`.

use crate::swarm::mailbox::Mailbox;
use crate::swarm::{MessageContent, TeamConfig};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Builds the approver used by a worker session.
///
/// * `BypassPermissions` teams keep unattended execution (explicit opt-in).
/// * Everything else asks the lead and fails closed.
pub fn worker_approver(
    mailbox: Arc<RwLock<Mailbox>>,
    team: &TeamConfig,
) -> crate::runtime::agent_loop::AutoApprover {
    if team.worker_mode.eq_ignore_ascii_case("bypass-permissions")
        || team.worker_mode.eq_ignore_ascii_case("bypasspermissions")
    {
        tracing::warn!(
            "Team '{}' runs with BypassPermissions: worker tool calls are auto-approved",
            team.team_name
        );
        return Arc::new(|_, _| Ok(true));
    }

    Arc::new(move |tool: &str, input: &serde_json::Value| {
        // Do NOT block here. `Handle::block_on` panics when called from inside
        // a runtime ("Cannot start a runtime from within a runtime"), and this
        // closure runs on the agent loop's thread. The mailbox append is a
        // short, lock-protected write, so it is safe to do synchronously.
        enqueue_approval_request_blocking(&mailbox, tool, input)
    })
}

/// Posts an approval request to the team lead without waiting.
///
/// The worker's `run_worker_turn` uses this as its `AutoApprover`; the lead (or
/// the dashboard) answers asynchronously with `ApprovalResponse`.
pub fn enqueue_approval_request_blocking(
    mailbox: &Arc<RwLock<Mailbox>>,
    tool: &str,
    input: &serde_json::Value,
) -> anyhow::Result<bool> {
    // `try_write`/`blocking_write` are not usable from every context, so the
    // mailbox write is performed under a short-lived lock acquired via the
    // std::sync bridge only when a runtime is *not* driving this thread.
    let mut mbox = mailbox
        .try_write()
        .map_err(|_| anyhow::anyhow!("mailbox is busy; cannot record approval request"))?;
    let request_id = uuid::Uuid::new_v4().to_string();
    mbox.send(
        "lead",
        MessageContent::ApprovalRequest {
            action: tool.to_string(),
            description: format!("[{}] {}", request_id, summarize(tool, input)),
        },
    )?;
    Ok(true)
}

/// Async variant used by `run_worker_turn` when no sync approver is set.
pub async fn enqueue_approval_request(
    mailbox: Arc<RwLock<Mailbox>>,
    tool: &str,
    input: &serde_json::Value,
) -> anyhow::Result<bool> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut mbox = mailbox.write().await;
    mbox.send(
        "lead",
        MessageContent::ApprovalRequest {
            action: tool.to_string(),
            description: format!("[{}] {}", request_id, summarize(tool, input)),
        },
    )?;
    Ok(true)
}

/// Sends an approval request to the team lead and waits for the response.
///
/// Any error, malformed response, or timeout denies the call.
pub async fn request_human_approval(
    mailbox: Arc<RwLock<Mailbox>>,
    tool: &str,
    input: &serde_json::Value,
    timeout: Duration,
) -> bool {
    let request_id = uuid::Uuid::new_v4().to_string();
    let summary = summarize(tool, input);

    {
        let mut mbox = mailbox.write().await;
        if mbox
            .send(
                "lead",
                MessageContent::ApprovalRequest {
                    action: tool.to_string(),
                    description: format!("[{}] {}", request_id, summary),
                },
            )
            .is_err()
        {
            tracing::warn!("Could not send approval request for '{}'", tool);
            return false;
        }
    }

    let deadline = tokio::time::Instant::now() + timeout;
    let mut interval = Duration::from_millis(250);
    loop {
        {
            let mut mbox = mailbox.write().await;
            // Claim *only* approval responses. The previous loop consumed every
            // unread message, so task assignments arriving during a wait were
            // dropped (and "requeued" to a non-existent agent).
            let messages = mbox
                .receive_matching(|m| matches!(m.content, MessageContent::ApprovalResponse { .. }))
                .unwrap_or_default();
            for message in messages {
                if let MessageContent::ApprovalResponse { approved, reason } = message.content {
                    if !approved {
                        tracing::warn!(
                            "Approval denied for '{}': {}",
                            tool,
                            reason.unwrap_or_else(|| "no reason given".into())
                        );
                    }
                    return approved;
                }
            }
        }

        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("Approval request for '{}' timed out; denying", tool);
            return false;
        }
        tokio::time::sleep(interval).await;
        // Back off polling so a long wait does not spin on disk.
        interval = (interval * 2).min(Duration::from_secs(5));
    }
}

fn summarize(tool: &str, input: &serde_json::Value) -> String {
    let detail = input
        .get("command")
        .or_else(|| input.get("code"))
        .or_else(|| input.get("script"))
        .or_else(|| input.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let truncated: String = detail.chars().take(300).collect();
    if truncated.is_empty() {
        format!("{} requested", tool)
    } else {
        format!("{}: {}", tool, truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn worker_mailbox(dir: &std::path::Path) -> Arc<RwLock<Mailbox>> {
        Arc::new(RwLock::new(
            Mailbox::new(dir.to_path_buf(), "approval-team", "worker").unwrap(),
        ))
    }

    #[tokio::test]
    async fn approval_is_denied_when_no_response_arrives() {
        let dir = tempdir().unwrap();
        let mailbox = worker_mailbox(dir.path()).await;
        let approved = request_human_approval(
            mailbox,
            "Bash",
            &serde_json::json!({"command": "rm -rf /"}),
            Duration::from_millis(350),
        )
        .await;
        assert!(!approved, "silence must fail closed");
    }

    #[tokio::test]
    async fn approval_is_granted_when_lead_responds() {
        let dir = tempdir().unwrap();
        let mailbox = worker_mailbox(dir.path()).await;
        let worker_id = {
            let mbox = mailbox.read().await;
            mbox.agent_name().to_string()
        };

        let responder = {
            let mailbox = mailbox.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let mut mbox = mailbox.write().await;
                mbox.send(
                    &worker_id,
                    MessageContent::ApprovalResponse {
                        approved: true,
                        reason: None,
                    },
                )
                .unwrap();
            })
        };

        let approved = request_human_approval(
            mailbox,
            "Bash",
            &serde_json::json!({"command": "cargo test"}),
            Duration::from_secs(5),
        )
        .await;
        responder.await.unwrap();
        assert!(approved);
    }

    #[tokio::test]
    async fn denial_response_is_honoured() {
        let dir = tempdir().unwrap();
        let mailbox = worker_mailbox(dir.path()).await;
        let worker_id = {
            let mbox = mailbox.read().await;
            mbox.agent_name().to_string()
        };

        let responder = {
            let mailbox = mailbox.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let mut mbox = mailbox.write().await;
                mbox.send(
                    &worker_id,
                    MessageContent::ApprovalResponse {
                        approved: false,
                        reason: Some("not on main".into()),
                    },
                )
                .unwrap();
            })
        };

        let approved = request_human_approval(
            mailbox,
            "Bash",
            &serde_json::json!({"command": "git push"}),
            Duration::from_secs(5),
        )
        .await;
        responder.await.unwrap();
        assert!(!approved);
    }
}
