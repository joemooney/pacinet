use chrono::{DateTime, Utc};
use pacinet_core::error::PaciNetError;
use pacinet_core::model::*;
use pacinet_core::Storage;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

const SCHEMA: &str = include_str!("schema.sql");

pub struct SqliteStorage {
    conn: Mutex<Connection>,
    deploying: std::sync::RwLock<HashSet<String>>,
}

impl SqliteStorage {
    pub fn open(path: &str) -> Result<Self, PaciNetError> {
        let conn = Connection::open(path)
            .map_err(|e| PaciNetError::Internal(format!("Failed to open database: {}", e)))?;
        conn.execute_batch(SCHEMA)
            .map_err(|e| PaciNetError::Internal(format!("Failed to initialize schema: {}", e)))?;
        Ok(Self {
            conn: Mutex::new(conn),
            deploying: std::sync::RwLock::new(HashSet::new()),
        })
    }

    pub fn in_memory() -> Result<Self, PaciNetError> {
        Self::open(":memory:")
    }
}

impl Storage for SqliteStorage {
    fn register_node(&self, node: Node) -> Result<String, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let labels_json =
            serde_json::to_string(&node.labels).map_err(|e| PaciNetError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO nodes (node_id, hostname, agent_address, labels, state, registered_at, last_heartbeat, pacgate_version, uptime_seconds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                &node.node_id,
                &node.hostname,
                &node.agent_address,
                &labels_json,
                &node.state.to_string(),
                node.registered_at.to_rfc3339(),
                node.last_heartbeat.to_rfc3339(),
                &node.pacgate_version,
                node.uptime_seconds as i64,
            ],
        )
        .map_err(|e| PaciNetError::Internal(format!("Failed to insert node: {}", e)))?;
        Ok(node.node_id)
    }

    fn get_node(&self, node_id: &str) -> Result<Option<Node>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT node_id, hostname, agent_address, labels, state, registered_at, last_heartbeat, pacgate_version, uptime_seconds
                 FROM nodes WHERE node_id = ?1",
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let node = stmt
            .query_row(rusqlite::params![node_id], |row| {
                Ok(row_to_node(row))
            })
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        match node {
            Some(Ok(n)) => Ok(Some(n)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    fn list_nodes(&self, label_filter: &HashMap<String, String>) -> Result<Vec<Node>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT node_id, hostname, agent_address, labels, state, registered_at, last_heartbeat, pacgate_version, uptime_seconds
                 FROM nodes",
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let nodes: Vec<Node> = stmt
            .query_map([], |row| Ok(row_to_node(row)))
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|r| r.ok())
            .filter(|node| {
                label_filter
                    .iter()
                    .all(|(k, v)| node.labels.get(k) == Some(v))
            })
            .collect();

        Ok(nodes)
    }

    fn remove_node(&self, node_id: &str) -> Result<bool, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        // CASCADE handles policies, counters, deployments, policy_versions
        let affected = conn
            .execute("DELETE FROM nodes WHERE node_id = ?1", rusqlite::params![node_id])
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        self.deploying.write().unwrap().remove(node_id);
        Ok(affected > 0)
    }

    fn update_heartbeat(
        &self,
        node_id: &str,
        state: NodeState,
        uptime: u64,
    ) -> Result<bool, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE nodes SET last_heartbeat = ?1, state = ?2, uptime_seconds = ?3 WHERE node_id = ?4",
                rusqlite::params![
                    Utc::now().to_rfc3339(),
                    state.to_string(),
                    uptime as i64,
                    node_id,
                ],
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        Ok(affected > 0)
    }

    fn update_node_state(&self, node_id: &str, state: NodeState) -> Result<bool, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        // Check current state for transition validation
        let current_state: Option<String> = conn
            .query_row(
                "SELECT state FROM nodes WHERE node_id = ?1",
                rusqlite::params![node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        match current_state {
            None => Ok(false),
            Some(current) => {
                let current_state: NodeState = current
                    .parse()
                    .map_err(|e: String| PaciNetError::Internal(e))?;
                if !current_state.can_transition_to(&state) {
                    return Err(PaciNetError::InvalidStateTransition {
                        from: current_state.to_string(),
                        to: state.to_string(),
                    });
                }
                conn.execute(
                    "UPDATE nodes SET state = ?1 WHERE node_id = ?2",
                    rusqlite::params![state.to_string(), node_id],
                )
                .map_err(|e| PaciNetError::Internal(e.to_string()))?;
                Ok(true)
            }
        }
    }

    fn store_counters(&self, node_id: &str, counters: Vec<RuleCounter>) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(&counters)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO counters (node_id, data) VALUES (?1, ?2)",
            rusqlite::params![node_id, json],
        )
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        Ok(())
    }

    fn get_counters(&self, node_id: &str) -> Result<Option<Vec<RuleCounter>>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT data FROM counters WHERE node_id = ?1",
                rusqlite::params![node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        match json {
            None => Ok(None),
            Some(j) => {
                let counters: Vec<RuleCounter> = serde_json::from_str(&j)
                    .map_err(|e| PaciNetError::Internal(e.to_string()))?;
                Ok(Some(counters))
            }
        }
    }

    fn store_policy(&self, policy: Policy) -> Result<u64, PaciNetError> {
        let conn = self.conn.lock().unwrap();

        // Get next version number
        let max_version: Option<i64> = conn
            .query_row(
                "SELECT MAX(version) FROM policy_versions WHERE node_id = ?1",
                rusqlite::params![&policy.node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .flatten();

        let version = (max_version.unwrap_or(0) + 1) as u64;

        // Insert into policy_versions
        conn.execute(
            "INSERT INTO policy_versions (version, node_id, rules_yaml, policy_hash, deployed_at, counters_enabled, rate_limit_enabled, conntrack_enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                version as i64,
                &policy.node_id,
                &policy.rules_yaml,
                &policy.policy_hash,
                policy.deployed_at.to_rfc3339(),
                policy.counters_enabled as i32,
                policy.rate_limit_enabled as i32,
                policy.conntrack_enabled as i32,
            ],
        )
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        // Upsert current policy
        conn.execute(
            "INSERT OR REPLACE INTO policies (node_id, rules_yaml, policy_hash, deployed_at, counters_enabled, rate_limit_enabled, conntrack_enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                &policy.node_id,
                &policy.rules_yaml,
                &policy.policy_hash,
                policy.deployed_at.to_rfc3339(),
                policy.counters_enabled as i32,
                policy.rate_limit_enabled as i32,
                policy.conntrack_enabled as i32,
            ],
        )
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        Ok(version)
    }

    fn get_policy(&self, node_id: &str) -> Result<Option<Policy>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let policy = conn
            .query_row(
                "SELECT node_id, rules_yaml, policy_hash, deployed_at, counters_enabled, rate_limit_enabled, conntrack_enabled
                 FROM policies WHERE node_id = ?1",
                rusqlite::params![node_id],
                |row| Ok(row_to_policy(row)),
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        match policy {
            Some(Ok(p)) => Ok(Some(p)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    fn get_policy_history(
        &self,
        node_id: &str,
        limit: u32,
    ) -> Result<Vec<PolicyVersion>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT version, node_id, rules_yaml, policy_hash, deployed_at, counters_enabled, rate_limit_enabled, conntrack_enabled
                 FROM policy_versions WHERE node_id = ?1 ORDER BY version DESC LIMIT ?2",
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let versions: Vec<PolicyVersion> = stmt
            .query_map(rusqlite::params![node_id, limit], |row| {
                Ok(row_to_policy_version(row))
            })
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|r| r.ok())
            .collect();

        Ok(versions)
    }

    fn get_policies_for_nodes(
        &self,
        node_ids: &[String],
    ) -> Result<HashMap<String, Policy>, PaciNetError> {
        if node_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.conn.lock().unwrap();
        let placeholders: Vec<String> = (1..=node_ids.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "SELECT node_id, rules_yaml, policy_hash, deployed_at, counters_enabled, rate_limit_enabled, conntrack_enabled
             FROM policies WHERE node_id IN ({})",
            placeholders.join(", ")
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let params: Vec<&dyn rusqlite::types::ToSql> = node_ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();

        let policies: HashMap<String, Policy> = stmt
            .query_map(params.as_slice(), |row| Ok(row_to_policy(row)))
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|r| r.ok())
            .map(|p| (p.node_id.clone(), p))
            .collect();

        Ok(policies)
    }

    fn record_deployment(&self, record: DeploymentRecord) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO deployments (id, node_id, policy_version, policy_hash, deployed_at, result, message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                &record.id,
                &record.node_id,
                record.policy_version as i64,
                &record.policy_hash,
                record.deployed_at.to_rfc3339(),
                record.result.to_string(),
                &record.message,
            ],
        )
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        Ok(())
    }

    fn get_deployments(
        &self,
        node_id: &str,
        limit: u32,
    ) -> Result<Vec<DeploymentRecord>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, node_id, policy_version, policy_hash, deployed_at, result, message
                 FROM deployments WHERE node_id = ?1 ORDER BY deployed_at DESC LIMIT ?2",
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let records: Vec<DeploymentRecord> = stmt
            .query_map(rusqlite::params![node_id, limit], |row| {
                Ok(row_to_deployment(row))
            })
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    fn begin_deploy(&self, node_id: &str) -> Result<(), PaciNetError> {
        let mut deploying = self.deploying.write().unwrap();
        if !deploying.insert(node_id.to_string()) {
            return Err(PaciNetError::ConcurrentDeploy(node_id.to_string()));
        }
        Ok(())
    }

    fn end_deploy(&self, node_id: &str) {
        self.deploying.write().unwrap().remove(node_id);
    }

    fn mark_stale_nodes(
        &self,
        threshold: chrono::Duration,
    ) -> Result<Vec<String>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let cutoff = (Utc::now() - threshold).to_rfc3339();
        let mut stmt = conn
            .prepare(
                "SELECT node_id FROM nodes WHERE state != 'offline' AND state != 'registered' AND last_heartbeat < ?1",
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let stale_ids: Vec<String> = stmt
            .query_map(rusqlite::params![cutoff], |row| row.get(0))
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        for id in &stale_ids {
            conn.execute(
                "UPDATE nodes SET state = 'offline' WHERE node_id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        }

        Ok(stale_ids)
    }

    fn status_summary(
        &self,
    ) -> Result<(usize, HashMap<String, usize>, Option<DateTime<Utc>>), PaciNetError> {
        let conn = self.conn.lock().unwrap();

        let total: usize = conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let mut stmt = conn
            .prepare("SELECT state, COUNT(*) FROM nodes GROUP BY state")
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        let by_state: HashMap<String, usize> = stmt
            .query_map([], |row| {
                let state: String = row.get(0)?;
                let count: usize = row.get(1)?;
                Ok((state, count))
            })
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        let oldest: Option<String> = conn
            .query_row(
                "SELECT MIN(last_heartbeat) FROM nodes",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .flatten();

        let oldest_dt = oldest.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc)));

        Ok((total, by_state, oldest_dt))
    }
}

