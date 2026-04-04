# DreamSwarm Architecture
This document explains how DreamSwarm is built, why each component exists, and how they interact. Read this before making architectural changes.

---

## System Overview
DreamSwarm is an autonomous multi-agent coding platform with five major subsystems:

```text
User Input │
           ▼
  ┌────────────────┐
  │   Agent Loop   │ ◄── The beating heart
  └───────┬────────┘
          │
  ┌───────┴───────┐
  │     Tools     │ ◄── Plugin System
  └───────────────┘
          │
          ▼
  ┌────────────────┐
  │     Swarm      │ ◄── Multi-Agent Orchestration
  └────────────────┘
          │
  ┌───────┴───────┐
  │    Memory     │ ◄── 3-Layer System
  └───────────────┘
          │
          ▼
  ┌────────────────┐
  │     KAIROS     │ ◄── Background Daemon
  └────────────────┘
```

---

## Module Map

### `runtime/` — The Core
**Files**: `agent_loop.rs`, `session.rs`, `permissions.rs`, `config.rs`
The agent loop is the central component. Every other module serves it. It handles the single-turn logic:
- `AgentLoop`: Manages the step-by-step reasoning cycle.
- `Permissions`: Enforces the 5-layer safety model.
- `Session`: Manages the dialogue and token budget.

### `tools/` — The Plugin System
**Files**: `mod.rs` + individual tool files
Every capability DreamSwarm has is a tool. Tools are stateless plugins that implement the `Tool` trait. 
- **Rule**: If a feature requires making a change to the filesystem, network, or external process, it should be a tool.

### `memory/` — 3-Layer Storage
**Files**: `index.rs`, `topics.rs`, `transcripts.rs`, `writer.rs`
Memory is designed to be bandwidth-aware:
- **Layer 1 (Index)**: Always loaded. Contains pointers and summaries.
- **Layer 2 (Topics)**: Loaded on-demand when the index matches a query.
- **Layer 3 (Transcripts)**: Never loaded directly by the agent; used only by autoDream for background consolidation.

### `swarm/` — Multi-Agent Orchestration
**Files**: `coordinator.rs`, `task_list.rs`, `mailbox.rs`, `executors/`
The swarm layer allows the agent to delegate work to parallel sub-agents and specialized teams.
- Uses **Tmux**, **Worktree**, and **In-Process** backends for execution.
- Inter-agent communication is achieved via an asynchronous **Mailbox** pattern on the filesystem.

### `daemon/` — KAIROS Daemon
**Files**: `kairos.rs`, `heartbeat.rs`, `signals.rs`, `trust.rs`
A persistent background process that takes initiative based on system signals.
- **Trust System**: Autonomy scales dynamically (AutoAct -> AskFirst -> ObserveOnly -> Paused) based on user interaction history.
- **Daily Log**: An immutable, append-only JSONL log of every action taken by the daemon.

---

## Data Flow: Single Turn (Agent Loop)
1. **Input**: User prompt is added to the session.
2. **Compression**: `ContextManager` runs `MicroCompact` to trim history.
3. **Prompt Build**: `SystemPromptBuilder` injects memory pointers and instructions.
4. **LLM Call**: `QueryEngine` streams the response.
5. **Tool Execution**: If tool calls are found:
   - `PermissionGate` checks against the 5-layer model.
   - `Tool.execute()` is called.
   - Result is added back to the session, and the loop repeats.

---

## Security Boundaries
1. **Trust Boundary 1 (User → Agent)**: Tool execution gated by the Permission System. Tools cannot bypass permissions or access other sessions.
2. **Trust Boundary 2 (Agent → Workers)**: The Mailbox pattern ensures workers request approval for dangerous operations from the lead coordinator.
3. **Trust Boundary 3 (Daemon → System)**: Proactive actions are throttled by the Trust Level. Trust degrades fast on denial and recovers slowly on approval.
4. **Trust Boundary 4 (autoDream → Memory)**: Memory consolidation runs in a **Sandbox** that cannot access the network or write to source files.

---

## Module Dependency Graph
```text
main.rs
└── runtime/agent_loop
    ├── runtime/session
    ├── runtime/permissions
    ├── runtime/config
    ├── query/engine
    │   └── query/providers/*
    ├── tools/*
    ├── memory/loader (context injection)
    ├── context/manager (compression)
    └── prompts/system (prompt builder)

daemon (KAIROS)
    ├── daemon/heartbeat
    ├── daemon/signals
    ├── daemon/initiative
    ├── daemon/trust
    └── dream (autoDream)
```
**Rule**: Dependencies flow downward. No circular dependencies are allowed.
