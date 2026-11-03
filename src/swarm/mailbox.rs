use crate::swarm::file_lock::{append_locked, write_atomic, FileLock};
use crate::swarm::{AgentMessage, MessageContent};
use chrono::Utc;
use std::path::PathBuf;

pub struct Mailbox {
    inboxes_dir: PathBuf,
    agent_name: String,
    relay_dir: Option<PathBuf>,
    pub poll_count: u64,
}

impl Mailbox {
    pub fn new(base_dir: PathBuf, team_name: &str, agent_name: &str) -> anyhow::Result<Self> {
        let inboxes_dir = base_dir
            .join("teams")
            .join(Self::sanitize(team_name))
            .join("inboxes");
        std::fs::create_dir_all(&inboxes_dir)?;

        let home = dirs::home_dir();
        let relay_dir = home.map(|h| h.join(".dreamswarm").join("relay").join("inboxes"));
        if let Some(ref rd) = relay_dir {
            let _ = std::fs::create_dir_all(rd);
        }

        Ok(Self {
            inboxes_dir,
            agent_name: agent_name.to_string(),
            relay_dir,
            poll_count: 0,
        })
    }

    /// Name of the agent that owns this mailbox.
    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    pub fn send(&self, to: &str, content: MessageContent) -> anyhow::Result<()> {
        let msg = AgentMessage {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            from: self.agent_name.clone(),
            to: to.to_string(),
            content,
            timestamp: Utc::now(),
            read: false,
        };
        let inbox_path = self
            .inboxes_dir
            .join(format!("{}.jsonl", Self::sanitize(to)));

        Self::write_to_path(&inbox_path, &msg)?;

        // Global Relay sync: If the message is directed at "coordinator" or "global",
        // we mirror it to the relay directory so other repo swarms can see it.
        if to == "lead" || to == "global" {
            if let Some(ref rd) = self.relay_dir {
                let relay_path = rd.join("global_relay.jsonl");
                let _ = Self::write_to_path(&relay_path, &msg);
            }
        }

        Ok(())
    }

    /// Appends one message under an exclusive lock.
    ///
    /// Unlocked appends from several worker processes could interleave partial
    /// lines, which `receive` then silently discarded as invalid JSON.
    fn write_to_path(path: &PathBuf, msg: &AgentMessage) -> anyhow::Result<()> {
        let line = serde_json::to_string(msg)?;
        append_locked(path, format!("{}\n", line).as_bytes())
    }

    pub fn send_chat(&self, to: &str, text: &str) -> anyhow::Result<()> {
        self.send(
            to,
            MessageContent::Chat {
                text: text.to_string(),
            },
        )
    }

    pub fn send_task_assignment(
        &self,
        to: &str,
        task_id: &str,
        instructions: &str,
    ) -> anyhow::Result<()> {
        self.send(
            to,
            MessageContent::TaskAssignment {
                task_id: task_id.to_string(),
                instructions: instructions.to_string(),
            },
        )
    }

    pub fn send_task_result(&self, to: &str, task_id: &str, result: &str) -> anyhow::Result<()> {
        self.send(
            to,
            MessageContent::TaskResult {
                task_id: task_id.to_string(),
                result: result.to_string(),
            },
        )
    }

    pub fn send_shutdown(&self, to: &str) -> anyhow::Result<()> {
        self.send(to, MessageContent::ShutdownRequest)
    }

    pub fn send_help_request(&self, to: &str, request_id: &str, task: &str) -> anyhow::Result<()> {
        self.send(
            to,
            MessageContent::HelpRequest {
                request_id: request_id.to_string(),
                task: task.to_string(),
            },
        )
    }

    pub fn send_help_response(
        &self,
        to: &str,
        request_id: &str,
        result: &str,
    ) -> anyhow::Result<()> {
        self.send(
            to,
            MessageContent::HelpResponse {
                request_id: request_id.to_string(),
                result: result.to_string(),
            },
        )
    }

    fn inbox_path(&self) -> PathBuf {
        self.inboxes_dir
            .join(format!("{}.jsonl", Self::sanitize(&self.agent_name)))
    }

    /// Takes messages matching `predicate`, marking only those read.
    ///
    /// The read/rewrite cycle holds the mailbox lock: previously a sender could
    /// append between the read and the rewrite, and the rewrite published from
    /// the stale snapshot — silently dropping that message.
    fn take_matching<F>(&mut self, predicate: F) -> anyhow::Result<Vec<AgentMessage>>
    where
        F: Fn(&AgentMessage) -> bool,
    {
        self.poll_count += 1;
        let inbox_path = self.inbox_path();
        if !inbox_path.exists() {
            return Ok(vec![]);
        }

        let _lock = FileLock::for_target(&inbox_path)?;
        let content = std::fs::read_to_string(&inbox_path)?;
        let mut all_messages: Vec<AgentMessage> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        let mut claimed = Vec::new();
        for msg in &mut all_messages {
            if !msg.read && predicate(msg) {
                msg.read = true;
                claimed.push(msg.clone());
            }
        }
        if claimed.is_empty() {
            return Ok(vec![]);
        }

        let mut output = String::new();
        for msg in &all_messages {
            output.push_str(&serde_json::to_string(msg)?);
            output.push('\n');
        }
        write_atomic(&inbox_path, output)?;
        Ok(claimed)
    }

