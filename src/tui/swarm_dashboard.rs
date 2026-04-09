use crate::swarm::{task_list::SharedTaskList, task_list::TaskStatus, TeamState, WorkerStatus};
use chrono::Utc;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Sparkline, Table, Tabs},
    Terminal,
};
use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::process::Command;

const MAX_LOG_ENTRIES: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    Registry,
    Memory,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConflictView {
    SideBySide,
    Stacked,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TicketKind {
    Conflict,
    Synthesis,
    Refinement,
}

#[derive(Debug, Clone)]
pub struct ConflictTicket {
    pub id: String,
    pub kind: TicketKind,
    pub topic: String,
    pub subtopic: String,
    pub reason: String,
    pub existing: String,
    pub proposed: String,
}

pub struct SwarmApp {
    pub team_name: String,
    pub base_dir: PathBuf,
    pub state: Option<TeamState>,
    pub selected_worker_index: usize,
    pub should_quit: bool,
    pub last_update: Instant,
    /// Rolling log of message bus events
    pub message_log: VecDeque<String>,
    /// Throughput for sparklines (msgs per tick)
    pub throughput_history: VecDeque<u64>,
    /// Transient status message (text, expiration_instant)
    pub last_action_status: Option<(String, Instant)>,
    pub total_cost: f64,
    pub selected_tab: usize,
    pub mode: AppMode,
    pub conflicts: Vec<ConflictTicket>,
    pub selected_conflict_index: usize,
    pub conflict_view: ConflictView,
    pub global_relays: Vec<String>,
}

impl SwarmApp {
    pub fn new(team_name: &str, base_dir: PathBuf) -> Self {
        Self {
            team_name: team_name.to_string(),
            base_dir,
            state: None,
            selected_worker_index: 0,
            should_quit: false,
            last_update: Instant::now(),
            message_log: VecDeque::with_capacity(MAX_LOG_ENTRIES),
            throughput_history: VecDeque::from(vec![0; 50]),
            last_action_status: None,
            total_cost: 0.0,
            selected_tab: 0,
            mode: AppMode::Registry,
            conflicts: Vec::new(),
            selected_conflict_index: 0,
            conflict_view: ConflictView::Stacked,
            global_relays: Vec::new(),
        }
    }

    pub fn refresh_conflicts(&mut self) -> anyhow::Result<()> {
        let conflicts_dir = self
            .base_dir
            .join(".dreamswarm")
            .join("memory")
            .join("conflicts");
        if !conflicts_dir.exists() {
            self.conflicts = Vec::new();
            return Ok(());
        }

        let mut new_conflicts = Vec::new();
        for entry in std::fs::read_dir(conflicts_dir)?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                let content = std::fs::read_to_string(&path)?;
                if let Some(ticket) = self.parse_conflict_ticket(
                    entry.file_name().to_string_lossy().to_string(),
                    &content,
                ) {
                    new_conflicts.push(ticket);
                }
            }
        }
        self.conflicts = new_conflicts;
        Ok(())
    }

    fn parse_conflict_ticket(&self, id: String, content: &str) -> Option<ConflictTicket> {
        let mut kind = TicketKind::Conflict;
        let mut topic = String::new();
        let mut subtopic = String::new();
        let mut reason = String::new();
        let mut existing = String::new();
        let mut proposed = String::new();

        if let Some(line) = content.lines().next() {
            if line.contains("Knowledge Conflict") {
                let header = line.trim_start_matches("# Knowledge Conflict: ");
                let parts: Vec<&str> = header.split('/').collect();
                if parts.len() >= 2 {
                    topic = parts[0].to_string();
                    subtopic = parts[1].to_string();
                }
            } else if line.contains("Thematic Consolidation Proposal") {
                kind = TicketKind::Synthesis;
                topic = line
                    .trim_start_matches("# Thematic Consolidation Proposal: ")
                    .to_string();
                subtopic = "L3 Theme".to_string();
            } else if line.contains("Instruction Refinement Proposal") {
                kind = TicketKind::Refinement;
                topic = line
                    .trim_start_matches("# Instruction Refinement Proposal: ")
                    .to_string();
                subtopic = "Self-Mirror".to_string();
            }
        }

        let sections: Vec<&str> = content.split("## ").collect();
        for section in sections {
            if section.starts_with("Reason") {
                reason = section.trim_start_matches("Reason").trim().to_string();
            } else if section.starts_with("Existing Knowledge")
                || section.starts_with("Source L2 Files")
            {
                let iden = if section.starts_with("Existing Knowledge") {
                    "Existing Knowledge"
                } else {
                    "Source L2 Files"
                };
                existing = section.trim_start_matches(iden).trim().to_string();
            } else if section.starts_with("New Contradicting Observation")
                || section.starts_with("Synthesis")
                || section.starts_with("New Instructions")
            {
                let iden = if section.starts_with("New Contradicting Observation") {
                    "New Contradicting Observation"
                } else if section.starts_with("Synthesis") {
                    "Synthesis"
                } else {
                    "New Instructions"
                };
                proposed = section.trim_start_matches(iden).trim().to_string();
                // Special case for footer stripping
                if let Some(pos) = proposed.find("\n---\n") {
                    proposed.truncate(pos);
                }
            }
        }

        Some(ConflictTicket {
            id,
            kind,
            topic,
            subtopic,
            reason,
            existing,
            proposed,
        })
    }

    pub fn update_state(&mut self) -> anyhow::Result<()> {
        let team_dir = self.base_dir.join("teams").join(&self.team_name);
        let mut messages_this_tick = 0;

        // Update swarm state
        let state_path = team_dir.join("state.json");
        if state_path.exists() {
            let content = std::fs::read_to_string(state_path)?;
            if let Ok(state) = serde_json::from_str::<TeamState>(&content) {
                // Approximate cost (normally would be aggregated from session DB)
                self.total_cost = state.workers.len() as f64 * 0.15;
                self.state = Some(state);
            }
        }

        // Scan mailbox directory
        let mailbox_dir = team_dir.join("inboxes");
        if mailbox_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&mailbox_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    if let Ok(raw) = std::fs::read_to_string(entry.path()) {
                        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&raw) {
                            let msg_type = msg["type"].as_str().unwrap_or("Unknown");
                            let from = msg["from"].as_str().unwrap_or("?");
                            let to = msg["to"].as_str().unwrap_or("?");

                            let log_line = match msg_type {
                                "HelpRequest" => {
                                    messages_this_tick += 1;
                                    let task = msg["content"]["task"]
                                        .as_str()
                                        .unwrap_or("")
                                        .chars()
                                        .take(40)
                                        .collect::<String>();
                                    format!("🆘 {} → {}: \"{}\"", from, to, task)
                                }
                                "TaskAssignment" => {
                                    messages_this_tick += 1;
                                    let tid = msg["content"]["task_id"].as_str().unwrap_or("?");
                                    format!("📋 Lead → {}: Task #{}", to, &tid[..6.min(tid.len())])
                                }
                                "TaskResult" => {
                                    messages_this_tick += 1;
                                    format!("🏁 {} → Lead: Task complete", from)
                                }
                                _ => continue,
                            };

                            if !self.message_log.contains(&log_line) {
                                if self.message_log.len() >= MAX_LOG_ENTRIES {
                                    self.message_log.pop_front();
                                }
                                self.message_log.push_back(log_line);
                            }
                        }
                    }
                }
            }
        }

        // Push throughput to history
        self.throughput_history.pop_front();
        self.throughput_history.push_back(messages_this_tick);

        // Check for expiring status messages
        if let Some((_, expires)) = self.last_action_status {
            if Instant::now() > expires {
                self.last_action_status = None;
            }
        }

        self.last_update = Instant::now();
        Ok(())
    }

    pub fn refresh_global_relays(&mut self) -> anyhow::Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home"))?;
        let relay_dir = home.join(".dreamswarm").join("relay").join("inboxes");
        if !relay_dir.exists() {
            return Ok(());
        }

        let mut relays = Vec::new();
        for entry in std::fs::read_dir(relay_dir)?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                let content = std::fs::read_to_string(&path)?;
                for line in content.lines().rev().take(10) {
                    if let Ok(msg) = serde_json::from_str::<crate::swarm::AgentMessage>(line) {
                        relays.push(format!(
                            "[{}] {} -> {}: {:?}",
                            msg.timestamp.format("%H:%M:%S"),
                            msg.from,
                            msg.to,
                            msg.content
                        ));
                    }
                }
            }
        }
        self.global_relays = relays;
        Ok(())
    }
}