// Helper functions to convert rusqlite rows to domain types

fn row_to_node(row: &rusqlite::Row) -> Result<Node, PaciNetError> {
    let node_id: String = row.get(0).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let hostname: String = row.get(1).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let agent_address: String = row.get(2).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let labels_json: String = row.get(3).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let state_str: String = row.get(4).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let registered_at_str: String = row.get(5).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let last_heartbeat_str: String = row.get(6).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let pacgate_version: String = row.get(7).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let uptime_seconds: i64 = row.get(8).map_err(|e| PaciNetError::Internal(e.to_string()))?;

    let labels: HashMap<String, String> =
        serde_json::from_str(&labels_json).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let state: NodeState = state_str
        .parse()
        .map_err(|e: String| PaciNetError::Internal(e))?;
    let registered_at = DateTime::parse_from_rfc3339(&registered_at_str)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?
        .with_timezone(&Utc);
    let last_heartbeat = DateTime::parse_from_rfc3339(&last_heartbeat_str)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?
        .with_timezone(&Utc);

    Ok(Node {
        node_id,
        hostname,
        agent_address,
        labels,
        state,
        registered_at,
        last_heartbeat,
        pacgate_version,
        uptime_seconds: uptime_seconds as u64,
    })
}

fn row_to_policy(row: &rusqlite::Row) -> Result<Policy, PaciNetError> {
    let node_id: String = row.get(0).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let rules_yaml: String = row.get(1).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let policy_hash: String = row.get(2).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let deployed_at_str: String = row.get(3).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let counters: i32 = row.get(4).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let rate_limit: i32 = row.get(5).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let conntrack: i32 = row.get(6).map_err(|e| PaciNetError::Internal(e.to_string()))?;

    let deployed_at = DateTime::parse_from_rfc3339(&deployed_at_str)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?
        .with_timezone(&Utc);

    Ok(Policy {
        node_id,
        rules_yaml,
        policy_hash,
        deployed_at,
        counters_enabled: counters != 0,
        rate_limit_enabled: rate_limit != 0,
        conntrack_enabled: conntrack != 0,
    })
}

