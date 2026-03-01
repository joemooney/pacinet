use chrono::{DateTime, Utc};
use pacinet_core::error::PaciNetError;
use pacinet_core::fsm::{FsmDefinition, FsmInstance, FsmInstanceStatus, FsmKind};
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
        let labels_json = serde_json::to_string(&node.labels)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        let annotations_json = serde_json::to_string(&node.annotations)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO nodes (node_id, hostname, agent_address, labels, state, registered_at, last_heartbeat, pacgate_version, uptime_seconds, annotations)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
                &annotations_json,
            ],
        )
        .map_err(|e| PaciNetError::Internal(format!("Failed to insert node: {}", e)))?;
        Ok(node.node_id)
    }

    fn get_node(&self, node_id: &str) -> Result<Option<Node>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT node_id, hostname, agent_address, labels, state, registered_at, last_heartbeat, pacgate_version, uptime_seconds, annotations
                 FROM nodes WHERE node_id = ?1",
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let node = stmt
            .query_row(rusqlite::params![node_id], |row| Ok(row_to_node(row)))
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        match node {
            Some(Ok(n)) => Ok(Some(n)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    fn list_nodes(
        &self,
        label_filter: &HashMap<String, String>,
    ) -> Result<Vec<Node>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT node_id, hostname, agent_address, labels, state, registered_at, last_heartbeat, pacgate_version, uptime_seconds, annotations
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
            .execute(
                "DELETE FROM nodes WHERE node_id = ?1",
                rusqlite::params![node_id],
            )
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

    fn store_counters(
        &self,
        node_id: &str,
        counters: Vec<RuleCounter>,
    ) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let json =
            serde_json::to_string(&counters).map_err(|e| PaciNetError::Internal(e.to_string()))?;
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
                let counters: Vec<RuleCounter> =
                    serde_json::from_str(&j).map_err(|e| PaciNetError::Internal(e.to_string()))?;
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

    fn mark_stale_nodes(&self, threshold: chrono::Duration) -> Result<Vec<String>, PaciNetError> {
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
            .query_row("SELECT MIN(last_heartbeat) FROM nodes", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .flatten();

        let oldest_dt = oldest.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        Ok((total, by_state, oldest_dt))
    }

    // ---- FSM operations ----

    fn store_fsm_definition(&self, def: FsmDefinition) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(&def)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO fsm_definitions (name, kind, definition_json, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                &def.name,
                def.kind.to_string(),
                json,
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(|e| PaciNetError::Internal(format!("Failed to store FSM definition: {}", e)))?;
        Ok(())
    }

    fn get_fsm_definition(&self, name: &str) -> Result<Option<FsmDefinition>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT definition_json FROM fsm_definitions WHERE name = ?1",
                rusqlite::params![name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        match json {
            None => Ok(None),
            Some(j) => {
                let def: FsmDefinition = serde_json::from_str(&j)
                    .map_err(|e| PaciNetError::Internal(e.to_string()))?;
                Ok(Some(def))
            }
        }
    }

    fn list_fsm_definitions(
        &self,
        kind: Option<FsmKind>,
    ) -> Result<Vec<FsmDefinition>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match &kind {
            Some(k) => (
                "SELECT definition_json FROM fsm_definitions WHERE kind = ?1",
                vec![Box::new(k.to_string())],
            ),
            None => (
                "SELECT definition_json FROM fsm_definitions",
                vec![],
            ),
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let defs: Vec<FsmDefinition> = stmt
            .query_map(params_refs.as_slice(), |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|j| serde_json::from_str(&j).ok())
            .collect();

        Ok(defs)
    }

    fn delete_fsm_definition(&self, name: &str) -> Result<bool, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "DELETE FROM fsm_definitions WHERE name = ?1",
                rusqlite::params![name],
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        Ok(affected > 0)
    }

    fn store_fsm_instance(&self, instance: FsmInstance) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(&instance)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO fsm_instances (instance_id, definition_name, status, instance_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                &instance.instance_id,
                &instance.definition_name,
                instance.status.to_string(),
                json,
                instance.created_at.to_rfc3339(),
                instance.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| PaciNetError::Internal(format!("Failed to store FSM instance: {}", e)))?;
        Ok(())
    }

    fn get_fsm_instance(&self, id: &str) -> Result<Option<FsmInstance>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT instance_json FROM fsm_instances WHERE instance_id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        match json {
            None => Ok(None),
            Some(j) => {
                let instance: FsmInstance = serde_json::from_str(&j)
                    .map_err(|e| PaciNetError::Internal(e.to_string()))?;
                Ok(Some(instance))
            }
        }
    }

    fn update_fsm_instance(&self, instance: FsmInstance) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(&instance)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        let affected = conn
            .execute(
                "UPDATE fsm_instances SET status = ?1, instance_json = ?2, updated_at = ?3 WHERE instance_id = ?4",
                rusqlite::params![
                    instance.status.to_string(),
                    json,
                    instance.updated_at.to_rfc3339(),
                    &instance.instance_id,
                ],
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        if affected == 0 {
            return Err(PaciNetError::Fsm(
                pacinet_core::fsm::FsmError::InstanceNotFound(instance.instance_id),
            ));
        }
        Ok(())
    }

    fn list_fsm_instances(
        &self,
        def_name: Option<&str>,
        status: Option<FsmInstanceStatus>,
    ) -> Result<Vec<FsmInstance>, PaciNetError> {
        let conn = self.conn.lock().unwrap();

        let mut conditions = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(name) = def_name {
            conditions.push(format!("definition_name = ?{}", idx));
            params.push(Box::new(name.to_string()));
            idx += 1;
        }
        if let Some(ref s) = status {
            conditions.push(format!("status = ?{}", idx));
            params.push(Box::new(s.to_string()));
        }

        let sql = if conditions.is_empty() {
            "SELECT instance_json FROM fsm_instances ORDER BY created_at DESC".to_string()
        } else {
            format!(
                "SELECT instance_json FROM fsm_instances WHERE {} ORDER BY created_at DESC",
                conditions.join(" AND ")
            )
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let instances: Vec<FsmInstance> = stmt
            .query_map(params_refs.as_slice(), |row| {
                let json: String = row.get(0)?;
                Ok(json)
            })
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|j| serde_json::from_str(&j).ok())
            .collect();

        Ok(instances)
    }

    // ---- Event log operations ----

    fn store_event(&self, event: PersistentEvent) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (id, event_type, source, payload, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                &event.id,
                &event.event_type,
                &event.source,
                &event.payload,
                event.timestamp.to_rfc3339(),
            ],
        )
        .map_err(|e| PaciNetError::Internal(format!("Failed to store event: {}", e)))?;
        Ok(())
    }

    fn query_events(
        &self,
        event_type: Option<&str>,
        source: Option<&str>,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<PersistentEvent>, PaciNetError> {
        let conn = self.conn.lock().unwrap();

        let mut conditions = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(et) = event_type {
            conditions.push(format!("event_type = ?{}", idx));
            params.push(Box::new(et.to_string()));
            idx += 1;
        }
        if let Some(src) = source {
            conditions.push(format!("source = ?{}", idx));
            params.push(Box::new(src.to_string()));
            idx += 1;
        }
        if let Some(s) = since {
            conditions.push(format!("timestamp >= ?{}", idx));
            params.push(Box::new(s.to_rfc3339()));
            idx += 1;
        }
        if let Some(u) = until {
            conditions.push(format!("timestamp <= ?{}", idx));
            params.push(Box::new(u.to_rfc3339()));
            idx += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, event_type, source, payload, timestamp FROM events {} ORDER BY timestamp DESC LIMIT ?{}",
            where_clause, idx
        );
        params.push(Box::new(limit as i64));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let events: Vec<PersistentEvent> = stmt
            .query_map(params_refs.as_slice(), |row| {
                let id: String = row.get(0)?;
                let event_type: String = row.get(1)?;
                let source: String = row.get(2)?;
                let payload: String = row.get(3)?;
                let ts_str: String = row.get(4)?;
                Ok((id, event_type, source, payload, ts_str))
            })
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|(id, event_type, source, payload, ts_str)| {
                let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                    .ok()?
                    .with_timezone(&Utc);
                Some(PersistentEvent {
                    id,
                    event_type,
                    source,
                    payload,
                    timestamp,
                })
            })
            .collect();

        Ok(events)
    }

    fn prune_events(&self, older_than: DateTime<Utc>) -> Result<u64, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "DELETE FROM events WHERE timestamp < ?1",
                rusqlite::params![older_than.to_rfc3339()],
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        Ok(affected as u64)
    }

    fn count_events(&self) -> Result<u64, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        Ok(count as u64)
    }

    // ---- Leader lease operations ----

    fn try_acquire_lease(
        &self,
        controller_id: &str,
        duration_secs: u64,
    ) -> Result<bool, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(duration_secs as i64);

        // Use BEGIN IMMEDIATE for atomic read-modify-write
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let result = (|| -> Result<bool, PaciNetError> {
            let existing: Option<(String, String)> = conn
                .query_row(
                    "SELECT controller_id, lease_expires_at FROM leader_lease WHERE id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|e| PaciNetError::Internal(e.to_string()))?;

            match existing {
                None => {
                    // No lease exists — acquire it
                    conn.execute(
                        "INSERT INTO leader_lease (id, controller_id, lease_expires_at, acquired_at) VALUES (1, ?1, ?2, ?3)",
                        rusqlite::params![controller_id, expires_at.to_rfc3339(), now.to_rfc3339()],
                    )
                    .map_err(|e| PaciNetError::Internal(e.to_string()))?;
                    Ok(true)
                }
                Some((existing_id, expires_str)) => {
                    if existing_id == controller_id {
                        // We hold the lease — renew it
                        conn.execute(
                            "UPDATE leader_lease SET lease_expires_at = ?1, acquired_at = ?2 WHERE id = 1",
                            rusqlite::params![expires_at.to_rfc3339(), now.to_rfc3339()],
                        )
                        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
                        Ok(true)
                    } else {
                        // Check if lease expired
                        let lease_expires = DateTime::parse_from_rfc3339(&expires_str)
                            .map_err(|e| PaciNetError::Internal(e.to_string()))?
                            .with_timezone(&Utc);
                        if now > lease_expires {
                            // Expired — take over
                            conn.execute(
                                "UPDATE leader_lease SET controller_id = ?1, lease_expires_at = ?2, acquired_at = ?3 WHERE id = 1",
                                rusqlite::params![controller_id, expires_at.to_rfc3339(), now.to_rfc3339()],
                            )
                            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
                            Ok(true)
                        } else {
                            Ok(false)
                        }
                    }
                }
            }
        })();

        match &result {
            Ok(_) => {
                conn.execute_batch("COMMIT")
                    .map_err(|e| PaciNetError::Internal(e.to_string()))?;
            }
            Err(_) => {
                let _ = conn.execute_batch("ROLLBACK");
            }
        }

        result
    }

    fn get_leader(&self) -> Result<Option<(String, DateTime<Utc>)>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let result = conn
            .query_row(
                "SELECT controller_id, lease_expires_at FROM leader_lease WHERE id = 1",
                [],
                |row| {
                    let id: String = row.get(0)?;
                    let expires: String = row.get(1)?;
                    Ok((id, expires))
                },
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        match result {
            None => Ok(None),
            Some((id, expires_str)) => {
                let expires = DateTime::parse_from_rfc3339(&expires_str)
                    .map_err(|e| PaciNetError::Internal(e.to_string()))?
                    .with_timezone(&Utc);
                Ok(Some((id, expires)))
            }
        }
    }

    // ---- Node annotations ----

    fn update_annotations(
        &self,
        node_id: &str,
        set: HashMap<String, String>,
        remove: &[String],
    ) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let annotations_json: String = conn
            .query_row(
                "SELECT annotations FROM nodes WHERE node_id = ?1",
                rusqlite::params![node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .ok_or_else(|| PaciNetError::NodeNotFound(node_id.to_string()))?;

        let mut annotations: HashMap<String, String> =
            serde_json::from_str(&annotations_json).unwrap_or_default();
        for key in remove {
            annotations.remove(key);
        }
        annotations.extend(set);

        let new_json = serde_json::to_string(&annotations)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        conn.execute(
            "UPDATE nodes SET annotations = ?1 WHERE node_id = ?2",
            rusqlite::params![new_json, node_id],
        )
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        Ok(())
    }

    // ---- Audit log ----

    fn store_audit(&self, entry: AuditEntry) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO audit_log (id, timestamp, actor, action, resource_type, resource_id, details) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                &entry.id,
                entry.timestamp.to_rfc3339(),
                &entry.actor,
                &entry.action,
                &entry.resource_type,
                &entry.resource_id,
                &entry.details,
            ],
        )
        .map_err(|e| PaciNetError::Internal(format!("Failed to store audit entry: {}", e)))?;
        Ok(())
    }

    fn query_audit(
        &self,
        action: Option<&str>,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        since: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<AuditEntry>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let mut conditions = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(a) = action {
            conditions.push(format!("action = ?{}", idx));
            params.push(Box::new(a.to_string()));
            idx += 1;
        }
        if let Some(rt) = resource_type {
            conditions.push(format!("resource_type = ?{}", idx));
            params.push(Box::new(rt.to_string()));
            idx += 1;
        }
        if let Some(ri) = resource_id {
            conditions.push(format!("resource_id = ?{}", idx));
            params.push(Box::new(ri.to_string()));
            idx += 1;
        }
        if let Some(s) = since {
            conditions.push(format!("timestamp >= ?{}", idx));
            params.push(Box::new(s.to_rfc3339()));
            idx += 1;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, timestamp, actor, action, resource_type, resource_id, details FROM audit_log {} ORDER BY timestamp DESC LIMIT ?{}",
            where_clause, idx
        );
        params.push(Box::new(limit as i64));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let entries: Vec<AuditEntry> = stmt
            .query_map(params_refs.as_slice(), |row| {
                let id: String = row.get(0)?;
                let ts_str: String = row.get(1)?;
                let actor: String = row.get(2)?;
                let action: String = row.get(3)?;
                let resource_type: String = row.get(4)?;
                let resource_id: String = row.get(5)?;
                let details: String = row.get(6)?;
                Ok((id, ts_str, actor, action, resource_type, resource_id, details))
            })
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|(id, ts_str, actor, action, resource_type, resource_id, details)| {
                let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                    .ok()?
                    .with_timezone(&Utc);
                Some(AuditEntry {
                    id,
                    timestamp,
                    actor,
                    action,
                    resource_type,
                    resource_id,
                    details,
                })
            })
            .collect();

        Ok(entries)
    }

    // ---- Policy templates ----

    fn store_template(&self, template: PolicyTemplate) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let tags_json = serde_json::to_string(&template.tags)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO policy_templates (name, description, rules_yaml, tags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                &template.name,
                &template.description,
                &template.rules_yaml,
                &tags_json,
                template.created_at.to_rfc3339(),
                template.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| PaciNetError::Internal(format!("Failed to store template: {}", e)))?;
        Ok(())
    }

    fn get_template(&self, name: &str) -> Result<Option<PolicyTemplate>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let result = conn
            .query_row(
                "SELECT name, description, rules_yaml, tags, created_at, updated_at FROM policy_templates WHERE name = ?1",
                rusqlite::params![name],
                |row| {
                    let name: String = row.get(0)?;
                    let description: String = row.get(1)?;
                    let rules_yaml: String = row.get(2)?;
                    let tags_json: String = row.get(3)?;
                    let created_at_str: String = row.get(4)?;
                    let updated_at_str: String = row.get(5)?;
                    Ok((name, description, rules_yaml, tags_json, created_at_str, updated_at_str))
                },
            )
            .optional()
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        match result {
            None => Ok(None),
            Some((name, description, rules_yaml, tags_json, created_at_str, updated_at_str)) => {
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| PaciNetError::Internal(e.to_string()))?
                    .with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                    .map_err(|e| PaciNetError::Internal(e.to_string()))?
                    .with_timezone(&Utc);
                Ok(Some(PolicyTemplate {
                    name,
                    description,
                    rules_yaml,
                    tags,
                    created_at,
                    updated_at,
                }))
            }
        }
    }

    fn list_templates(&self, tag: Option<&str>) -> Result<Vec<PolicyTemplate>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT name, description, rules_yaml, tags, created_at, updated_at FROM policy_templates ORDER BY name")
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let templates: Vec<PolicyTemplate> = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                let description: String = row.get(1)?;
                let rules_yaml: String = row.get(2)?;
                let tags_json: String = row.get(3)?;
                let created_at_str: String = row.get(4)?;
                let updated_at_str: String = row.get(5)?;
                Ok((name, description, rules_yaml, tags_json, created_at_str, updated_at_str))
            })
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|(name, description, rules_yaml, tags_json, created_at_str, updated_at_str)| {
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                let created_at = DateTime::parse_from_rfc3339(&created_at_str).ok()?.with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&updated_at_str).ok()?.with_timezone(&Utc);
                Some(PolicyTemplate { name, description, rules_yaml, tags, created_at, updated_at })
            })
            .filter(|t| tag.is_none_or(|tg| t.tags.iter().any(|tt| tt == tg)))
            .collect();

        Ok(templates)
    }

    fn delete_template(&self, name: &str) -> Result<bool, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "DELETE FROM policy_templates WHERE name = ?1",
                rusqlite::params![name],
            )
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;
        Ok(affected > 0)
    }

    // ---- Webhook delivery history ----

    fn store_webhook_delivery(&self, delivery: WebhookDelivery) -> Result<(), PaciNetError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO webhook_deliveries (id, instance_id, url, method, status_code, success, duration_ms, error, attempt, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                &delivery.id,
                &delivery.instance_id,
                &delivery.url,
                &delivery.method,
                delivery.status_code.map(|c| c as i64),
                delivery.success as i32,
                delivery.duration_ms as i64,
                &delivery.error,
                delivery.attempt as i64,
                delivery.timestamp.to_rfc3339(),
            ],
        )
        .map_err(|e| PaciNetError::Internal(format!("Failed to store webhook delivery: {}", e)))?;
        Ok(())
    }

    fn query_webhook_deliveries(
        &self,
        instance_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<WebhookDelivery>, PaciNetError> {
        let conn = self.conn.lock().unwrap();
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match instance_id {
            Some(id) => (
                "SELECT id, instance_id, url, method, status_code, success, duration_ms, error, attempt, timestamp FROM webhook_deliveries WHERE instance_id = ?1 ORDER BY timestamp DESC LIMIT ?2".to_string(),
                vec![Box::new(id.to_string()) as Box<dyn rusqlite::types::ToSql>, Box::new(limit as i64)],
            ),
            None => (
                "SELECT id, instance_id, url, method, status_code, success, duration_ms, error, attempt, timestamp FROM webhook_deliveries ORDER BY timestamp DESC LIMIT ?1".to_string(),
                vec![Box::new(limit as i64) as Box<dyn rusqlite::types::ToSql>],
            ),
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| PaciNetError::Internal(e.to_string()))?;

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let deliveries: Vec<WebhookDelivery> = stmt
            .query_map(params_refs.as_slice(), |row| {
                let id: String = row.get(0)?;
                let instance_id: String = row.get(1)?;
                let url: String = row.get(2)?;
                let method: String = row.get(3)?;
                let status_code: Option<i64> = row.get(4)?;
                let success: i32 = row.get(5)?;
                let duration_ms: i64 = row.get(6)?;
                let error: Option<String> = row.get(7)?;
                let attempt: i64 = row.get(8)?;
                let ts_str: String = row.get(9)?;
                Ok((id, instance_id, url, method, status_code, success, duration_ms, error, attempt, ts_str))
            })
            .map_err(|e| PaciNetError::Internal(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|(id, instance_id, url, method, status_code, success, duration_ms, error, attempt, ts_str)| {
                let timestamp = DateTime::parse_from_rfc3339(&ts_str).ok()?.with_timezone(&Utc);
                Some(WebhookDelivery {
                    id,
                    instance_id,
                    url,
                    method,
                    status_code: status_code.map(|c| c as u16),
                    success: success != 0,
                    duration_ms: duration_ms as u64,
                    error,
                    attempt: attempt as u32,
                    timestamp,
                })
            })
            .collect();

        Ok(deliveries)
    }
}

// Helper functions to convert rusqlite rows to domain types

fn row_to_node(row: &rusqlite::Row) -> Result<Node, PaciNetError> {
    let node_id: String = row
        .get(0)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let hostname: String = row
        .get(1)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let agent_address: String = row
        .get(2)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let labels_json: String = row
        .get(3)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let state_str: String = row
        .get(4)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let registered_at_str: String = row
        .get(5)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let last_heartbeat_str: String = row
        .get(6)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let pacgate_version: String = row
        .get(7)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let uptime_seconds: i64 = row
        .get(8)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;

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

    let annotations_json: String = row.get(9).unwrap_or_else(|_| "{}".to_string());
    let annotations: HashMap<String, String> =
        serde_json::from_str(&annotations_json).unwrap_or_default();

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
        annotations,
    })
}