pub async fn run_dashboard(team_name: &str, base_dir: PathBuf) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = SwarmApp::new(team_name, base_dir);
    let tick_rate = Duration::from_millis(250);

    loop {
        let _ = app.update_state();
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(app.last_update.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Char('m') => {
                        app.mode = match app.mode {
                            AppMode::Registry => {
                                let _ = app.refresh_conflicts();
                                AppMode::Memory
                            }
                            AppMode::Memory => {
                                let _ = app.refresh_global_relays();
                                AppMode::Global
                            }
                            AppMode::Global => AppMode::Registry,
                        };
                    }
                    KeyCode::Char('v') if app.mode == AppMode::Memory => {
                        app.conflict_view = match app.conflict_view {
                            ConflictView::SideBySide => ConflictView::Stacked,
                            ConflictView::Stacked => ConflictView::SideBySide,
                        };
                        app.last_action_status = Some((
                            format!("View toggled to {:?}", app.conflict_view),
                            Instant::now() + Duration::from_secs(2),
                        ));
                    }
                    KeyCode::Tab if app.mode == AppMode::Registry => {
                        app.selected_tab = (app.selected_tab + 1) % 3;
                    }
                    KeyCode::Up => match app.mode {
                        AppMode::Registry => {
                            if app.selected_worker_index > 0 {
                                app.selected_worker_index -= 1;
                            }
                        }
                        AppMode::Memory => {
                            if app.selected_conflict_index > 0 {
                                app.selected_conflict_index -= 1;
                            }
                        }
                        AppMode::Global => {}
                    },
                    KeyCode::Down => match app.mode {
                        AppMode::Registry => {
                            if let Some(ref state) = app.state {
                                if app.selected_worker_index < state.workers.len().saturating_sub(1)
                                {
                                    app.selected_worker_index += 1;
                                }
                            }
                        }
                        AppMode::Memory => {
                            if app.selected_conflict_index < app.conflicts.len().saturating_sub(1) {
                                app.selected_conflict_index += 1;
                            }
                        }
                        AppMode::Global => {}
                    },
                    KeyCode::Char('k') if app.mode == AppMode::Registry => {
                        if let Some(ref state) = app.state {
                            if let Some(worker) = state.workers.get(app.selected_worker_index) {
                                if let Some(ref pane_id) = worker.tmux_pane_id {
                                    let mut cmd = Command::new("tmux");
                                    cmd.args(["kill-pane", "-t", pane_id]);
                                    let _ = cmd.spawn();
                                    app.last_action_status = Some((
                                        format!("Killed agent {}", worker.name),
                                        Instant::now() + Duration::from_secs(3),
                                    ));
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') if app.mode == AppMode::Registry => {
                        if let Some(ref state) = app.state {
                            if let Some(worker) = state.workers.get(app.selected_worker_index) {
                                if let Ok(tl) = SharedTaskList::new(&app.team_name) {
                                    if let Ok(tasks) = tl.list_tasks() {
                                        for task in tasks {
                                            if task.assigned_to.as_deref() == Some(&worker.id) {
                                                let _ = tl.update_task(
                                                    &task.id,
                                                    TaskStatus::Pending,
                                                    None,
                                                );
                                                app.last_action_status = Some((
                                                    format!(
                                                        "Re-assigned task from {}",
                                                        worker.name
                                                    ),
                                                    Instant::now() + Duration::from_secs(3),
                                                ));
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('a') if app.mode == AppMode::Memory => {
                        if let Some(conflict) = app.conflicts.get(app.selected_conflict_index) {
                            let conflicts_dir = app
                                .base_dir
                                .join(".dreamswarm")
                                .join("memory")
                                .join("conflicts");
                            let resolved_dir = conflicts_dir.join("resolved");
                            let _ = std::fs::create_dir_all(&resolved_dir);

                            match conflict.kind {
                                TicketKind::Conflict => {
                                    // 1. Update topic
                                    let topic_dir = app
                                        .base_dir
                                        .join(".dreamswarm")
                                        .join("memory")
                                        .join("topics")
                                        .join(conflict.topic.to_lowercase().replace(' ', "-"));
                                    let topic_path = topic_dir.join(format!(
                                        "{}.md",
                                        conflict.subtopic.to_lowercase().replace(' ', "-")
                                    ));

                                    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
                                    let resolution_entry = format!(
                                        "\n---\n_[{} | ✅ verified]_\n_Source: User Resolved Conflict_\n{}\n",
                                        timestamp, conflict.proposed
                                    );

                                    if let Ok(mut file) =
                                        std::fs::OpenOptions::new().append(true).open(&topic_path)
                                    {
                                        use std::io::Write;
                                        let _ = write!(file, "{}", resolution_entry);
                                    }
                                }
                                TicketKind::Synthesis => {
                                    // 1. Write L3 Chapter
                                    let memory_dir =
                                        app.base_dir.join(".dreamswarm").join("memory");
                                    let themes_dir = memory_dir.join("themes");
                                    let theme_path = themes_dir.join(format!(
                                        "{}.md",
                                        conflict.topic.to_lowercase().replace(' ', "-")
                                    ));
                                    let _ = std::fs::write(&theme_path, &conflict.proposed);

                                    // 2. Archive Source L2s
                                    let archive_dir =
                                        memory_dir.join("archive").join("synthesized");
                                    let _ = std::fs::create_dir_all(&archive_dir);
                                    let topic_dir = memory_dir.join("topics");

                                    // 3. Update L1 Index (MEMORY.md)
                                    let index_path = memory_dir.join("MEMORY.md");
                                    let index = crate::memory::index::MemoryIndex::new(index_path);

                                    for line in conflict.existing.lines() {
                                        let source_rel = line.trim();
                                        if source_rel.is_empty() {
                                            continue;
                                        }
                                        let source_path = topic_dir.join(source_rel);

                                        if source_path.exists() {
                                            let dest = archive_dir.join(source_rel);
                                            if let Some(p) = dest.parent() {
                                                let _ = std::fs::create_dir_all(p);
                                            }
                                            let _ = std::fs::rename(&source_path, dest);
                                            // Remove from index
                                            let _ = index.remove_pointer(source_rel);
                                        }
                                    }

                                    // Add L3 theme to index
                                    let theme_rel = format!(
                                        "themes/{}.md",
                                        conflict.topic.to_lowercase().replace(' ', "-")
                                    );
                                    let _ = index.upsert_pointer(
                                        &conflict.topic,
                                        &theme_rel,
                                        "Synthesized Chapter (L3)",
                                    );
                                }
                                TicketKind::Refinement => {
                                    // Save refinement to a dedicated store
                                    let refinements_dir = app
                                        .base_dir
                                        .join(".dreamswarm")
                                        .join("memory")
                                        .join("refinements");
                                    let _ = std::fs::create_dir_all(&refinements_dir);
                                    let ref_path = refinements_dir.join(format!(
                                        "{}_{}.md",
                                        conflict.topic.to_lowercase().replace(' ', "-"),
                                        Utc::now().timestamp()
                                    ));
                                    let content = format!(
                                        "# Agent Refinement: {}\n\n## Instructions\n{}\n\n## Reasoning\n{}\n",
                                        conflict.topic, conflict.proposed, conflict.reason
                                    );
                                    let _ = std::fs::write(ref_path, content);
                                }
                            }

                            // Archive ticket
                            let _ = std::fs::rename(
                                conflicts_dir.join(&conflict.id),
                                resolved_dir.join(&conflict.id),
                            );

                            app.last_action_status = Some((
                                format!("Accepted: {}/{}", conflict.topic, conflict.subtopic),
                                Instant::now() + Duration::from_secs(3),
                            ));
                            let _ = app.refresh_conflicts();
                        }
                    }
                    KeyCode::Char('k') if app.mode == AppMode::Memory => {
                        if let Some(conflict) = app.conflicts.get(app.selected_conflict_index) {
                            let conflicts_dir = app
                                .base_dir
                                .join(".dreamswarm")
                                .join("memory")
                                .join("conflicts");
                            let resolved_dir = conflicts_dir.join("resolved");
                            let _ = std::fs::create_dir_all(&resolved_dir);

                            let _ = std::fs::rename(
                                conflicts_dir.join(&conflict.id),
                                resolved_dir.join(&conflict.id),
                            );

                            app.last_action_status = Some((
                                format!("Kept Existing: {}/{}", conflict.topic, conflict.subtopic),
                                Instant::now() + Duration::from_secs(3),
                            ));
                            let _ = app.refresh_conflicts();
                        }
                    }
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn ui(f: &mut ratatui::Frame, app: &SwarmApp) {
    match app.mode {
        AppMode::Registry => render_registry_view(f, app),
        AppMode::Memory => render_memory_view(f, app),
        AppMode::Global => render_global_view(f, app),
    }
}

fn render_registry_view(f: &mut ratatui::Frame, app: &SwarmApp) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3), // Vitals
                Constraint::Min(10),   // Main body
                Constraint::Length(1), // Spacer
                Constraint::Length(3), // Footer / Status
            ]
            .as_ref(),
        )
        .split(f.size());

    // ── 1. Swarm Vitals (Header) ──────────────────────────────────────────────
    let worker_count = app.state.as_ref().map_or(0, |s| s.workers.len());
    let active_count = app.state.as_ref().map_or(0, |s| {
        s.workers
            .iter()
            .filter(|w| w.status == WorkerStatus::Active)
            .count()
    });

    let uptime = app.state.as_ref().map_or("0m".to_string(), |s| {
        let elapsed = Utc::now() - s.created_at;
        format!("{}m", elapsed.num_minutes())
    });

    let vitals_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(root[0]);

    let vital_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    f.render_widget(
        Paragraph::new(format!(" 🐝 Team: {}", app.team_name))
            .block(Block::default().borders(Borders::ALL))
            .style(vital_style),
        vitals_layout[0],
    );
    f.render_widget(
        Paragraph::new(format!(" ⏱ Uptime: {}", uptime))
            .block(Block::default().borders(Borders::ALL))
            .style(vital_style),
        vitals_layout[1],
    );
    f.render_widget(
        Paragraph::new(format!(" 🤖 Agents: {}/{}", active_count, worker_count))
            .block(Block::default().borders(Borders::ALL))
            .style(vital_style),
        vitals_layout[2],
    );
    f.render_widget(
        Paragraph::new(format!(" 💰 Est. Cost: ${:.2}", app.total_cost))
            .block(Block::default().borders(Borders::ALL))
            .style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        vitals_layout[3],
    );

    // ── 2. Main Body ──────────────────────────────────────────────────────────
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(root[1]);

    let right_column = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage(50), // Inspector
                Constraint::Percentage(50), // Message Bus
            ]
            .as_ref(),
        )
        .split(body[1]);

    // Workers Panel
    let workers_list: Vec<ListItem> = if let Some(ref state) = app.state {
        state
            .workers
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let (symbol, color) = match w.status {
                    WorkerStatus::Active => ("🐝", Color::Green),
                    WorkerStatus::Idle => ("💤", Color::DarkGray),
                    WorkerStatus::Spawning => ("🥚", Color::Yellow),
                    WorkerStatus::Completed => ("✅", Color::Cyan),
                    _ => ("🚫", Color::Red),
                };
                let style = if i == app.selected_worker_index {
                    Style::default()
                        .bg(Color::Indexed(236))
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(color)
                };
                let label = format!(
                    " {} {:<12} [{}]",
                    symbol,
                    w.name,
                    &w.id[..6.min(w.id.len())]
                );
                ListItem::new(Line::from(vec![Span::styled(label, style)]))
            })
            .collect()
    } else {
        vec![ListItem::new(" Scanning for agents...")]
    };

    f.render_widget(
        List::new(workers_list).block(
            Block::default()
                .title(" 🐝 Swarm Registry ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        body[0],
    );

    // Agent Inspector (Top Right)
    let inspector_block = Block::default()
        .title(" 🔍 Agent Deep-Dive ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inspector_inner = inspector_block.inner(right_column[0]);
    f.render_widget(inspector_block, right_column[0]);

    if let Some(ref state) = app.state {
        if let Some(worker) = state.workers.get(app.selected_worker_index) {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(inspector_inner);

            // Tab bar
            let titles = vec![" General ", " Logs ", " Files "];
            let tabs = Tabs::new(titles)
                .select(app.selected_tab)
                .block(Block::default().borders(Borders::BOTTOM))
                .style(Style::default().fg(Color::DarkGray))
                .highlight_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                );
            f.render_widget(tabs, chunks[0]);

            match app.selected_tab {
                0 => {
                    let status_str = format!("{:?}", worker.status);
                    let branch_str = worker.branch_name.as_deref().unwrap_or("-").to_string();
                    let worktree_str = worker.worktree_path.as_deref().unwrap_or("-").to_string();
                    let rows = vec![
                        Row::new(vec!["Role".to_string(), worker.role.clone()]),
                        Row::new(vec!["Status".to_string(), status_str]),
                        Row::new(vec!["Branch".to_string(), branch_str]),
                        Row::new(vec!["Worktree".to_string(), worktree_str]),
                    ];
                    let table = Table::new(rows, [Constraint::Length(10), Constraint::Min(20)])
                        .header(
                            Row::new(vec!["Property", "Value"])
                                .style(Style::default().add_modifier(Modifier::UNDERLINED)),
                        );
                    f.render_widget(table, chunks[1]);
                }
                1 => {
                    let worker_logs: Vec<ListItem> = app
                        .message_log
                        .iter()
                        .filter(|l| l.contains(&worker.id[..6]))
                        .map(|l| ListItem::new(l.as_str()))
                        .collect();
                    f.render_widget(List::new(worker_logs), chunks[1]);
                }
                _ => {
                    f.render_widget(
                        Paragraph::new("\n  File tracking disabled in preview."),
                        chunks[1],
                    );
                }
            }
        }
    }

    // Message Bus with Throughput (Bottom Right)
    let bus_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(right_column[1]);

    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .title(" 📡 Message Throughput ")
                .borders(Borders::LEFT | Borders::RIGHT | Borders::TOP),
        )
        .data(app.throughput_history.as_slices().0)
        .style(Style::default().fg(Color::Magenta));
    f.render_widget(sparkline, bus_layout[0]);

    let log_items: Vec<ListItem> = app
        .message_log
        .iter()
        .rev()
        .take(10)
        .map(|line| {
            let color = if line.contains("🆘") {
                Color::Red
            } else if line.contains("✅") {
                Color::Green
            } else {
                Color::White
            };
            ListItem::new(Line::from(Span::styled(
                format!(" {}", line),
                Style::default().fg(color),
            )))
        })
        .collect();

    f.render_widget(
        List::new(log_items).block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                .border_style(Style::default().fg(Color::Magenta)),
        ),
        bus_layout[1],
    );

    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(40)])
        .split(root[3]);

    let footer_text = match app.mode {
        AppMode::Registry => {
            " [q] Quit  [tab] Switch Tab  [m] Memory Mode  [k] Kill  [r] Re-assign "
        }
        AppMode::Memory => {
            " [q] Quit  [m] Global Mode  [v] Toggle View  [a] Accept New  [k] Keep Existing "
        }
        AppMode::Global => " [q] Quit  [m] Registry Mode  Relay: ~/.dreamswarm/relay ",
    };

    let base_footer = Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(base_footer, footer_chunks[0]);

    if let Some((ref msg, _)) = app.last_action_status {
        let status = Paragraph::new(format!(" 📢 {} ", msg))
            .style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(status, footer_chunks[1]);
    }
}

fn render_memory_view(f: &mut ratatui::Frame, app: &SwarmApp) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3), // Header (Reuse Vitals)
                Constraint::Min(10),   // Main body
                Constraint::Length(1), // Spacer
                Constraint::Length(3), // Footer / Status
            ]
            .as_ref(),
        )
        .split(f.size());

    // Reuse vitals for consistency
    render_vitals(f, app, root[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)].as_ref())
        .split(root[1]);

    // 1. Conflict List
    let items: Vec<ListItem> = app
        .conflicts
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let style = if i == app.selected_conflict_index {
                Style::default()
                    .bg(Color::Indexed(236))
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let icon = match c.kind {
                TicketKind::Conflict => "⚠️ ",
                TicketKind::Synthesis => "✨ ",
                TicketKind::Refinement => "🎯 ",
            };
            ListItem::new(format!(" {} {}/{}", icon, c.topic, c.subtopic)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" 🚩 Conflicts ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(list, body[0]);

    // 2. Conflict Detail
    if let Some(conflict) = app.conflicts.get(app.selected_conflict_index) {
        let detail_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(8)])
            .split(body[1]);

        // Knowledge Diffs
        let knowledge_area = detail_chunks[0];
        match app.conflict_view {
            ConflictView::SideBySide => {
                let diff_layout = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                    .split(knowledge_area);

                let (left_head, right_head) = match conflict.kind {
                    TicketKind::Conflict => (" 🏛 Existing (L2) ", " 🆕 Proposed (L1) "),
                    TicketKind::Synthesis => (" 🧩 Sources (L2) ", " 📖 Synthetic (L3) "),
                    TicketKind::Refinement => (" 👤 Current Role ", " 🧠 Optimized Role "),
                };

                f.render_widget(
                    render_knowledge_block(left_head, &conflict.existing, Color::Cyan),
                    diff_layout[0],
                );
                f.render_widget(
                    render_knowledge_block(right_head, &conflict.proposed, Color::Green),
                    diff_layout[1],
                );
            }
            ConflictView::Stacked => {
                let diff_layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                    .split(knowledge_area);

                let (left_head, right_head) = match conflict.kind {
                    TicketKind::Conflict => (" 🏛 Existing (L2) ", " 🆕 Proposed (L1) "),
                    TicketKind::Synthesis => (" 🧩 Sources (L2) ", " 📖 Synthetic (L3) "),
                    TicketKind::Refinement => (" 👤 Current Role ", " 🧠 Optimized Role "),
                };

                f.render_widget(
                    render_knowledge_block(left_head, &conflict.existing, Color::Cyan),
                    diff_layout[0],
                );
                f.render_widget(
                    render_knowledge_block(right_head, &conflict.proposed, Color::Green),
                    diff_layout[1],
                );
            }
        }

        // Reasoning
        let reason_block = Paragraph::new(format!("Reasoning: {}", conflict.reason))
            .block(
                Block::default()
                    .title(" 💡 Analyst Notes ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .wrap(ratatui::widgets::Wrap { trim: true });
        f.render_widget(reason_block, detail_chunks[1]);
    } else {
        f.render_widget(
            Paragraph::new("\n\n  No active knowledge conflicts found. Memory is synchronized."),
            body[1],
        );
    }

    render_footer(f, app, root[3]);
}

fn render_vitals(f: &mut ratatui::Frame, app: &SwarmApp, area: ratatui::layout::Rect) {
    let worker_count = app.state.as_ref().map_or(0, |s| s.workers.len());
    let active_count = app.state.as_ref().map_or(0, |s| {
        s.workers
            .iter()
            .filter(|w| w.status == WorkerStatus::Active)
            .count()
    });
    let uptime = app.state.as_ref().map_or("0m".to_string(), |s| {
        let elapsed = Utc::now() - s.created_at;
        format!("{}m", elapsed.num_minutes())
    });

    let vitals_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    let vital_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    f.render_widget(
        Paragraph::new(format!(" 🐝 Team: {}", app.team_name))
            .block(Block::default().borders(Borders::ALL))
            .style(vital_style),
        vitals_layout[0],
    );
    f.render_widget(
        Paragraph::new(format!(" ⏱ Uptime: {}", uptime))
            .block(Block::default().borders(Borders::ALL))
            .style(vital_style),
        vitals_layout[1],
    );
    f.render_widget(
        Paragraph::new(format!(" 🤖 Agents: {}/{}", active_count, worker_count))
            .block(Block::default().borders(Borders::ALL))
            .style(vital_style),
        vitals_layout[2],
    );
    f.render_widget(
        Paragraph::new(format!(" 💰 Est. Cost: ${:.2}", app.total_cost))
            .block(Block::default().borders(Borders::ALL))
            .style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        vitals_layout[3],
    );
}

fn render_footer(f: &mut ratatui::Frame, app: &SwarmApp, area: ratatui::layout::Rect) {
    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(40)])
        .split(area);

    let footer_text = match app.mode {
        AppMode::Registry => {
            " [q] Quit  [tab] Switch Tab  [m] Memory Mode  [k] Kill  [r] Re-assign "
        }
        AppMode::Memory => {
            " [q] Quit  [m] Global Mode  [v] Toggle View  [a] Accept New  [k] Keep Existing "
        }
        AppMode::Global => " [q] Quit  [m] Registry Mode  Relay: ~/.dreamswarm/relay ",
    };

    let base_footer = Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(base_footer, footer_chunks[0]);

    if let Some((ref msg, _)) = app.last_action_status {
        let status = Paragraph::new(format!(" 📢 {} ", msg))
            .style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(status, footer_chunks[1]);
    }
}

