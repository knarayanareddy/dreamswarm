use clap::{Parser, Subcommand};
use dreamswarm::runtime::config::AppConfig;
use std::path::PathBuf;
use tracing_subscriber;

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
}

#[derive(Subcommand)]
enum Commands {
    /// Start the KAIROS background daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonCommand,
    },
    /// Run an interactive chat session
    Chat,
}

#[derive(Subcommand)]
enum DaemonCommand {
    Start,
    Status,
    Stop,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    
    let cli = Cli::parse();
    let config = AppConfig::new(cli.model, cli.provider, cli.mode);
    
    match cli.command {
        Some(Commands::Daemon { action }) => {
            match action {
                DaemonCommand::Start => println!("Starting daemon..."),
                DaemonCommand::Status => println!("Daemon status..."),
                DaemonCommand::Stop => println!("Stopping daemon..."),
            }
        }
        Some(Commands::Chat) | None => {
            println!("Initializing DreamSwarm chat with model: {}", config.model);
            // Wait for DB and Session to be fully implemented
            // let db = Database::new(&config.state_dir)?;
            // let session = Session::new();
            // let mut runtime = AgentRuntime::new(config, session);
            // runtime.run_interactive().await?;
        }
    }
    
    Ok(())
}
