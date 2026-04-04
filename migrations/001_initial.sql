-- DreamSwarm Initial Schema 🐝
-- Phase 1 - Core Tables
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    model TEXT NOT NULL,
    provider TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    turn_count INTEGER DEFAULT 0,
    total_tokens INTEGER DEFAULT 0,
    total_cost_usd REAL DEFAULT 0.0,
    summary TEXT
);

CREATE TABLE IF NOT EXISTS tool_executions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    input TEXT NOT NULL,
    output TEXT NOT NULL,
    is_error BOOLEAN DEFAULT 0,
    risk_level TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS permission_decisions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    command TEXT NOT NULL,
    decision TEXT NOT NULL,
    reason TEXT,
    created_at DATETIME NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS cost_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_usd REAL NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY(session_id) REFERENCES sessions(id)
);

-- Phase 2 - Metrics & Context
CREATE TABLE IF NOT EXISTS session_metrics (
    session_id TEXT PRIMARY KEY,
    avg_response_time REAL,
    max_context_usage REAL,
    compression_count INTEGER DEFAULT 0,
    FOREIGN KEY(session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS config_snapshots (
    id TEXT PRIMARY KEY,
    config_json TEXT NOT NULL,
    created_at DATETIME NOT NULL
);

-- Phase 3 - Multi-Agent Swarm
CREATE TABLE IF NOT EXISTS teams (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    mission TEXT NOT NULL,
    coordinator_session_id TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

CREATE TABLE IF NOT EXISTS team_members (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    role TEXT NOT NULL,
    status TEXT NOT NULL,
    FOREIGN KEY(team_id) REFERENCES teams(id)
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    description TEXT NOT NULL,
    assigned_to TEXT,
    status TEXT NOT NULL,
    dependencies TEXT, -- JSON array of task IDs
    result TEXT,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    FOREIGN KEY(team_id) REFERENCES teams(id)
);

-- Phase 4 - KAIROS Daemon
CREATE TABLE IF NOT EXISTS daemon_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    event_data TEXT NOT NULL,
    trust_level REAL NOT NULL,
    initiative_score REAL,
    action_taken TEXT,
    created_at DATETIME NOT NULL
);

-- Phase 5 - autoDream
CREATE TABLE IF NOT EXISTS dream_reports (
    id TEXT PRIMARY KEY,
    cycle_id TEXT NOT NULL,
    consolidated_observations INTEGER NOT NULL,
    conflicts_resolved INTEGER NOT NULL,
    new_facts INTEGER NOT NULL,
    report_json TEXT NOT NULL,
    created_at DATETIME NOT NULL
);