fn render_knowledge_block<'a>(
    title: &'a str,
    content: &'a str,
    border_color: Color,
) -> Paragraph<'a> {
    Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .wrap(ratatui::widgets::Wrap { trim: true })
}

fn render_global_view(f: &mut ratatui::Frame, app: &SwarmApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(f.size());

    let header = Paragraph::new(" 🛸 GLOBAL OVERLORD: INTER-SWARM RELAY ")
        .block(Block::default().borders(Borders::ALL))
        .style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(header, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[1]);

    // 1. Relay Message Feed
    let relay_items: Vec<ListItem> = app
        .global_relays
        .iter()
        .map(|line| ListItem::new(line.as_str()))
        .collect();

    let relay_list = List::new(relay_items).block(
        Block::default()
            .title(" 📡 Cross-Swarm Signals ")
            .borders(Borders::ALL),
    );
    f.render_widget(relay_list, body[0]);

    // 2. Active Snapshots / Heartbeats
    let snapshot_dir = app.base_dir.join("teams");

    let mut swarm_items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(snapshot_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                swarm_items.push(ListItem::new(format!(" 🐝 Active Swarm: {}", name)));
            }
        }
    }

    let swarm_list = List::new(swarm_items).block(
        Block::default()
            .title(" 🏗 Machine-Local Swarms ")
            .borders(Borders::ALL),
    );
    f.render_widget(swarm_list, body[1]);
}
