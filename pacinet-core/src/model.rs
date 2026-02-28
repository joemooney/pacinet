use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// State of a PacGate node in the network
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    Registered,
    Online,
    Deploying,
    Active,
    Error,
    Offline,
}

impl std::fmt::Display for NodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeState::Registered => write!(f, "registered"),
            NodeState::Online => write!(f, "online"),
            NodeState::Deploying => write!(f, "deploying"),
            NodeState::Active => write!(f, "active"),
            NodeState::Error => write!(f, "error"),
            NodeState::Offline => write!(f, "offline"),
        }
    }
}

impl From<pacinet_proto::NodeState> for NodeState {
    fn from(proto: pacinet_proto::NodeState) -> Self {
        match proto {
            pacinet_proto::NodeState::Registered => NodeState::Registered,
            pacinet_proto::NodeState::Online => NodeState::Online,
            pacinet_proto::NodeState::Deploying => NodeState::Deploying,
            pacinet_proto::NodeState::Active => NodeState::Active,
            pacinet_proto::NodeState::Error => NodeState::Error,
            pacinet_proto::NodeState::Offline => NodeState::Offline,
            pacinet_proto::NodeState::Unspecified => NodeState::Offline,
        }
    }
}

impl From<NodeState> for pacinet_proto::NodeState {
    fn from(state: NodeState) -> Self {
        match state {
            NodeState::Registered => pacinet_proto::NodeState::Registered,
            NodeState::Online => pacinet_proto::NodeState::Online,
            NodeState::Deploying => pacinet_proto::NodeState::Deploying,
            NodeState::Active => pacinet_proto::NodeState::Active,
            NodeState::Error => pacinet_proto::NodeState::Error,
            NodeState::Offline => pacinet_proto::NodeState::Offline,
        }
    }
}

/// A PacGate node managed by the controller
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub node_id: String,
    pub hostname: String,
    pub agent_address: String,
    pub labels: HashMap<String, String>,
    pub state: NodeState,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub pacgate_version: String,
}

impl Node {
    pub fn new(
        hostname: String,
        agent_address: String,
        labels: HashMap<String, String>,
        pacgate_version: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            node_id: uuid::Uuid::new_v4().to_string(),
            hostname,
            agent_address,
            labels,
            state: NodeState::Registered,
            registered_at: now,
            last_heartbeat: now,
            pacgate_version,
        }
    }
}

/// A deployed policy on a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub node_id: String,
    pub rules_yaml: String,
    pub policy_hash: String,
    pub deployed_at: DateTime<Utc>,
    pub counters_enabled: bool,
    pub rate_limit_enabled: bool,
    pub conntrack_enabled: bool,
}

/// Counter data from a rule match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCounter {
    pub rule_name: String,
    pub match_count: u64,
    pub byte_count: u64,
}

impl From<pacinet_proto::RuleCounter> for RuleCounter {
    fn from(proto: pacinet_proto::RuleCounter) -> Self {
        Self {
            rule_name: proto.rule_name,
            match_count: proto.match_count,
            byte_count: proto.byte_count,
        }
    }
}

impl From<RuleCounter> for pacinet_proto::RuleCounter {
    fn from(counter: RuleCounter) -> Self {
        pacinet_proto::RuleCounter {
            rule_name: counter.rule_name,
            match_count: counter.match_count,
            byte_count: counter.byte_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = Node::new(
            "node-1".to_string(),
            "192.168.1.10:50055".to_string(),
            HashMap::from([("env".to_string(), "prod".to_string())]),
            "0.1.0".to_string(),
        );
        assert_eq!(node.hostname, "node-1");
        assert_eq!(node.state, NodeState::Registered);
        assert!(!node.node_id.is_empty());
    }

    #[test]
    fn test_node_state_display() {
        assert_eq!(NodeState::Active.to_string(), "active");
        assert_eq!(NodeState::Offline.to_string(), "offline");
    }
}
