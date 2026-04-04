# Contributing to DreamSwarm 🐝

Thank you for your interest in contributing to DreamSwarm! This project exists because of contributors like you—whether you're fixing a typo, reporting a bug, adding a feature, or improving documentation.

---

## 🗺️ Table of Contents
- [Code of Conduct](#code-of-conduct)
- [Ways to Contribute](#ways-to-contribute)
- [Quick Start](#quick-start)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Submitting a Pull Request](#submitting-a-pull-request)
- [Review Process](#review-process)
- [Architecture Overview](#architecture-overview)

---

## Code of Conduct
This project follows our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold a welcoming, inclusive, and respectful community. 
**TL;DR:** Be kind. Assume good intent. Help others learn.

---

## Ways to Contribute

### 🐛 Report Bugs
Found something broken? [Open a bug report](https://github.com/dreamswarm/dreamswarm/issues/new?template=bug_report.yml). Include:
- What you expected vs. what happened.
- Steps to reproduce.
- Your OS and Rust version.
- Relevant log output.

### ✨ Suggest Features
Have an idea? [Open a feature request](https://github.com/dreamswarm/dreamswarm/issues/new?template=feature_request.yml). Describe the problem you're solving, not just the solution you want.

### 📖 Improve Documentation
No PR is too small! Typo fixes, clarifications, and new examples are all valuable.

### 🧪 Fix Bugs & Add Features
Browse [issues labeled `bug`](https://github.com/dreamswarm/dreamswarm/labels/bug) or [`good first issue`](https://github.com/dreamswarm/dreamswarm/labels/good%20first%20issue). For larger features (> 200 lines), open an issue first to discuss the design.

---

## Quick Start
If you want to make your first contribution *right now*, here's the fastest path:
```bash
# 1. Fork and clone
git clone https://github.com/YOUR_USERNAME/dreamswarm.git
cd dreamswarm

# 2. Create a branch
git checkout -b fix/my-improvement

# 3. Make your change (even a typo fix counts!)

# 4. Verify it works
make fmt lint test

# 5. Commit with a conventional commit message
git commit -m "fix: correct typo in permission gate error message"

# 6. Push and open a PR
git push origin fix/my-improvement
```

---

## Development Setup
**Prerequisites**:
- **Rust**: 1.77.0+ 
- **Git**: 2.30+
- **tmux**: 3.0+ (Required for Multi-Agent swarm testing)
- **ripgrep**: 13.0+ (Fast file searching)

```bash
# Linux (Debian/Ubuntu)
sudo apt-get install -y tmux ripgrep
# macOS
brew install tmux ripgrep
```

---

## Making Changes

### Branch Naming
Use descriptive branch names with a category prefix:
- `feat/multi-model-routing`
- `fix/bash-security-bypass`
- `docs/tool-authoring-guide`
- `refactor/query-engine-cleanup`

### Commit Messages
We use **Conventional Commits**. This enables automatic changelog generation.
Format: `<type>(<scope>): <description>`
Types: `feat`, `fix`, `docs`, `refactor`, `test`, `perf`, `ci`, `chore`, `security`.

---

## Review Process
We review for:
- **Correctness**: Does it do what it claims?
- **Safety**: Could this introduce a security issue?
- **Tests**: Is the new code tested?
- **Architecture**: Does it fit the existing patterns? (See [ARCHITECTURE.md](ARCHITECTURE.md))
- **Simplicity**: Is this the simplest correct solution?

---

## Mentorship
If you're new to Rust or open-source contribution, look for issues tagged `mentored`. A maintainer will pair with you on these issues, providing detailed feedback on your PR and guidance on implementation.

**Thank you for helping build DreamSwarm!** 🐝
