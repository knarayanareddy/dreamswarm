# DreamSwarm Development Guide 🛠️

This guide covers everything you need to build, test, and debug the DreamSwarm platform.

---

## 🏛 Project Structure

DreamSwarm is a monolithic Rust project with a clear separation of concerns:

- `src/main.rs`: Entry point and CLI command definitions.
- `src/daemon/`: The KAIROS background daemon logic.
- `src/swarm/`: Multi-agent coordination and execution strategies.
- `src/memory/`: 3-layer biological memory implementation.
- `src/runtime/`: The core autonomous agent reasoning loop.
- `src/tools/`: Extension system for file, web, and shell access.

---

## 🚀 Building from Source

### Standard Build
```bash
cargo build --release
```
The binary will be located at `./target/release/dreamswarm`.

### Development Build with Full Logging
```bash
# Enable trace-level logging during development
RUST_LOG=trace cargo run -- daemon run
```

---

## 🧪 Testing Strategy

DreamSwarm utilizes three tiers of testing to ensure hive stability.

### Tier 1: Unit Tests
Focus on individual logic components (e.g., memory compression, trust math).
```bash
cargo test --lib
```

### Tier 2: Integration Tests (`tests/`)
Verify the interaction between subsystems (e.g., Peer-to-Peer communication, Coordinator lifecycle).
```bash
cargo test --test swarm_integration
cargo test --test daemon_integration
```

### Tier 3: E2E Verification Suite
Run a full daemon cycle with mock providers to verify the entire stack.
```bash
# Run the end-to-end verification script
# (Requires tmux and git installed locally)
./scripts/verify_e2e.sh
```

---

## 🛰 The War Room: Diagnostics

The **War Room** is a specialized diagnostic mode designed to stress the system. Use this to verify stability under high-load or failure scenarios.

### Initiating a Stress Test
```bash
# Start the daemon
dreamswarm daemon start

# Trigger the War Room via the API
curl -X POST http://127.0.0.1:8080/api/v1/control/war-room
```

---

## 🧬 Neural Evolution Debugging

To inspect how prompts are evolving:

1.  **Check the Database**:
    ```bash
    sqlite3 ~/.dreamswarm/daemon/dreamswarm.db "SELECT * FROM prompt_lineage;"
    ```
2.  **Monitor Telemetry**:
    ```bash
    curl http://127.0.0.1:8080/api/v1/telemetry/history
    ```

---

## 🩹 Common Development Tasks

### Adding a New Tool
1.  Create a new file in `src/tools/`.
2.  Implement the `Tool` trait.
3.  Register the tool in `src/tools/mod.rs`.
4.  Add a unit test in the tool's module.

### Modifying the System Prompt
The system prompt is built dynamically. See `src/prompts/system.rs` and the evolution logic in `src/swarm/evolution/`.
