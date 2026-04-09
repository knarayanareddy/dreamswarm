# Contributing to DreamSwarm 🐝

Thank you for your interest in contributing to DreamSwarm! We are building the future of autonomous software engineering, and we're excited to have you join us.

## 🚀 How to Contribute

### 1. Find an Issue
- Browse our [Issue Tracker](https://github.com/dreamswarm/dreamswarm/issues).
- Look for `good first issue` for small, well-scoped tasks.
- Look for `mentored` issues if you'd like guidance from a maintainer.

### 2. Fork & Setup
- Fork the repository and clone it locally.
- Run `./install.sh` to set up the development environment.
- Run `make test` to ensure your initial setup is working correctly.

### 3. Development Workflow
- Create a new branch: `git checkout -b feature/your-feature-name`.
- **Async First**: DreamSwarm is built on `tokio`. Ensure all new code adheres to async best practices.
- **Safety First**: Any changes to `runtime/permissions.rs` or `tools/bash_tool.rs` require a security review.
- **Documentation**: If you're adding a new tool or feature, please update the relevant documentation in `docs/` or `README.md`.

### 4. Quality Standards
Before submitting a PR, please ensure:
- [ ] Code compiles without warnings.
- [ ] All tests pass (`make test`).
- [ ] Code is formatted (`cargo fmt`).
- [ ] New code has unit or integration tests.
- [ ] Doc comments are added to all public items.

### 5. Submit a PR
- Open a Pull Request against the `main` branch.
- Follow the [PULL_REQUEST_TEMPLATE](.github/PULL_REQUEST_TEMPLATE.md).
- Be prepared to discuss your implementation and make follow-up changes based on review.

## 🧠 Technical Requirements
DreamSwarm is a high-performance Rust project. Contributors should ideally be comfortable with:
- **Async Rust**: `tokio`, `async-trait`, `std::future`.
- **System Architecture**: Multi-agent systems, file-based IPC (Mailboxes).
- **Tooling**: `cargo`, `make`, `ratatui` (for TUI changes).

## 🌱 Mentorship Policy
We are committed to helping new contributors! If you're new to Rust or open-source, just ask for help in your PR or in an issue. We provide:
- **Detailed Reviews**: Not just "what" to change, but "why" from an architectural perspective.
- **Pair Programming**: On complex features, maintainers are happy to jump on a call.
- **Educational Issues**: Some issues are tagged `learn-rust` specifically to help you explore the language features.

**Happy Coding!** 🐝