fn row_to_policy_version(row: &rusqlite::Row) -> Result<PolicyVersion, PaciNetError> {
    let version: i64 = row.get(0).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let node_id: String = row.get(1).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let rules_yaml: String = row.get(2).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let policy_hash: String = row.get(3).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let deployed_at_str: String = row.get(4).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let counters: i32 = row.get(5).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let rate_limit: i32 = row.get(6).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let conntrack: i32 = row.get(7).map_err(|e| PaciNetError::Internal(e.to_string()))?;

    let deployed_at = DateTime::parse_from_rfc3339(&deployed_at_str)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?
        .with_timezone(&Utc);

    Ok(PolicyVersion {
        version: version as u64,
        node_id,
        rules_yaml,
        policy_hash,
        deployed_at,
        counters_enabled: counters != 0,
        rate_limit_enabled: rate_limit != 0,
        conntrack_enabled: conntrack != 0,
    })
}

fn row_to_deployment(row: &rusqlite::Row) -> Result<DeploymentRecord, PaciNetError> {
    let id: String = row.get(0).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let node_id: String = row.get(1).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let policy_version: i64 = row.get(2).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let policy_hash: String = row.get(3).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let deployed_at_str: String = row.get(4).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let result_str: String = row.get(5).map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let message: String = row.get(6).map_err(|e| PaciNetError::Internal(e.to_string()))?;

    let deployed_at = DateTime::parse_from_rfc3339(&deployed_at_str)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?
        .with_timezone(&Utc);
    let result: DeploymentResult = result_str
        .parse()
        .map_err(|e: String| PaciNetError::Internal(e))?;

    Ok(DeploymentRecord {
        id,
        node_id,
        policy_version: policy_version as u64,
        policy_hash,
        deployed_at,
        result,
        message,
    })
}

use rusqlite::OptionalExtension;
