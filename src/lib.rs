pub mod runtime;
pub mod query;
pub mod tools;
pub mod memory;
pub mod context;
pub mod swarm;
pub mod daemon;
pub mod dream;
pub mod db;
pub mod prompts;
pub mod tui;

// Convenience re-exports used by integration tests and the CLI
pub use runtime::agent_loop::AgentRuntime;
pub use runtime::config::AppConfig;
pub use runtime::session::Session;
pub use query::engine::QueryEngine;
pub use tools::ToolRegistry;
pub use db::Database;
pub use memory::MemorySystem;
