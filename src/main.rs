use clap::{Parser, Subcommand};
use dreamswarm::runtime::config::AppConfig;
use dreamswarm::daemon::{DaemonConfig, DaemonStatus};
use dreamswarm::daemon::process::DaemonProcess;
use dreamswarm::daemon::kairos::KairosDaemon;
use dreamswarm::daemon::daily_log::DailyLog;
use dreamswarm::query::engine::QueryEngine;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "dreamswarm")]
#[command(about = "Open-source autonomous multi-agent coding platform")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Model to use
    #[arg(short, long, default_value = "claude-sonnet-4-20250514")]
    model: String,

    /// Provider (anthropic, openai)
    #[arg(short, long, default_value = "anthropic")]
    provider: String,

    /// Permission mode
    #[arg(long, default_value = "default")]
    mode: String,

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
                    println!("Running: {}", if status.running { "✅ Yes" } else { "❌ No" });
                    if let Some(started) = status.started_at {
                        println!("Started: {}", started.format("%Y-%m-%d %H:%M UTC"));
                    }
                    println!("Actions today: {}", status.actions_taken);
                    println!("Tokens today: {}", status.tokens_used_today);
                    println!("Cost today: ${:.4}", status.cost_today_usd);
                    println!("Trust: {:.0}%", status.trust_level * 100.0);
                }
                DaemonAction::Run => {
                    let query_engine = Arc::new(QueryEngine::new(config.model.clone(), config.provider.clone())?);
                    let mut daemon = KairosDaemon::new(daemon_config, &config, Some(query_engine))?;
                    daemon.run().await?;
                }
                DaemonAction::Log { count } => {
                    let log = DailyLog::new(&daemon_config.state_dir)?;
                    let entries = log.read_today()?;
                    println!("\n📋 Today's Daemon Log ({} entries)\n", entries.len());
                    for entry in entries.iter().rev().take(count) {
                        println!("[{}] {:?}: {}", entry.timestamp.format("%H:%M:%S"), entry.kind, &entry.content[..entry.content.len().min(120)]);
                    }
                }
                DaemonAction::ResetTrust => {
                    println!("🔄 Trust reset to 100%. Restart the daemon for this to take effect.");
                }
            }
        }
        Some(Commands::Chat) | None => {
            println!("Initializing DreamSwarm chat with model: {}", config.model);
            // Interactive loop would go here in Phase 6
        }
        Some(Commands::Sessions) => {
            println!("Listing active sessions...");
            // Sessions logic
        }
    }
    
    Ok(())
}
