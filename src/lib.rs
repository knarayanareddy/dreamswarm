#![allow(clippy::ptr_arg, clippy::manual_strip, clippy::vec_init_then_push, clippy::field_reassign_with_default)]
pub mod context;
pub mod daemon;
pub mod db;
pub mod dream;
pub mod memory;
pub mod prompts;
pub mod query;
pub mod runtime;
pub mod swarm;
pub mod tools;
pub mod tui;

// Convenience re-exports used by integration tests and the CLI
pub use db::Database;
pub use memory::MemorySystem;
pub use query::engine::QueryEngine;
pub use runtime::agent_loop::AgentRuntime;
pub use runtime::config::AppConfig;
pub use runtime::session::Session;
pub use tools::ToolRegistry;
