# DreamSwarm 🐝
**Open-source autonomous multi-agent coding platform.**
DreamSwarm is a production-grade, autonomous platform designed for complex software engineering tasks. It combines a high-performance agent loop with a 3-layer memory system, multi-agent orchestration, and the KAIROS background daemon.

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Protocol](https://img.shields.io/badge/Protocol-MCP-green.svg)](https://modelcontextprotocol.io)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-orange.svg)](CONTRIBUTING.md)

---

## 🚀 Key Features

### 🧠 3-Layer Memory & Context Compression
- **Adaptive Memory**: Pointers in Layer 1 (`MEMORY.md`), topic-specific details in Layer 2, and full history in Layer 3.
- **MicroCompact**: Zero-cost, local token trimming for long sessions.
- **autoDream**: An autonomous memory consolidation engine that merges observations and resolves contradictions during idle time.

### 🐝 Multi-Agent Swarm Orchestration
- **Dynamic Delegation**: Orchestrate dozens of agents across parallel execution backends.
- **Backends**: 
  - **Tmux**: Live terminal visibility for parallel tasks.
  - **Worktree**: Full filesystem isolation for concurrent feature development.
  - **In-Process**: Lightweight subagents for quick lookups.
- **Mailbox Communication**: Asynchronous, file-based inter-agent messaging.

### 🕒 KAIROS Background Daemon
- **Autonomous Initiative**: Watches your filesystem, git activity, and idle time to proactively run tests, fix bugs, or suggest improvements.
- **Trust-Based Autonomy**: A dynamic permission system that learns from your feedback—AutoAct, AskFirst, ObserveOnly, or Paused.
- **Daily Audit Log**: Immutable JSONL logs of every daemon action and observation.

---

## 🛠 Quick Start

### Installation
```bash
# Clone the repository
git clone https://github.com/dreamswarm/dreamswarm.git
cd dreamswarm

# One-line install (macOS/Linux)
./install.sh
```

### Usage
```bash
# Start an interactive chat
dreamswarm chat

# Start the KAIROS background daemon
dreamswarm daemon start

# Manually trigger memory consolidation
dreamswarm daemon run-dream
```

---

## 🏗 System Architecture

DreamSwarm is built on a layered architecture designed for safety and speed:

1. **Intelligence Layer**: autoDream, MicroCompact, and the 3-Layer Memory System.
2. **Orchestration Layer**: SwarmCoordinator and KAIROS Daemon.
3. **Execution Layer**: The Core Agent Loop and Tool Dispatcher.
4. **Infrastructure**: SQLite persistence, 5-layer permission gating, and multi-provider LLM support (Claude, GPT-4, Ollama).

For a deep-dive, see [ARCHITECTURE.md](ARCHITECTURE.md).

---

## 🤝 Contributing
Documentation fixes, new tools, and architectural improvements are all welcome!
- Read [CONTRIBUTING.md](CONTRIBUTING.md) for our workflow.
- Check [DEVELOPMENT.md](DEVELOPMENT.md) for environment setup.
- See [SECURITY.md](SECURITY.md) for vulnerability reporting.

---

## 📝 License
DreamSwarm is licensed under the **Apache License, Version 2.0**. See [LICENSE](LICENSE) for details.
