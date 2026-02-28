use pacinet_core::model::{Node, Policy, RuleCounter};
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory node registry
pub struct NodeRegistry {
    nodes: RwLock<HashMap<String, Node>>,
    policies: RwLock<HashMap<String, Policy>>,
    counters: RwLock<HashMap<String, Vec<RuleCounter>>>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(HashMap::new()),
            policies: RwLock::new(HashMap::new()),
            counters: RwLock::new(HashMap::new()),
        }
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
                    node.labels.get(k).map_or(false, |nv| nv == v)
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
}
