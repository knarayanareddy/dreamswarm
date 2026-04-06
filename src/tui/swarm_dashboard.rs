use crate::swarm::{task_list::SharedTaskList, task_list::TaskStatus, TeamState, WorkerStatus};
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
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Terminal,
};
use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::process::Command;

const MAX_LOG_ENTRIES: usize = 50;

pub struct SwarmApp {
    pub team_name: String,
    pub base_dir: PathBuf,
    pub state: Option<TeamState>,
    pub selected_worker_index: usize,
    pub should_quit: bool,
    pub last_update: Instant,
    /// Rolling log of message bus events (HelpRequest, HelpResponse, PublishKnowledge)
    pub message_log: VecDeque<String>,
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
        }
    }

    pub fn update_state(&mut self) -> anyhow::Result<()> {
        let team_dir = self.base_dir.join("teams").join(&self.team_name);

        // Update swarm state
        let state_path = team_dir.join("state.json");
        if state_path.exists() {
            let content = std::fs::read_to_string(state_path)?;
            if let Ok(state) = serde_json::from_str::<TeamState>(&content) {
                self.state = Some(state);
            }
        }

        // Scan mailbox directory for new messages to populate the message bus log
        let mailbox_dir = team_dir.join("inboxes");
        if mailbox_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&mailbox_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    if let Ok(raw) = std::fs::read_to_string(entry.path()) {
                        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&raw) {
                            let msg_type = msg["content"]["type"].as_str().unwrap_or("Unknown");
                            let from = msg["from"].as_str().unwrap_or("?");
                            let to = msg["to"].as_str().unwrap_or("?");

                            let log_line = match msg_type {
                                "HelpRequest" => {
                                    let task = msg["content"]["task"]
                                        .as_str()
                                        .unwrap_or("")
                                        .chars()
                                        .take(40)
                                        .collect::<String>();
                                    format!("🆘 {} → {}: \"{}\"", from, to, task)
                                }
                                "HelpResponse" => {
                                    format!("✅ {} → {}: [HelpResponse]", from, to)
                                }
                                "TaskAssignment" => {
                                    let tid = msg["content"]["task_id"].as_str().unwrap_or("?");
                                    format!("📋 Lead → {}: Task #{}", to, &tid[..6.min(tid.len())])
                                }
                                "TaskResult" => {
                                    format!("🏁 {} → Lead: Task complete", from)
                                }
                                _ => continue,
                            };

                            // Only add if not already in the log
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

        // Also show recent knowledge publications
        let knowledge_dir = self.base_dir.join("memory").join("knowledge");
        if knowledge_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&knowledge_dir) {
                for entry in entries.flatten() {
                    if let Ok(raw) = std::fs::read_to_string(entry.path()) {
                        if let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) {
                            let title = doc["title"].as_str().unwrap_or("Untitled");
                            let log_line = format!("🧠 [Knowledge] \"{}\"", title);
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

        self.last_update = Instant::now();
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
                    KeyCode::Up => {
                        if app.selected_worker_index > 0 {
                            app.selected_worker_index -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if let Some(ref state) = app.state {
                            if app.selected_worker_index < state.workers.len().saturating_sub(1) {
                                app.selected_worker_index += 1;
                            }
                        }
                    }
                    KeyCode::Char('k') => {
                        if let Some(ref state) = app.state {
                            if let Some(worker) = state.workers.get(app.selected_worker_index) {
                                if let Some(ref pane_id) = worker.tmux_pane_id {
                                    let mut cmd = Command::new("tmux");
                                    cmd.args(["kill-pane", "-t", pane_id]);
                                    let _ = cmd.spawn();
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') => {
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
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
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
    // 3 vertical sections: header | body | footer
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(2),
            ]
            .as_ref(),
        )
        .split(f.size());

    // ── Header ────────────────────────────────────────────────────────────────
    let worker_count = app.state.as_ref().map_or(0, |s| s.workers.len());
    let heartbeat = if app.last_update.elapsed().as_millis() < 500 {
        "●"
    } else {
        "○"
    };
    let header_text = format!(
        " 🐝 DreamSwarm Dashboard {} │  Team: {}  │  Agents: {} ",
        heartbeat, app.team_name, worker_count
    );
    let header = Paragraph::new(header_text)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, root[0]);

    // ── Body: left = agents, center = inspector, bottom-right = message bus ────
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)].as_ref())
        .split(root[1]);

    let right_column = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .split(body[1]);

    // Workers panel
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
                    _ => ("❌", Color::Red),
                };
                let style = if i == app.selected_worker_index {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(color)
                };
                let label = format!("{} {} [{}]", symbol, w.name, &w.id[..6.min(w.id.len())]);
                ListItem::new(Line::from(vec![Span::styled(label, style)]))
            })
            .collect()
    } else {
        vec![ListItem::new("  Waiting for swarm data…")]
    };

    let workers_panel = List::new(workers_list).block(
        Block::default()
            .title(" 🐝 Swarm Members ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(workers_panel, body[0]);

    // Agent Inspector (Top Right)
    let inspector_data = if let Some(ref state) = app.state {
        if let Some(worker) = state.workers.get(app.selected_worker_index) {
            vec![
                Row::new(vec![Cell::from("Name"), Cell::from(worker.name.clone())]),
                Row::new(vec![Cell::from("ID"), Cell::from(worker.id.clone())]),
                Row::new(vec![
                    Cell::from("Status"),
                    Cell::from(format!("{:?}", worker.status)),
                ]),
                Row::new(vec![Cell::from("Role"), Cell::from(worker.role.clone())]),
                Row::new(vec![
                    Cell::from("Branch"),
                    Cell::from(worker.branch_name.as_deref().unwrap_or("None")),
                ]),
                Row::new(vec![
                    Cell::from("Tmux Pane"),
                    Cell::from(worker.tmux_pane_id.as_deref().unwrap_or("-")),
                ]),
                Row::new(vec![
                    Cell::from("Last Update"),
                    Cell::from(worker.updated_at.format("%H:%M:%S").to_string()),
                ]),
            ]
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let inspector = Table::new(
        inspector_data,
        [Constraint::Length(12), Constraint::Min(20)],
    )
    .block(
        Block::default()
            .title(" 🔍 Agent Inspector ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    )
    .header(
        Row::new(vec!["Attribute", "Value"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    );
    f.render_widget(inspector, right_column[0]);

    // Message Bus panel
    let log_items: Vec<ListItem> = if app.message_log.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  No messages yet…",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.message_log
            .iter()
            .rev()
            .map(|line| {
                let color = if line.starts_with("🆘") {
                    Color::Red
                } else if line.starts_with("✅") {
                    Color::Green
                } else if line.starts_with("🧠") {
                    Color::Magenta
                } else if line.starts_with("📋") {
                    Color::Yellow
                } else {
                    Color::White
                };
                ListItem::new(Line::from(Span::styled(
                    format!(" {}", line),
                    Style::default().fg(color),
                )))
            })
            .collect()
    };

    let bus_panel = List::new(log_items).block(
        Block::default()
            .title(" 📡 Hybrid Message Bus ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(bus_panel, right_column[1]);

    // ── Footer ────────────────────────────────────────────────────────────────
    let footer = Paragraph::new(" [q] Quit  [↑↓] Select  [k] Force-Kill  [r] Re-assign Task ")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, root[2]);
}
