//! Interactive REPL for the DreamSwarm agent.

use crate::runtime::agent_loop::AgentRuntime;
use colored::*;
use std::io::{self, Write};

/// Print the startup banner.
fn print_banner() {
    println!();
    println!(
        "  {} Autonomous Coding Agent",
        "DreamSwarm 🐝".bold().yellow()
    );
    println!("  {}", "─".repeat(40));
}

/// Detect user frustration signals and log them for telemetry.
fn detect_frustration_signal(input: &str) {
    let frustration_keywords = ["wrong", "no!", "stop", "undo", "revert", "why did you"];
    if frustration_keywords
        .iter()
        .any(|k| input.to_lowercase().contains(k))
    {
        tracing::warn!(
            signal = "frustration",
            input = input,
            "Frustration signal detected"
        );
    }
}

/// Detect stall signals — repetitive patterns that may indicate a reasoning loop.
fn detect_stall_signal(input: &str) {
    if input.len() < 10 {
        tracing::debug!(
            signal = "possible_stall",
            input = input,
            "Short repeated input"
        );
    }
}

/// Run the interactive REPL loop.
///
/// This is the main user-facing entry point. It reads from stdin, runs agent
/// turns, and prints responses until the user exits.
pub async fn run_interactive(mut runtime: AgentRuntime) -> anyhow::Result<()> {
    print_banner();
    println!(
        "  Session: {} | Model: configured",
        &runtime.session.id[..8]
    );
    println!(
        "  Type {} for commands, {} to exit\n",
        "/help".cyan(),
        "/quit".cyan()
    );

    loop {
        // Print the prompt
        print!("{}", " > ".green().bold());
        io::stdout().flush()?;

        // Read one line of input
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => break, // EOF — user pressed Ctrl-D
            Ok(_) => {}
            Err(e) => {
                eprintln!("Input error: {}", e);
                continue;
            }
        }

        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        // Handle slash commands
        if input.starts_with('/') {
            if input == "/quit" || input == "/exit" {
                println!("\n  Session saved. Goodbye! 👋\n");
                break;
            }
            if let Some(response) = runtime.handle_slash_command(input) {
                println!("\n{}\n", response);
            } else {
                println!("\n  Unknown command: {}\n", input);
            }
            continue;
        }

        // Telemetry signals
        detect_frustration_signal(input);
        detect_stall_signal(input);

        // Run the agent turn
        println!();
        println!("  {} Thinking...", "⟳".dimmed());

        let on_approval = |name: String, input: serde_json::Value| async move {
            println!("\n  ⚠️  {} requests permission to use: {}", "Bee".bold().yellow(), name.bold().red());
            println!("  Input: {}", input);
            print!("  {} Approve this action? (y/N): ", "›".bold().yellow());
            io::stdout().flush().ok();
            
            let mut answer = String::new();
            io::stdin().read_line(&mut answer).ok();
            answer.trim().to_lowercase() == "y"
        };

        match runtime.run_turn(input, on_approval).await {
            Ok(result) => {
                println!();
                println!("  {}", "─".repeat(60));
                println!();
                for line in result.final_text.lines() {
                    println!("  {}", line);
                }
                println!();
                println!(
                    "  📊 {} tool calls | {} tokens | ${:.4}",
                    result.tool_calls_made.len(),
                    result.tokens_used,
                    result.cost_usd,
                );
                println!();
            }
            Err(e) => {
                eprintln!("\n  ✗ Error: {}\n", e);
            }
        }
    }

    Ok(())
}