fn row_to_policy(row: &rusqlite::Row) -> Result<Policy, PaciNetError> {
    let node_id: String = row
        .get(0)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let rules_yaml: String = row
        .get(1)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let policy_hash: String = row
        .get(2)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let deployed_at_str: String = row
        .get(3)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let counters: i32 = row
        .get(4)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let rate_limit: i32 = row
        .get(5)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let conntrack: i32 = row
        .get(6)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;

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
    let version: i64 = row
        .get(0)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let node_id: String = row
        .get(1)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let rules_yaml: String = row
        .get(2)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let policy_hash: String = row
        .get(3)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let deployed_at_str: String = row
        .get(4)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let counters: i32 = row
        .get(5)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let rate_limit: i32 = row
        .get(6)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let conntrack: i32 = row
        .get(7)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;

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
    let id: String = row
        .get(0)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let node_id: String = row
        .get(1)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let policy_version: i64 = row
        .get(2)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let policy_hash: String = row
        .get(3)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let deployed_at_str: String = row
        .get(4)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let result_str: String = row
        .get(5)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;
    let message: String = row
        .get(6)
        .map_err(|e| PaciNetError::Internal(e.to_string()))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(hostname: &str, labels: Vec<(&str, &str)>) -> Node {
        let label_map = labels
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Node::new(
            hostname.to_string(),
            format!("127.0.0.1:5005{}", hostname.len()),
            label_map,
            "0.1.0".to_string(),
        )
    }

    #[test]
    fn test_register_and_get() {
        let storage = SqliteStorage::in_memory().unwrap();
        let node = make_node("test-host", vec![("env", "dev")]);
        let node_id = storage.register_node(node).unwrap();

        let retrieved = storage.get_node(&node_id).unwrap().unwrap();
        assert_eq!(retrieved.hostname, "test-host");
        assert_eq!(retrieved.labels.get("env").unwrap(), "dev");
    }

    #[test]
    fn test_remove_cleans_up() {
        let storage = SqliteStorage::in_memory().unwrap();
        let node = make_node("remove-me", vec![]);
        let node_id = storage.register_node(node).unwrap();

        storage
            .store_policy(Policy {
                node_id: node_id.clone(),
                rules_yaml: "rules: []".to_string(),
                policy_hash: "abc123".to_string(),
                deployed_at: Utc::now(),
                counters_enabled: false,
                rate_limit_enabled: false,
                conntrack_enabled: false,
            })
            .unwrap();
        storage
            .store_counters(
                &node_id,
                vec![RuleCounter {
                    rule_name: "rule1".to_string(),
                    match_count: 10,
                    byte_count: 100,
                }],
            )
            .unwrap();

        assert!(storage.get_policy(&node_id).unwrap().is_some());
        assert!(storage.get_counters(&node_id).unwrap().is_some());

        assert!(storage.remove_node(&node_id).unwrap());
        assert!(storage.get_node(&node_id).unwrap().is_none());
        assert!(storage.get_policy(&node_id).unwrap().is_none());
        assert!(storage.get_counters(&node_id).unwrap().is_none());
    }

    #[test]
    fn test_label_filtering() {
        let storage = SqliteStorage::in_memory().unwrap();
        storage
            .register_node(make_node("prod-1", vec![("env", "prod"), ("region", "us")]))
            .unwrap();
        storage
            .register_node(make_node("dev-1", vec![("env", "dev"), ("region", "us")]))
            .unwrap();
        storage
            .register_node(make_node("prod-2", vec![("env", "prod"), ("region", "eu")]))
            .unwrap();

        let filter: HashMap<String, String> = [("env".to_string(), "prod".to_string())].into();
        assert_eq!(storage.list_nodes(&filter).unwrap().len(), 2);

        let filter: HashMap<String, String> = [("region".to_string(), "us".to_string())].into();
        assert_eq!(storage.list_nodes(&filter).unwrap().len(), 2);

        let filter: HashMap<String, String> = [
            ("env".to_string(), "prod".to_string()),
            ("region".to_string(), "eu".to_string()),
        ]
        .into();
        let nodes = storage.list_nodes(&filter).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].hostname, "prod-2");

        assert_eq!(storage.list_nodes(&HashMap::new()).unwrap().len(), 3);
    }

    #[test]
    fn test_state_transitions() {
        let storage = SqliteStorage::in_memory().unwrap();
        let node = make_node("state-test", vec![]);
        let node_id = storage.register_node(node).unwrap();

        // Registered -> Online: valid
        assert!(storage
            .update_node_state(&node_id, NodeState::Online)
            .unwrap());
        let node = storage.get_node(&node_id).unwrap().unwrap();
        assert_eq!(node.state, NodeState::Online);

        // Online -> Deploying: valid
        assert!(storage
            .update_node_state(&node_id, NodeState::Deploying)
            .unwrap());

        // Deploying -> Active: valid
        assert!(storage
            .update_node_state(&node_id, NodeState::Active)
            .unwrap());

        // Active -> Deploying: valid (redeploy)
        assert!(storage
            .update_node_state(&node_id, NodeState::Deploying)
            .unwrap());

        // Deploying -> Error: valid
        assert!(storage
            .update_node_state(&node_id, NodeState::Error)
            .unwrap());

        // Error -> Online: valid
        assert!(storage
            .update_node_state(&node_id, NodeState::Online)
            .unwrap());

        // Non-existent node
        assert!(!storage
            .update_node_state("nonexistent", NodeState::Online)
            .unwrap());
    }

    #[test]
    fn test_invalid_state_transition() {
        let storage = SqliteStorage::in_memory().unwrap();
        let node = make_node("invalid-test", vec![]);
        let node_id = storage.register_node(node).unwrap();

        // Registered -> Active: invalid
        let result = storage.update_node_state(&node_id, NodeState::Active);
        assert!(result.is_err());
        match result.unwrap_err() {
            PaciNetError::InvalidStateTransition { from, to } => {
                assert_eq!(from, "registered");
                assert_eq!(to, "active");
            }
            e => panic!("Expected InvalidStateTransition, got: {:?}", e),
        }
    }

    #[test]
    fn test_concurrent_deploy_protection() {
        let storage = SqliteStorage::in_memory().unwrap();
        let node = make_node("deploy-test", vec![]);
        let node_id = storage.register_node(node).unwrap();

        // First begin_deploy succeeds
        storage.begin_deploy(&node_id).unwrap();

        // Second begin_deploy fails
        let result = storage.begin_deploy(&node_id);
        assert!(result.is_err());
        match result.unwrap_err() {
            PaciNetError::ConcurrentDeploy(id) => assert_eq!(id, node_id),
            e => panic!("Expected ConcurrentDeploy, got: {:?}", e),
        }

        // After end_deploy, begin_deploy works again
        storage.end_deploy(&node_id);
        storage.begin_deploy(&node_id).unwrap();
    }

    #[test]
    fn test_policy_versioning() {
        let storage = SqliteStorage::in_memory().unwrap();
        let node = make_node("version-test", vec![]);
        let node_id = storage.register_node(node).unwrap();

        // Store 3 policies
        for i in 1..=3 {
            let v = storage
                .store_policy(Policy {
                    node_id: node_id.clone(),
                    rules_yaml: format!("rules: v{}", i),
                    policy_hash: format!("hash{}", i),
                    deployed_at: Utc::now(),
                    counters_enabled: false,
                    rate_limit_enabled: false,
                    conntrack_enabled: false,
                })
                .unwrap();
            assert_eq!(v, i);
        }

        // Current policy is the last one
        let current = storage.get_policy(&node_id).unwrap().unwrap();
        assert_eq!(current.policy_hash, "hash3");

        // History returns newest first
        let history = storage.get_policy_history(&node_id, 10).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].version, 3);
        assert_eq!(history[2].version, 1);

        // Limit works
        let limited = storage.get_policy_history(&node_id, 2).unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn test_deployment_audit() {
        let storage = SqliteStorage::in_memory().unwrap();
        let node = make_node("audit-test", vec![]);
        let node_id = storage.register_node(node).unwrap();

        storage
            .record_deployment(DeploymentRecord {
                id: "d1".to_string(),
                node_id: node_id.clone(),
                policy_version: 1,
                policy_hash: "hash1".to_string(),
                deployed_at: Utc::now(),
                result: DeploymentResult::Success,
                message: "ok".to_string(),
            })
            .unwrap();
        storage
            .record_deployment(DeploymentRecord {
                id: "d2".to_string(),
                node_id: node_id.clone(),
                policy_version: 2,
                policy_hash: "hash2".to_string(),
                deployed_at: Utc::now(),
                result: DeploymentResult::AgentFailure,
                message: "compile failed".to_string(),
            })
            .unwrap();

        let records = storage.get_deployments(&node_id, 10).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, "d2"); // newest first
        assert_eq!(records[1].id, "d1");
    }

    #[test]
    fn test_stale_node_detection() {
        let storage = SqliteStorage::in_memory().unwrap();
        let mut node = make_node("stale-test", vec![]);
        // Set heartbeat to 5 minutes ago
        node.last_heartbeat = Utc::now() - chrono::Duration::minutes(5);
        node.state = NodeState::Online;
        let node_id = storage.register_node(node).unwrap();

        // Threshold of 2 minutes — node should be marked stale
        let stale = storage
            .mark_stale_nodes(chrono::Duration::minutes(2))
            .unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0], node_id);

        let node = storage.get_node(&node_id).unwrap().unwrap();
        assert_eq!(node.state, NodeState::Offline);
    }
}
