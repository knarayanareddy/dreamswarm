# Development Guide
This document explains how to set up your environment and contribute to DreamSwarm.

---

## 🛠️ Environment Setup

### Quick Setup (Recommended)
```bash
# Clone and enter the repo
git clone https://github.com/dreamswarm/dreamswarm.git
cd dreamswarm

# Run the setup script
./install.sh

# Verify
make check test
```

### Manual Setup
1. **Rust Toolchain**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. **System Dependencies**:
   - `tmux`: For Multi-Agent swarm testing.
   - `ripgrep`: For `GrepTool` functionality.
   - `git`: For version control and worktree support.
3. **API Keys**:
   - `ANTHROPIC_API_KEY`: Required for Claude.
   - `OPENAI_API_KEY`: Required for GPT-4.
   - `OLLAMA_BASE_URL`: For local models (Ollama).

---

## 🏗️ Common Development Workflows

### Adding a New Tool
1. **Create the tool file**: `src/tools/my_new_tool.rs`.
2. **Implement the `Tool` trait**:
   - Define a unique `name`.
   - Provide a clear `description` for the LLM.
   - Define a JSON `input_schema`.
   - Implement the `execute` method.
3. **Register the tool**: Add it to `ToolRegistry::default_phaseN()` in `src/tools/mod.rs`.
4. **Add tests**: Always include tests for both success and error cases.

### Adding a New LLM Provider
1. **Create the provider file**: `src/query/providers/my_provider.rs`.
2. **Implement the `LLMProvider` trait**.
3. **Register in `QueryEngine::new()`**.

---

## 🧪 Testing

### Test Organization
- **Unit tests**: Located in the same file as the code they test (`#[cfg(test)]`).
- **Integration tests**: Located in the `tests/` directory.

### Running Tests
```bash
# All tests
make test

# Unit tests only (fast)
make test-unit

# Integration tests only
make test-integration

# With coverage (requires cargo-llvm-cov)
make coverage
```

---

## 📜 Code Style
- **Formatting**: `cargo fmt` is non-negotiable.
- **Lints**: `cargo clippy --pedantic` is required. All warnings must be resolved.
- **Doc Comments**: Every public function, struct, and trait must have `///` comments.
- **Async**: Use the `tokio` runtime.
- **Errors**: use `anyhow::Result` in application code, `thiserror` for library errors. Never `unwrap()` in non-test code.

---

## 🐞 Debugging
- **Verbose logging**: `RUST_LOG=dreamswarm=debug cargo run`.
- **Module-specific**: `RUST_LOG=dreamswarm::daemon=trace,dreamswarm::tools=debug cargo run`.
- **Log to file**: `RUST_LOG=dreamswarm=debug cargo run 2>&1 | tee debug.log`.

---

## ❓ Common Issues
- **"ANTHROPIC_API_KEY not set"**: `export ANTHROPIC_API_KEY="sk-ant-..."`.
- **"tmux not found"**: Integration tests will fail. Install `tmux`.
- **"ripgrep not found"**: Grep tool will fail. Install `ripgrep`.
- **"Tests hang/timeout"**: Run with `--test-threads=1` to isolate sequential issues.
