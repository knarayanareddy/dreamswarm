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

    fn write_to_path(path: &PathBuf, msg: &AgentMessage) -> anyhow::Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        let line = serde_json::to_string(msg)?;
        writeln!(file, "{}", line)?;
        Ok(())
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

    pub fn receive(&mut self) -> anyhow::Result<Vec<AgentMessage>> {
        self.poll_count += 1;
        let inbox_path = self
            .inboxes_dir
            .join(format!("{}.jsonl", Self::sanitize(&self.agent_name)));
        if !inbox_path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&inbox_path)?;
        let mut all_messages: Vec<AgentMessage> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        let unread: Vec<AgentMessage> = all_messages.iter().filter(|m| !m.read).cloned().collect();
        if unread.is_empty() {
            return Ok(vec![]);
        }

        for msg in &mut all_messages {
            msg.read = true;
        }

        let mut output = String::new();
        for msg in &all_messages {
            output.push_str(&serde_json::to_string(msg)?);
            output.push('\n');
        }
        std::fs::write(&inbox_path, output)?;
        Ok(unread)
    }

    pub fn peek(&self) -> anyhow::Result<Vec<AgentMessage>> {
        let inbox_path = self
            .inboxes_dir
            .join(format!("{}.jsonl", Self::sanitize(&self.agent_name)));
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
        let inbox_path = self
            .inboxes_dir
            .join(format!("{}.jsonl", Self::sanitize(&self.agent_name)));
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