    /// Returns unread messages and marks them read.
    pub fn receive(&mut self) -> anyhow::Result<Vec<AgentMessage>> {
        self.take_matching(|_| true)
    }

    /// Claims specific messages by predicate, leaving everything else unread.
    ///
    /// `receive` marks the whole inbox read, which loses unrelated traffic when
    /// a caller only cares about a subset (e.g. approval responses).
    pub fn receive_matching<F>(&mut self, predicate: F) -> anyhow::Result<Vec<AgentMessage>>
    where
        F: Fn(&AgentMessage) -> bool,
    {
        self.take_matching(predicate)
    }

    pub fn peek(&self) -> anyhow::Result<Vec<AgentMessage>> {
        let inbox_path = self.inbox_path();
        if !inbox_path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&inbox_path)?;
        Ok(content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .filter(|m: &AgentMessage| !m.read)
            .collect())
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        let inbox_path = self.inbox_path();
        if inbox_path.exists() {
            std::fs::remove_file(&inbox_path)?;
        }
        Ok(())
    }

    pub fn cleanup_team(base_dir: PathBuf, team_name: &str) -> anyhow::Result<()> {
        let inboxes_dir = base_dir
            .join("teams")
            .join(Self::sanitize(team_name))
            .join("inboxes");
        if inboxes_dir.exists() {
            std::fs::remove_dir_all(&inboxes_dir)?;
        }
        Ok(())
    }

    fn sanitize(name: &str) -> String {
        name.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn mailbox(dir: &std::path::Path, agent: &str) -> Mailbox {
        Mailbox::new(dir.to_path_buf(), "atomic-team", agent).unwrap()
    }

    #[test]
    fn concurrent_sends_are_all_delivered() {
        let dir = tempdir().unwrap();
        let base = dir.path().to_path_buf();

        std::thread::scope(|scope| {
            for sender in 0..4 {
                let base = base.clone();
                scope.spawn(move || {
                    let mbox = Mailbox::new(base, "atomic-team", &format!("w{}", sender)).unwrap();
                    for n in 0..25 {
                        mbox.send_chat("lead", &format!("{}-{}", sender, n))
                            .unwrap();
                    }
                });
            }
        });

        let mut lead = mailbox(&base, "lead");
        let messages = lead.receive().unwrap();
        assert_eq!(messages.len(), 100, "no message may be lost or corrupted");
    }

    #[test]
    fn receive_marks_messages_read_once() {
        let dir = tempdir().unwrap();
        let base = dir.path().to_path_buf();
        mailbox(&base, "worker").send_chat("lead", "hello").unwrap();

        let mut lead = mailbox(&base, "lead");
        assert_eq!(lead.receive().unwrap().len(), 1);
        assert!(lead.receive().unwrap().is_empty());
    }

    #[test]
    fn receive_matching_leaves_other_traffic_unread() {
        let dir = tempdir().unwrap();
        let base = dir.path().to_path_buf();
        {
            let sender = mailbox(&base, "worker");
            sender
                .send(
                    "lead",
                    MessageContent::ApprovalResponse {
                        approved: true,
                        reason: None,
                    },
                )
                .unwrap();
            sender
                .send_chat("lead", "unrelated task assignment")
                .unwrap();
        }

        let mut lead = mailbox(&base, "lead");
        let approvals = lead
            .receive_matching(|m| matches!(m.content, MessageContent::ApprovalResponse { .. }))
            .unwrap();
        assert_eq!(approvals.len(), 1);

        // The unrelated message is still available.
        let remaining = lead.receive().unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(matches!(remaining[0].content, MessageContent::Chat { .. }));
    }

    #[test]
    fn a_send_during_a_read_is_not_lost() {
        let dir = tempdir().unwrap();
        let base = dir.path().to_path_buf();
        let mut lead = mailbox(&base, "lead");
        mailbox(&base, "worker").send_chat("lead", "first").unwrap();

        let first = lead.receive().unwrap();
        assert_eq!(first.len(), 1);

        mailbox(&base, "worker")
            .send_chat("lead", "second")
            .unwrap();
        let second = lead.receive().unwrap();
        assert_eq!(second.len(), 1);
        match &second[0].content {
            MessageContent::Chat { text } => assert_eq!(text, "second"),
            other => panic!("unexpected content: {:?}", other),
        }
    }
}
