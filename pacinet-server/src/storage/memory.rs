use chrono::{DateTime, Utc};
use pacinet_core::error::PaciNetError;
use pacinet_core::fsm::{FsmDefinition, FsmInstance, FsmInstanceStatus, FsmKind};
use pacinet_core::model::*;
use pacinet_core::Storage;
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

const MAX_MEMORY_EVENTS: usize = 10_000;
const MAX_MEMORY_AUDIT: usize = 10_000;

/// In-memory storage backend (replaces NodeRegistry from Phase 2).
pub struct MemoryStorage {
    nodes: RwLock<HashMap<String, Node>>,
    policies: RwLock<HashMap<String, Policy>>,
    counters: RwLock<HashMap<String, Vec<RuleCounter>>>,
    flow_counters: RwLock<HashMap<String, Vec<FlowCounter>>>,
    policy_versions: RwLock<HashMap<String, Vec<PolicyVersion>>>,
    deployments: RwLock<Vec<DeploymentRecord>>,
    deploying: RwLock<HashSet<String>>,
    fsm_definitions: RwLock<HashMap<String, FsmDefinition>>,
    fsm_instances: RwLock<HashMap<String, FsmInstance>>,
    events: RwLock<Vec<PersistentEvent>>,
    audit_log: RwLock<Vec<AuditEntry>>,
    templates: RwLock<HashMap<String, PolicyTemplate>>,
    webhook_deliveries: RwLock<Vec<WebhookDelivery>>,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            policies: RwLock::new(HashMap::new()),
            counters: RwLock::new(HashMap::new()),
            flow_counters: RwLock::new(HashMap::new()),
            policy_versions: RwLock::new(HashMap::new()),
            deployments: RwLock::new(Vec::new()),
            deploying: RwLock::new(HashSet::new()),
            fsm_definitions: RwLock::new(HashMap::new()),
            fsm_instances: RwLock::new(HashMap::new()),
            events: RwLock::new(Vec::new()),
            audit_log: RwLock::new(Vec::new()),
            templates: RwLock::new(HashMap::new()),
            webhook_deliveries: RwLock::new(Vec::new()),
        }
    }
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Storage for MemoryStorage {
    fn register_node(&self, node: Node) -> Result<String, PaciNetError> {
        let node_id = node.node_id.clone();
        self.nodes.write().unwrap().insert(node_id.clone(), node);
        Ok(node_id)
    }

    fn get_node(&self, node_id: &str) -> Result<Option<Node>, PaciNetError> {
        Ok(self.nodes.read().unwrap().get(node_id).cloned())
    }

    fn list_nodes(
        &self,
        label_filter: &HashMap<String, String>,
    ) -> Result<Vec<Node>, PaciNetError> {
        let nodes = self.nodes.read().unwrap();
        Ok(nodes
            .values()
            .filter(|node| {
                label_filter
                    .iter()
                    .all(|(k, v)| node.labels.get(k) == Some(v))
            })
            .cloned()
            .collect())
    }

    fn remove_node(&self, node_id: &str) -> Result<bool, PaciNetError> {
        self.policies.write().unwrap().remove(node_id);
        self.counters.write().unwrap().remove(node_id);
        self.flow_counters.write().unwrap().remove(node_id);
        self.policy_versions.write().unwrap().remove(node_id);
        self.deploying.write().unwrap().remove(node_id);
        Ok(self.nodes.write().unwrap().remove(node_id).is_some())
    }

    fn update_heartbeat(
        &self,
        node_id: &str,
        state: NodeState,
        uptime: u64,
    ) -> Result<bool, PaciNetError> {
        let mut nodes = self.nodes.write().unwrap();
        if let Some(node) = nodes.get_mut(node_id) {
            node.last_heartbeat = Utc::now();
            node.state = state;
            node.uptime_seconds = uptime;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn update_node_state(&self, node_id: &str, state: NodeState) -> Result<bool, PaciNetError> {
        let mut nodes = self.nodes.write().unwrap();
        if let Some(node) = nodes.get_mut(node_id) {
            if !node.state.can_transition_to(&state) {
                return Err(PaciNetError::InvalidStateTransition {
                    from: node.state.to_string(),
                    to: state.to_string(),
                });
            }
            node.state = state;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn store_counters(
        &self,
        node_id: &str,
        counters: Vec<RuleCounter>,
    ) -> Result<(), PaciNetError> {
        self.counters
            .write()
            .unwrap()
            .insert(node_id.to_string(), counters);
        Ok(())
    }

    fn get_counters(&self, node_id: &str) -> Result<Option<Vec<RuleCounter>>, PaciNetError> {
        Ok(self.counters.read().unwrap().get(node_id).cloned())
    }

    fn store_flow_counters(
        &self,
        node_id: &str,
        counters: Vec<FlowCounter>,
    ) -> Result<(), PaciNetError> {
        self.flow_counters
            .write()
            .unwrap()
            .insert(node_id.to_string(), counters);
        Ok(())
    }

    fn get_flow_counters(&self, node_id: &str) -> Result<Option<Vec<FlowCounter>>, PaciNetError> {
        Ok(self.flow_counters.read().unwrap().get(node_id).cloned())
    }

    fn store_policy(&self, policy: Policy) -> Result<u64, PaciNetError> {
        let node_id = policy.node_id.clone();
        let mut versions = self.policy_versions.write().unwrap();
        let history = versions.entry(node_id.clone()).or_default();
        let version = (history.len() as u64) + 1;
        history.push(PolicyVersion {
            version,
            node_id: node_id.clone(),
            rules_yaml: policy.rules_yaml.clone(),
            policy_hash: policy.policy_hash.clone(),
            deployed_at: policy.deployed_at,
            counters_enabled: policy.counters_enabled,
            rate_limit_enabled: policy.rate_limit_enabled,
            conntrack_enabled: policy.conntrack_enabled,
            axi_enabled: policy.axi_enabled,
            ports: policy.ports,
            target: policy.target.clone(),
            dynamic: policy.dynamic,
            dynamic_entries: policy.dynamic_entries,
            width: policy.width,
            ptp: policy.ptp,
            rss: policy.rss,
            rss_queues: policy.rss_queues,
            int: policy.int,
            int_switch_id: policy.int_switch_id,
        });
        self.policies.write().unwrap().insert(node_id, policy);
        Ok(version)
    }

    fn get_policy(&self, node_id: &str) -> Result<Option<Policy>, PaciNetError> {
        Ok(self.policies.read().unwrap().get(node_id).cloned())
    }

    fn get_policy_history(
        &self,
        node_id: &str,
        limit: u32,
    ) -> Result<Vec<PolicyVersion>, PaciNetError> {
        let versions = self.policy_versions.read().unwrap();
        Ok(versions
            .get(node_id)
            .map(|v| v.iter().rev().take(limit as usize).cloned().collect())
            .unwrap_or_default())
    }

    fn get_policies_for_nodes(
        &self,
        node_ids: &[String],
    ) -> Result<HashMap<String, Policy>, PaciNetError> {
        let policies = self.policies.read().unwrap();
        Ok(node_ids
            .iter()
            .filter_map(|id| policies.get(id).map(|p| (id.clone(), p.clone())))
            .collect())
    }

    fn record_deployment(&self, record: DeploymentRecord) -> Result<(), PaciNetError> {
        self.deployments.write().unwrap().push(record);
        Ok(())
    }

    fn get_deployments(
        &self,
        node_id: &str,
        limit: u32,
    ) -> Result<Vec<DeploymentRecord>, PaciNetError> {
        let deployments = self.deployments.read().unwrap();
        Ok(deployments
            .iter()
            .rev()
            .filter(|d| d.node_id == node_id)
            .take(limit as usize)
            .cloned()
            .collect())
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
        let now = Utc::now();
        let mut nodes = self.nodes.write().unwrap();
        let mut stale = Vec::new();
        for node in nodes.values_mut() {
            if node.state != NodeState::Offline
                && node.state != NodeState::Registered
                && (now - node.last_heartbeat) > threshold
            {
                node.state = NodeState::Offline;
                stale.push(node.node_id.clone());
            }
        }
        Ok(stale)
    }

    fn status_summary(
        &self,
    ) -> Result<(usize, HashMap<String, usize>, Option<DateTime<Utc>>), PaciNetError> {
        let nodes = self.nodes.read().unwrap();
        let total = nodes.len();
        let mut by_state: HashMap<String, usize> = HashMap::new();
        let mut oldest: Option<DateTime<Utc>> = None;
        for node in nodes.values() {
            *by_state.entry(node.state.to_string()).or_insert(0) += 1;
            match oldest {
                None => oldest = Some(node.last_heartbeat),
                Some(prev) if node.last_heartbeat < prev => oldest = Some(node.last_heartbeat),
                _ => {}
            }
        }
        Ok((total, by_state, oldest))
    }

    // ---- FSM operations ----

    fn store_fsm_definition(&self, def: FsmDefinition) -> Result<(), PaciNetError> {
        self.fsm_definitions
            .write()
            .unwrap()
            .insert(def.name.clone(), def);
        Ok(())
    }

    fn get_fsm_definition(&self, name: &str) -> Result<Option<FsmDefinition>, PaciNetError> {
        Ok(self.fsm_definitions.read().unwrap().get(name).cloned())
    }

    fn list_fsm_definitions(
        &self,
        kind: Option<FsmKind>,
    ) -> Result<Vec<FsmDefinition>, PaciNetError> {
        let defs = self.fsm_definitions.read().unwrap();
        Ok(defs
            .values()
            .filter(|d| kind.as_ref().is_none_or(|k| &d.kind == k))
            .cloned()
            .collect())
    }

    fn delete_fsm_definition(&self, name: &str) -> Result<bool, PaciNetError> {
        Ok(self.fsm_definitions.write().unwrap().remove(name).is_some())
    }

    fn store_fsm_instance(&self, instance: FsmInstance) -> Result<(), PaciNetError> {
        self.fsm_instances
            .write()
            .unwrap()
            .insert(instance.instance_id.clone(), instance);
        Ok(())
    }

    fn get_fsm_instance(&self, id: &str) -> Result<Option<FsmInstance>, PaciNetError> {
        Ok(self.fsm_instances.read().unwrap().get(id).cloned())
    }

    fn update_fsm_instance(&self, instance: FsmInstance) -> Result<(), PaciNetError> {
        let mut instances = self.fsm_instances.write().unwrap();
        if instances.contains_key(&instance.instance_id) {
            instances.insert(instance.instance_id.clone(), instance);
            Ok(())
        } else {
            Err(PaciNetError::Fsm(
                pacinet_core::fsm::FsmError::InstanceNotFound(instance.instance_id),
            ))
        }
    }

    fn list_fsm_instances(
        &self,
        def_name: Option<&str>,
        status: Option<FsmInstanceStatus>,
    ) -> Result<Vec<FsmInstance>, PaciNetError> {
        let instances = self.fsm_instances.read().unwrap();
        Ok(instances
            .values()
            .filter(|i| def_name.is_none_or(|n| i.definition_name == n))
            .filter(|i| status.as_ref().is_none_or(|s| &i.status == s))
            .cloned()
            .collect())
    }

    // ---- Event log operations ----

    fn store_event(&self, event: PersistentEvent) -> Result<(), PaciNetError> {
        let mut events = self.events.write().unwrap();
        events.push(event);
        if events.len() > MAX_MEMORY_EVENTS {
            let drain_count = events.len() - MAX_MEMORY_EVENTS;
            events.drain(..drain_count);
        }
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
        let events = self.events.read().unwrap();
        let filtered: Vec<PersistentEvent> = events
            .iter()
            .rev()
            .filter(|e| event_type.is_none_or(|t| e.event_type == t))
            .filter(|e| source.is_none_or(|s| e.source == s))
            .filter(|e| since.is_none_or(|s| e.timestamp >= s))
            .filter(|e| until.is_none_or(|u| e.timestamp <= u))
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(filtered)
    }

    fn prune_events(&self, older_than: DateTime<Utc>) -> Result<u64, PaciNetError> {
        let mut events = self.events.write().unwrap();
        let before = events.len();
        events.retain(|e| e.timestamp >= older_than);
        Ok((before - events.len()) as u64)
    }

    fn count_events(&self) -> Result<u64, PaciNetError> {
        Ok(self.events.read().unwrap().len() as u64)
    }

    // ---- Node annotations ----

    fn update_annotations(
        &self,
        node_id: &str,
        set: HashMap<String, String>,
        remove: &[String],
    ) -> Result<(), PaciNetError> {
        let mut nodes = self.nodes.write().unwrap();
        let node = nodes
            .get_mut(node_id)
            .ok_or_else(|| PaciNetError::NodeNotFound(node_id.to_string()))?;
        for key in remove {
            node.annotations.remove(key);
        }
        node.annotations.extend(set);
        Ok(())
    }

    // ---- Audit log ----

    fn store_audit(&self, entry: AuditEntry) -> Result<(), PaciNetError> {
        let mut log = self.audit_log.write().unwrap();
        log.push(entry);
        if log.len() > MAX_MEMORY_AUDIT {
            let drain_count = log.len() - MAX_MEMORY_AUDIT;
            log.drain(..drain_count);
        }
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
        let log = self.audit_log.read().unwrap();
        Ok(log
            .iter()
            .rev()
            .filter(|e| action.is_none_or(|a| e.action == a))
            .filter(|e| resource_type.is_none_or(|rt| e.resource_type == rt))
            .filter(|e| resource_id.is_none_or(|ri| e.resource_id == ri))
            .filter(|e| since.is_none_or(|s| e.timestamp >= s))
            .take(limit as usize)
            .cloned()
            .collect())
    }

    // ---- Policy templates ----

    fn store_template(&self, template: PolicyTemplate) -> Result<(), PaciNetError> {
        self.templates
            .write()
            .unwrap()
            .insert(template.name.clone(), template);
        Ok(())
    }

    fn get_template(&self, name: &str) -> Result<Option<PolicyTemplate>, PaciNetError> {
        Ok(self.templates.read().unwrap().get(name).cloned())
    }

    fn list_templates(&self, tag: Option<&str>) -> Result<Vec<PolicyTemplate>, PaciNetError> {
        let templates = self.templates.read().unwrap();
        Ok(templates
            .values()
            .filter(|t| tag.is_none_or(|tg| t.tags.iter().any(|tt| tt == tg)))
            .cloned()
            .collect())
    }

    fn delete_template(&self, name: &str) -> Result<bool, PaciNetError> {
        Ok(self.templates.write().unwrap().remove(name).is_some())
    }

    // ---- Webhook delivery history ----

    fn store_webhook_delivery(&self, delivery: WebhookDelivery) -> Result<(), PaciNetError> {
        self.webhook_deliveries.write().unwrap().push(delivery);
        Ok(())
    }

    fn query_webhook_deliveries(
        &self,
        instance_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<WebhookDelivery>, PaciNetError> {
        let deliveries = self.webhook_deliveries.read().unwrap();
        Ok(deliveries
            .iter()
            .rev()
            .filter(|d| instance_id.is_none_or(|id| d.instance_id == id))
            .take(limit as usize)
            .cloned()
            .collect())
    }
}

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
        let storage = MemoryStorage::new();
        let node = make_node("test-host", vec![("env", "dev")]);
        let node_id = storage.register_node(node).unwrap();

        let retrieved = storage.get_node(&node_id).unwrap().unwrap();
        assert_eq!(retrieved.hostname, "test-host");
        assert_eq!(retrieved.labels.get("env").unwrap(), "dev");
    }

    #[test]
    fn test_remove_cleans_up() {
        let storage = MemoryStorage::new();
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
                axi_enabled: false,
                ports: 1,
                target: "standalone".to_string(),
                dynamic: false,
                dynamic_entries: 16,
                width: 8,
                ptp: false,
                rss: false,
                rss_queues: 4,
                int: false,
                int_switch_id: 0,
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
        let storage = MemoryStorage::new();
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
        let storage = MemoryStorage::new();
        let node = make_node("state-test", vec![]);
        let node_id = storage.register_node(node).unwrap();

        // Registered → Online: valid
        assert!(storage
            .update_node_state(&node_id, NodeState::Online)
            .unwrap());
        let node = storage.get_node(&node_id).unwrap().unwrap();
        assert_eq!(node.state, NodeState::Online);

        // Online → Deploying: valid
        assert!(storage
            .update_node_state(&node_id, NodeState::Deploying)
            .unwrap());

        // Deploying → Active: valid
        assert!(storage
            .update_node_state(&node_id, NodeState::Active)
            .unwrap());

        // Active → Deploying: valid (redeploy)
        assert!(storage
            .update_node_state(&node_id, NodeState::Deploying)
            .unwrap());

        // Deploying → Error: valid
        assert!(storage
            .update_node_state(&node_id, NodeState::Error)
            .unwrap());

        // Error → Online: valid
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
        let storage = MemoryStorage::new();
        let node = make_node("invalid-test", vec![]);
        let node_id = storage.register_node(node).unwrap();

        // Registered → Active: invalid
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
        let storage = MemoryStorage::new();
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
        let storage = MemoryStorage::new();
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
                    axi_enabled: false,
                    ports: 1,
                    target: "standalone".to_string(),
                    dynamic: false,
                    dynamic_entries: 16,
                    width: 8,
                    ptp: false,
                    rss: false,
                    rss_queues: 4,
                    int: false,
                    int_switch_id: 0,
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
        let storage = MemoryStorage::new();
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
        let storage = MemoryStorage::new();
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
