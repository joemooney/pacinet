-- PaciNet SQLite schema

PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);

INSERT OR IGNORE INTO schema_version (version) VALUES (1);

CREATE TABLE IF NOT EXISTS nodes (
    node_id TEXT PRIMARY KEY,
    hostname TEXT NOT NULL,
    agent_address TEXT NOT NULL,
    labels TEXT NOT NULL DEFAULT '{}',  -- JSON
    state TEXT NOT NULL DEFAULT 'registered',
    registered_at TEXT NOT NULL,
    last_heartbeat TEXT NOT NULL,
    pacgate_version TEXT NOT NULL DEFAULT '',
    uptime_seconds INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS policies (
    node_id TEXT PRIMARY KEY REFERENCES nodes(node_id) ON DELETE CASCADE,
    rules_yaml TEXT NOT NULL,
    policy_hash TEXT NOT NULL,
    deployed_at TEXT NOT NULL,
    counters_enabled INTEGER NOT NULL DEFAULT 0,
    rate_limit_enabled INTEGER NOT NULL DEFAULT 0,
    conntrack_enabled INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS policy_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    version INTEGER NOT NULL,
    node_id TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    rules_yaml TEXT NOT NULL,
    policy_hash TEXT NOT NULL,
    deployed_at TEXT NOT NULL,
    counters_enabled INTEGER NOT NULL DEFAULT 0,
    rate_limit_enabled INTEGER NOT NULL DEFAULT 0,
    conntrack_enabled INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_policy_versions_node ON policy_versions(node_id, version DESC);

CREATE TABLE IF NOT EXISTS counters (
    node_id TEXT PRIMARY KEY REFERENCES nodes(node_id) ON DELETE CASCADE,
    data TEXT NOT NULL  -- JSON array of RuleCounter
);

CREATE TABLE IF NOT EXISTS deployments (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    policy_version INTEGER NOT NULL,
    policy_hash TEXT NOT NULL,
    deployed_at TEXT NOT NULL,
    result TEXT NOT NULL,
    message TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_deployments_node ON deployments(node_id, deployed_at DESC);
