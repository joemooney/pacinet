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
    capabilities TEXT NOT NULL DEFAULT '{}',  -- JSON
    state TEXT NOT NULL DEFAULT 'registered',
    registered_at TEXT NOT NULL,
    last_heartbeat TEXT NOT NULL,
    pacgate_version TEXT NOT NULL DEFAULT '',
    uptime_seconds INTEGER NOT NULL DEFAULT 0,
    annotations TEXT NOT NULL DEFAULT '{}'  -- JSON
);

CREATE TABLE IF NOT EXISTS policies (
    node_id TEXT PRIMARY KEY REFERENCES nodes(node_id) ON DELETE CASCADE,
    rules_yaml TEXT NOT NULL,
    policy_hash TEXT NOT NULL,
    deployed_at TEXT NOT NULL,
    counters_enabled INTEGER NOT NULL DEFAULT 0,
    rate_limit_enabled INTEGER NOT NULL DEFAULT 0,
    conntrack_enabled INTEGER NOT NULL DEFAULT 0,
    axi_enabled INTEGER NOT NULL DEFAULT 0,
    ports INTEGER NOT NULL DEFAULT 1,
    target TEXT NOT NULL DEFAULT 'standalone',
    dynamic INTEGER NOT NULL DEFAULT 0,
    dynamic_entries INTEGER NOT NULL DEFAULT 16,
    width INTEGER NOT NULL DEFAULT 8,
    ptp INTEGER NOT NULL DEFAULT 0,
    rss INTEGER NOT NULL DEFAULT 0,
    rss_queues INTEGER NOT NULL DEFAULT 4,
    int_enabled INTEGER NOT NULL DEFAULT 0,
    int_switch_id INTEGER NOT NULL DEFAULT 0
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
    conntrack_enabled INTEGER NOT NULL DEFAULT 0,
    axi_enabled INTEGER NOT NULL DEFAULT 0,
    ports INTEGER NOT NULL DEFAULT 1,
    target TEXT NOT NULL DEFAULT 'standalone',
    dynamic INTEGER NOT NULL DEFAULT 0,
    dynamic_entries INTEGER NOT NULL DEFAULT 16,
    width INTEGER NOT NULL DEFAULT 8,
    ptp INTEGER NOT NULL DEFAULT 0,
    rss INTEGER NOT NULL DEFAULT 0,
    rss_queues INTEGER NOT NULL DEFAULT 4,
    int_enabled INTEGER NOT NULL DEFAULT 0,
    int_switch_id INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_policy_versions_node ON policy_versions(node_id, version DESC);

CREATE TABLE IF NOT EXISTS counters (
    node_id TEXT PRIMARY KEY REFERENCES nodes(node_id) ON DELETE CASCADE,
    data TEXT NOT NULL  -- JSON array of RuleCounter
);

CREATE TABLE IF NOT EXISTS flow_counters (
    node_id TEXT PRIMARY KEY REFERENCES nodes(node_id) ON DELETE CASCADE,
    data TEXT NOT NULL  -- JSON array of FlowCounter
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

-- Phase 5: FSM tables

CREATE TABLE IF NOT EXISTS fsm_definitions (
    name TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    definition_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS fsm_instances (
    instance_id TEXT PRIMARY KEY,
    definition_name TEXT NOT NULL,
    status TEXT NOT NULL,
    instance_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_fsm_instances_status ON fsm_instances(status);
CREATE INDEX IF NOT EXISTS idx_fsm_instances_def ON fsm_instances(definition_name);

-- Phase 8: Event log
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    source TEXT NOT NULL,
    payload TEXT NOT NULL,
    timestamp TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);

-- Phase 8: Leader lease for HA
CREATE TABLE IF NOT EXISTS leader_lease (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    controller_id TEXT NOT NULL,
    lease_expires_at TEXT NOT NULL,
    acquired_at TEXT NOT NULL
);

-- Phase 9: Audit log
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    details TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action);

-- Phase 9: Policy templates
CREATE TABLE IF NOT EXISTS policy_templates (
    name TEXT PRIMARY KEY,
    description TEXT NOT NULL DEFAULT '',
    rules_yaml TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Phase 9: Webhook delivery history
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL,
    url TEXT NOT NULL,
    method TEXT NOT NULL DEFAULT 'POST',
    status_code INTEGER,
    success INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    attempt INTEGER NOT NULL DEFAULT 0,
    timestamp TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_webhook_instance ON webhook_deliveries(instance_id, timestamp DESC);
