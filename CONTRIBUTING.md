# Contributing to DreamSwarm 🐝

Thank you for your interest in contributing to DreamSwarm! We are building the next generation of autonomous engineering, and we're excited to have you join us.

---

## 🧭 Contribution Path

1.  **Exploration**: Read the [README.md](README.md) and [ARCHITECTURE.md](ARCHITECTURE.md).
2.  **Setup**: Follow the [Development Guide](DEVELOPMENT.md) to set up your local environment.
3.  **Communication**: Open an issue to discuss major changes before starting work.
4.  **Implementation**: Submit a PR from a feature branch.
5.  **Review**: Collaborate with the core maintainers on the PR.

---

## 🛠 Developer Setup

### Prerequisites
- **Rust Toolchain**: [Install Rust](https://rustup.rs/) (latest stable).
- **Git**: Required for worktree isolation features.
- **Tmux**: Required for the `TmuxExecutor`.
- **Database**: SQLite (usually included with most modern OSs).

### Getting Started
```bash
# Clone the repo
git clone https://github.com/dreamswarm/dreamswarm.git
cd dreamswarm

# Run tests to ensure everything is green
cargo test
```

---

## 📐 Coding Standards

### 1. Safety First
- We prioritize memory safety. Avoid `unsafe` blocks unless absolutely necessary for external bindings.
- All tool executions must be gated through the `PermissionGate`.

### 2. Error Handling
- Use `anyhow::Result` for application-level errors.
- Use `thiserror` for library-level error definitions.
- Never use `unwrap()` or `expect()` in production code pathways; handle errors gracefully.

### 3. Asynchrony
- DreamSwarm is built on **Tokio**. Use `tokio::spawn` for background tasks and `tokio::sync` for communication.

### 4. Logging & Telemetry
- Use the `tracing` crate for all logging.
- Any autonomous action taken by the KAIROS daemon MUST be logged at the `INFO` level and recorded in the SQLite telemetry table.

---

## 🧪 Testing Requirements

We maintain a strict testing culture:
- **Unit Tests**: Required for every new module in `src/`.
- **Integration Tests**: Required for any change to the Swarm Coordination or Memory layers.
- **Documentation Tests**: We encourage using doc-comments with examples.

Run the full suite:
```bash
cargo test --all-features
```

---

## 📝 Pull Request Template

When opening a PR, please include:
1.  **Context**: What problem does this solve?
2.  **Changes**: A high-level bullet list of what was added/modified.
3.  **Verification**: Links to test results or screenshots showing the feature in action.
4.  **Safety**: Confirmation that no security boundaries were bypassed.

---

## 📜 Code of Conduct

Please be respectful and professional in all communications. We follow the [Contributor Covenant](CODE_OF_CONDUCT.md).

---

## 🏅 Recognition

Contributors who land significant features or architectural improvements will be invited to the **DreamSwarm Core Hive**.
