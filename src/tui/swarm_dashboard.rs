use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crate::swarm::{task_list::TaskStatus, TeamState, WorkerStatus, task_list::SharedTaskList};
use std::io;
use std::time::{Duration, Instant};
use std::path::PathBuf;
use tokio::process::Command;

pub struct SwarmApp {
    pub team_name: String,
    pub state: Option<TeamState>,
    pub selected_worker_index: usize,
    pub selected_task_index: usize,
    pub should_quit: bool,
    pub last_update: Instant,
}

impl SwarmApp {
    pub fn new(team_name: &str) -> Self {
        Self {
            team_name: team_name.to_string(),
            state: None,
            selected_worker_index: 0,
            selected_task_index: 0,
            should_quit: false,
            last_update: Instant::now(),
        }
    }

    pub fn update_state(&mut self) -> anyhow::Result<()> {
        let state_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dreamswarm")
            .join("teams")
            .join(&self.team_name)
            .join("state.json");
        
        if state_path.exists() {
            let content = std::fs::read_to_string(state_path)?;
            let state: TeamState = serde_json::from_str(&content)?;
            self.state = Some(state);
        }
        self.last_update = Instant::now();
        Ok(())
    }
}

pub async fn run_dashboard(team_name: &str) -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = SwarmApp::new(team_name);
    let tick_rate = Duration::from_millis(250);

    loop {
        app.update_state()?;
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
                                    // Direct kill for phase 1 dashboard autonomy
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
                                let task_list = SharedTaskList::new(&app.team_name).ok();
                                if let Some(tl) = task_list {
                                    if let Ok(tasks) = tl.list_tasks() {
                                        for mut task in tasks {
                                            if task.assigned_to.as_deref() == Some(&worker.id) {
                                                task.status = TaskStatus::Pending;
                                                task.assigned_to = None;
                                                let _ = tl.update_task(&task.id, TaskStatus::Pending, None);
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

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut ratatui::Frame, app: &SwarmApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ].as_ref())
        .split(f.size());

    // Header
    let header = Paragraph::new(format!(" DreamSwarm Swarm Dashboard | Team: {} ", app.team_name))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // Main Content
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .split(chunks[1]);

    // Workers Panel
    let workers_list: Vec<ListItem> = if let Some(ref state) = app.state {
        state.workers.iter().enumerate().map(|(i, w)| {
            let status_symbol = match w.status {
                WorkerStatus::Active => "🐝",
                WorkerStatus::Idle => "💤",
                WorkerStatus::Spawning => "🥚",
                WorkerStatus::Completed => "✅",
                _ => "❌",
            };
            let style = if i == app.selected_worker_index {
                ratatui::style::Style::default().fg(ratatui::style::Color::Yellow).add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                ratatui::style::Style::default()
            };
            ListItem::new(format!("{} {} ({}) — {:?}", status_symbol, w.name, &w.id[..6], w.status)).style(style)
        }).collect()
    } else {
        vec![ListItem::new("No workers active.")]
    };
    let workers = List::new(workers_list)
        .block(Block::default().title(" Active Swarm ").borders(Borders::ALL));
    f.render_widget(workers, main_chunks[0]);

    // Task Panel
    // Note: We'd ideally pull from TaskList here too, but for MVP we'll show team status
    let status_block = Block::default().title(" Swarm Operations ").borders(Borders::ALL);
    let _inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(2)])
        .split(status_block.inner(main_chunks[1]));
    
    f.render_widget(status_block, main_chunks[1]);
    
    let instructions = Paragraph::new(" [q/Esc]: Quit | [k]: Kill Agent | [r]: Re-assign Task ");
    f.render_widget(instructions, chunks[2]);
}
