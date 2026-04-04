pub const INITIAL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    messages TEXT NOT NULL, -- JSON array of Message objects
    created_at TEXT NOT NULL, -- ISO 8601
    updated_at TEXT NOT NULL, -- ISO 8601
    turn_count INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    total_cost_usd REAL NOT NULL DEFAULT 0.0,
    summary TEXT, -- Auto-generated summary
    model TEXT, -- Primary model used
    provider TEXT, -- anthropic, openai, etc.
    permission_mode TEXT DEFAULT 'default', -- Mode at session start
    working_dir TEXT, -- Project directory
    parent_session_id TEXT, -- For subagent sessions
    status TEXT DEFAULT 'active', -- active, completed, aborted
    FOREIGN KEY (parent_session_id) REFERENCES sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_sessions_parent ON sessions(parent_session_id);

CREATE TABLE IF NOT EXISTS tool_executions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_number INTEGER NOT NULL,
    tool_name TEXT NOT NULL,
    input TEXT NOT NULL, -- JSON input
    output TEXT, -- Tool output (may be large)
    is_error BOOLEAN NOT NULL DEFAULT 0,
    duration_ms INTEGER, -- Execution time
    executed_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_tool_exec_session ON tool_executions(session_id);
CREATE INDEX IF NOT EXISTS idx_tool_exec_name ON tool_executions(tool_name);
CREATE INDEX IF NOT EXISTS idx_tool_exec_error ON tool_executions(is_error) WHERE is_error = 1;

CREATE TABLE IF NOT EXISTS permission_decisions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_number INTEGER NOT NULL,
    tool_name TEXT NOT NULL,
    command_signature TEXT NOT NULL,
    risk_level TEXT NOT NULL, -- Safe, Moderate, Dangerous, Critical
    decision TEXT NOT NULL, -- Allow, Deny, AskApproved, AskDenied
    decided_by TEXT NOT NULL, -- auto, user, blocklist, allowlist
    decided_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_perm_session ON permission_decisions(session_id);
CREATE INDEX IF NOT EXISTS idx_perm_decision ON permission_decisions(decision);

CREATE TABLE IF NOT EXISTS cost_log (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_number INTEGER NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_usd REAL NOT NULL,
    model TEXT NOT NULL,
    cache_hits INTEGER DEFAULT 0,
    cache_misses INTEGER DEFAULT 0,
    logged_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_cost_session ON cost_log(session_id);
CREATE INDEX IF NOT EXISTS idx_cost_date ON cost_log(logged_at);

CREATE VIEW IF NOT EXISTS daily_costs AS
    SELECT DATE(logged_at) as date,
    SUM(cost_usd) as total_cost,
    SUM(input_tokens) as total_input_tokens,
    SUM(output_tokens) as total_output_tokens,
    COUNT(DISTINCT session_id) as session_count
    FROM cost_log
    GROUP BY DATE(logged_at)
    ORDER BY date DESC;

CREATE TABLE IF NOT EXISTS session_metrics (
    session_id TEXT PRIMARY KEY,
    continue_count INTEGER DEFAULT 0, -- "continue" / stall signals
    frustration_signals INTEGER DEFAULT 0, -- Detected frustration
    consecutive_denials INTEGER DEFAULT 0, -- Max consecutive denials
    compaction_events INTEGER DEFAULT 0, -- Context compressions
    cache_breaks INTEGER DEFAULT 0, -- Prompt cache invalidations
    avg_turn_latency_ms INTEGER DEFAULT 0, -- Average LLM response time
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS config_snapshots (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    config_json TEXT NOT NULL, -- Full AppConfig as JSON
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS teams (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    lead_session_id TEXT NOT NULL,
    status TEXT DEFAULT 'active', -- active, completed, failed
    created_at TEXT NOT NULL,
    completed_at TEXT,
    config_json TEXT, -- TeamConfig
    FOREIGN KEY (lead_session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS team_members (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    session_id TEXT,
    role TEXT NOT NULL, -- lead, worker, reviewer
    status TEXT DEFAULT 'idle', -- idle, active, completed
    spawn_type TEXT NOT NULL, -- in_process, tmux, worktree
    worktree_path TEXT,
    branch_name TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (team_id) REFERENCES teams(id),
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT DEFAULT 'pending', -- pending, claimed, in_progress, completed, failed
    assigned_to TEXT, -- team_member id
    dependencies TEXT, -- JSON array of task ids
    result TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (team_id) REFERENCES teams(id),
    FOREIGN KEY (assigned_to) REFERENCES team_members(id)
);

CREATE TABLE IF NOT EXISTS daemon_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- Sequential, never deleted
    kind TEXT NOT NULL, -- observation, decision, action, error, dream
    content TEXT NOT NULL,
    tools_used TEXT, -- JSON array
    tokens_used INTEGER DEFAULT 0,
    cost_usd REAL DEFAULT 0.0,
    logged_at TEXT NOT NULL
);

CREATE TRIGGER IF NOT EXISTS prevent_daemon_log_delete
    BEFORE DELETE ON daemon_log
    BEGIN
        SELECT RAISE(ABORT, 'daemon_log is append-only: deletions are not permitted');
    END;

CREATE TRIGGER IF NOT EXISTS prevent_daemon_log_update
    BEFORE UPDATE ON daemon_log
    BEGIN
        SELECT RAISE(ABORT, 'daemon_log is append-only: updates are not permitted');
    END;

CREATE TABLE IF NOT EXISTS dream_reports (
    id TEXT PRIMARY KEY,
    entries_merged INTEGER DEFAULT 0,
    contradictions_resolved INTEGER DEFAULT 0,
    entries_pruned INTEGER DEFAULT 0,
    memory_before_hash TEXT,
    memory_after_hash TEXT,
    dreamed_at TEXT NOT NULL
);
"#;
