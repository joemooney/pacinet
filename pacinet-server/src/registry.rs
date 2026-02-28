use pacinet_core::model::{Node, Policy, RuleCounter};
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory node registry
pub struct NodeRegistry {
    nodes: RwLock<HashMap<String, Node>>,
    policies: RwLock<HashMap<String, Policy>>,
    counters: RwLock<HashMap<String, Vec<RuleCounter>>>,
}

impl Default for NodeRegistry {
    fn default() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            policies: RwLock::new(HashMap::new()),
            counters: RwLock::new(HashMap::new()),
        }
    }
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_node(&self, node: Node) -> String {
        let node_id = node.node_id.clone();
        self.nodes.write().unwrap().insert(node_id.clone(), node);
        node_id
    }

    pub fn get_node(&self, node_id: &str) -> Option<Node> {
        self.nodes.read().unwrap().get(node_id).cloned()
    }

    pub fn list_nodes(&self, label_filter: &HashMap<String, String>) -> Vec<Node> {
        let nodes = self.nodes.read().unwrap();
        nodes
            .values()
            .filter(|node| {
                label_filter.iter().all(|(k, v)| {
                    node.labels.get(k) == Some(v)
                })
            })
            .cloned()
            .collect()
    }

    pub fn remove_node(&self, node_id: &str) -> bool {
        self.policies.write().unwrap().remove(node_id);
        self.counters.write().unwrap().remove(node_id);
        self.nodes.write().unwrap().remove(node_id).is_some()
    }

    pub fn update_heartbeat(&self, node_id: &str, state: pacinet_core::NodeState) -> bool {
        let mut nodes = self.nodes.write().unwrap();
        if let Some(node) = nodes.get_mut(node_id) {
            node.last_heartbeat = chrono::Utc::now();
            node.state = state;
            true
        } else {
            false
        }
    }

    pub fn store_counters(&self, node_id: &str, counters: Vec<RuleCounter>) {
        self.counters
            .write()
            .unwrap()
            .insert(node_id.to_string(), counters);
    }

    pub fn get_counters(&self, node_id: &str) -> Option<Vec<RuleCounter>> {
        self.counters.read().unwrap().get(node_id).cloned()
    }

    pub fn store_policy(&self, policy: Policy) {
        self.policies
            .write()
            .unwrap()
            .insert(policy.node_id.clone(), policy);
    }

    pub fn get_policy(&self, node_id: &str) -> Option<Policy> {
        self.policies.read().unwrap().get(node_id).cloned()
    }

    pub fn update_node_state(&self, node_id: &str, state: pacinet_core::NodeState) -> bool {
        let mut nodes = self.nodes.write().unwrap();
        if let Some(node) = nodes.get_mut(node_id) {
            node.state = state;
            true
        } else {
            false
        }
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
        let registry = NodeRegistry::new();
        let node = make_node("test-host", vec![("env", "dev")]);
        let node_id = registry.register_node(node);

        let retrieved = registry.get_node(&node_id).unwrap();
        assert_eq!(retrieved.hostname, "test-host");
        assert_eq!(retrieved.labels.get("env").unwrap(), "dev");
    }

    #[test]
    fn test_remove_cleans_up_policies_and_counters() {
        let registry = NodeRegistry::new();
        let node = make_node("remove-me", vec![]);
        let node_id = registry.register_node(node);

        // Store policy and counters
        registry.store_policy(Policy {
            node_id: node_id.clone(),
            rules_yaml: "rules: []".to_string(),
            policy_hash: "abc123".to_string(),
            deployed_at: chrono::Utc::now(),
            counters_enabled: false,
            rate_limit_enabled: false,
            conntrack_enabled: false,
        });
        registry.store_counters(
            &node_id,
            vec![RuleCounter {
                rule_name: "rule1".to_string(),
                match_count: 10,
                byte_count: 100,
            }],
        );

        assert!(registry.get_policy(&node_id).is_some());
        assert!(registry.get_counters(&node_id).is_some());

        // Remove node — should clean up everything
        assert!(registry.remove_node(&node_id));
        assert!(registry.get_node(&node_id).is_none());
        assert!(registry.get_policy(&node_id).is_none());
        assert!(registry.get_counters(&node_id).is_none());
    }

    #[test]
    fn test_label_filtering() {
        let registry = NodeRegistry::new();
        registry.register_node(make_node("prod-1", vec![("env", "prod"), ("region", "us")]));
        registry.register_node(make_node("dev-1", vec![("env", "dev"), ("region", "us")]));
        registry.register_node(make_node("prod-2", vec![("env", "prod"), ("region", "eu")]));

        // Filter by env=prod
        let filter: std::collections::HashMap<String, String> =
            [("env".to_string(), "prod".to_string())].into();
        let nodes = registry.list_nodes(&filter);
        assert_eq!(nodes.len(), 2);

        // Filter by region=us
        let filter: std::collections::HashMap<String, String> =
            [("region".to_string(), "us".to_string())].into();
        let nodes = registry.list_nodes(&filter);
        assert_eq!(nodes.len(), 2);

        // Filter by both
        let filter: std::collections::HashMap<String, String> = [
            ("env".to_string(), "prod".to_string()),
            ("region".to_string(), "eu".to_string()),
        ]
        .into();
        let nodes = registry.list_nodes(&filter);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].hostname, "prod-2");

        // Empty filter — all nodes
        let nodes = registry.list_nodes(&std::collections::HashMap::new());
        assert_eq!(nodes.len(), 3);
    }

    #[test]
    fn test_update_node_state() {
        let registry = NodeRegistry::new();
        let node = make_node("state-test", vec![]);
        let node_id = registry.register_node(node);

        // Initial state is Registered
        let node = registry.get_node(&node_id).unwrap();
        assert!(matches!(node.state, pacinet_core::NodeState::Registered));

        // Update to Active
        assert!(registry.update_node_state(&node_id, pacinet_core::NodeState::Active));
        let node = registry.get_node(&node_id).unwrap();
        assert!(matches!(node.state, pacinet_core::NodeState::Active));

        // Update to Error
        assert!(registry.update_node_state(&node_id, pacinet_core::NodeState::Error));
        let node = registry.get_node(&node_id).unwrap();
        assert!(matches!(node.state, pacinet_core::NodeState::Error));

        // Non-existent node
        assert!(!registry.update_node_state("nonexistent", pacinet_core::NodeState::Online));
    }
}
