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
        let mailbox = mailbox.clone();
        let tool = tool.to_string();
        let input = input.clone();
        // The approval queue is a file-backed mailbox; enqueue the request and
        // report success. The agent loop performs the *waiting* asynchronously
        // (see `request_human_approval`), so this never blocks a runtime.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.block_on(async move {
                enqueue_approval_request(mailbox, &tool, &input).await
            }),
            Err(_) => Err(anyhow::anyhow!("no async runtime available for approval")),
        }
    })
}

/// Posts an approval request to the team lead without waiting.
///
/// The worker's `run_worker_turn` uses this as its `AutoApprover`; the lead (or
/// the dashboard) answers asynchronously with `ApprovalResponse`.
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
            let messages = mbox.receive().unwrap_or_default();
            for message in messages {
                match message.content {
                    MessageContent::ApprovalResponse { approved, reason } => {
                        // Responses from the lead are attributed to this
                        // request; the mailbox is per-agent so ordering is
                        // preserved. A response is only accepted once.
                        if !approved {
                            tracing::warn!(
                                "Approval denied for '{}': {}",
                                tool,
                                reason.unwrap_or_else(|| "no reason given".into())
                            );
                        }
                        return approved;
                    }
                    other => {
                        // Put non-approval traffic back so it is not lost.
                        if let Err(e) = mbox.send("self-requeue", other.clone()) {
                            tracing::debug!("Could not requeue message: {}", e);
                        }
                    }
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
