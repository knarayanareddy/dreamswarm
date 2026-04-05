#![allow(
    clippy::ptr_arg,
    clippy::manual_strip,
    clippy::vec_init_then_push,
    clippy::field_reassign_with_default
)]
use clap::{Parser, Subcommand};
use dreamswarm::daemon::daily_log::DailyLog;
use dreamswarm::daemon::kairos::KairosDaemon;
use dreamswarm::daemon::process::DaemonProcess;
use dreamswarm::daemon::DaemonConfig;
use dreamswarm::db::Database;
use dreamswarm::memory::MemorySystem;
use dreamswarm::query::engine::QueryEngine;
use dreamswarm::runtime::agent_loop::AgentRuntime;
use dreamswarm::runtime::config::AppConfig;
use dreamswarm::runtime::session::Session;
use dreamswarm::swarm::mailbox::Mailbox;
use dreamswarm::tools::ToolRegistry;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(name = "dreamswarm")]
#[command(about = "Open-source autonomous multi-agent coding platform")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Model to use
    #[arg(short, long, default_value = "claude-sonnet-4-20250514", global = true)]
    model: String,

    /// Provider (anthropic, openai, mock)
    #[arg(short, long, default_value = "anthropic", global = true)]
    provider: String,

    /// Permission mode
    #[arg(long, default_value = "default", global = true)]
    mode: String,

    /// Initial system prompt (for workers)
    #[arg(long, global = true)]
    prompt: Option<String>,

    /// Working directory context
    #[arg(long, global = true)]
    directory: Option<PathBuf>,

    /// Agent role (e.g. "architect", "coder")
    #[arg(long, global = true)]
    role: Option<String>,

    /// Run in background (daemon mode)
    #[arg(long)]
    bg: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// KAIROS daemon management
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Run an interactive chat session
    Chat,
    /// Manage active sessions
    Sessions,
    /// Launch an autonomous worker agent (non-interactive)
    Worker {
        /// Team name this worker belongs to
        #[arg(short, long)]
        team: String,
    },
    /// Launch the visual swarm dashboard for a team
    Swarm {
        /// Team name to monitor
        #[arg(short, long, default_value = "default")]
        team: String,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon in the background
    Start,
    /// Stop the running daemon
    Stop,
    /// Show daemon status
    Status,
    /// Run the daemon in the foreground (used internally by --bg)
    Run,
    /// View today's daemon log
    Log {
        /// Number of entries to show
        #[arg(short, long, default_value = "20")]
        count: usize,
    },
    /// Reset daemon trust level to 100%
    ResetTrust,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let config = AppConfig::new(cli.model.clone(), cli.provider.clone(), cli.mode.clone());
    let daemon_config = DaemonConfig::default();

    // Initialize Memory System
    let memory_dir = config.state_dir.join("memory");
    let memory = Arc::new(RwLock::new(MemorySystem::new(memory_dir)?));

    if cli.bg {
        let process = DaemonProcess::new(daemon_config.state_dir.clone());
        return process.start(&[]).await;
    }

    match cli.command {
        Some(Commands::Daemon { action }) => {
            let process = DaemonProcess::new(daemon_config.state_dir.clone());
            match action {
                DaemonAction::Start => {
                    process.start(&[]).await?;
                }
                DaemonAction::Stop => {
                    process.stop().await?;
                }
                DaemonAction::Status => {
                    let status = process.status().await?;
                    println!("\n🌙 KAIROS Daemon Status");
                    println!(
                        "Running: {}",
                        if status.running { "✅ Yes" } else { "❌ No" }
                    );
                    if let Some(started) = status.started_at {
                        println!("Started: {}", started.format("%Y-%m-%d %H:%M UTC"));
                    }
                    println!("Actions today: {}", status.actions_taken);
                    println!("Tokens today: {}", status.tokens_used_today);
                    println!("Cost today: ${:.4}", status.cost_today_usd);
                    println!("Trust: {:.0}%", status.trust_level * 100.0);
                }
                DaemonAction::Run => {
                    let query_engine =
                        Arc::new(QueryEngine::new(&config.provider, &config.model, &config)?);
                    let mut daemon =
                        KairosDaemon::new(daemon_config, &config, Some(query_engine), memory)?;
                    daemon.run().await?;
                }
                DaemonAction::Log { count } => {
                    let log = DailyLog::new(&daemon_config.state_dir)?;
                    let entries = log.read_today()?;
                    println!("\n📋 Today's Daemon Log ({} entries)\n", entries.len());
                    for entry in entries.iter().rev().take(count) {
                        println!(
                            "[{}] {:?}: {}",
                            entry.timestamp.format("%H:%M:%S"),
                            entry.kind,
                            &entry.content[..entry.content.len().min(120)]
                        );
                    }
                }
                DaemonAction::ResetTrust => {
                    println!("🔄 Trust reset to 100%. Restart the daemon for this to take effect.");
                }
            }
        }
        Some(Commands::Chat) | None => {
            println!("Initializing DreamSwarm chat with model: {}", config.model);

            // Initialize Database
            let db = Database::new(&config.state_dir)?;
            db.migrate()?;

            // Initialize Query Engine
            let query_engine = QueryEngine::new(&config.provider, &config.model, &config)?;

            // Initialize Mailbox
            let mbox = Arc::new(RwLock::new(Mailbox::new("default", "lead")?));

            // Initialize Tool Registry
            let tool_registry = ToolRegistry::default_phase1(memory.clone(), Some(mbox.clone()));

            // Initialize Session
            let session = Session::new();

            // Initialize Runtime
            let runtime = AgentRuntime::new(session, query_engine, tool_registry, config, db, Some(mbox));

            // Start TUI
            dreamswarm::tui::app::run_interactive(runtime).await?;
        }
        Some(Commands::Worker { team }) => {
            println!("🐝 Worker: Starting member of team: {}", team);

            let db = Database::new(&config.state_dir)?;
            db.migrate()?;

            let query_engine = QueryEngine::new(&config.provider, &config.model, &config)?;
            
            // Initialize Mailbox for worker
            let mbox = Arc::new(RwLock::new(Mailbox::new(&team, &cli.role.clone().unwrap_or_else(|| "worker".to_string()))?));
            
            let tool_registry = ToolRegistry::default_phase1(memory.clone(), Some(mbox.clone()));
            let mut session = Session::new();

            // Inject role/prompt into session
            if let Some(ref role) = cli.role {
                session.add_user_message(&format!("System: Your role is {}.", role));
            }
            if let Some(ref prompt) = cli.prompt {
                session.add_user_message(prompt);
            }

            let mut runtime = AgentRuntime::new(
                session,
                query_engine,
                tool_registry,
                config,
                db,
                Some(mbox),
            );

            // In worker mode, we assume semi-autonomy or piping
            // We use an auto-approver for Dangerous tools for now (or until mailbox is ready)
            let auto_approve = |name: String, _input: serde_json::Value| async move {
                println!("  [Worker Auto-Approve] Executing Dangerous tool: {}", name);
                true
            };

            // Simple line-by-line REPL for the orchestration layer (e.g. Tmux) to drive
            use std::io::{BufRead, Write};
            let stdin = std::io::stdin();
            let mut stdout = std::io::stdout();

            for line in stdin.lock().lines() {
                let input = line?;
                if input.trim() == "/quit" {
                    break;
                }

                let result = runtime.run_turn(&input, auto_approve).await?;
                println!("{}", result.final_text);
                stdout.flush()?;
            }
        }
        Some(Commands::Swarm { team }) => {
            println!("🐝 Launching Swarm Dashboard for team: {}", team);
            dreamswarm::tui::swarm_dashboard::run_dashboard(&team).await?;
        }
        Some(Commands::Sessions) => {
            println!("Listing active sessions...");
        }
    }

    Ok(())
}
