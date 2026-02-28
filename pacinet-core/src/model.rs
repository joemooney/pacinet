use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// State of a PacGate node in the network
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum NodeState {
    Registered,
    Online,
    Deploying,
    Active,
    Error,
    Offline,
}

impl NodeState {
    /// Returns the set of valid target states for this state.
    pub fn valid_transitions(&self) -> &[NodeState] {
        match self {
            NodeState::Registered => &[NodeState::Online, NodeState::Offline],
            NodeState::Online => &[NodeState::Deploying, NodeState::Error, NodeState::Offline],
            NodeState::Deploying => &[NodeState::Active, NodeState::Error, NodeState::Offline],
            NodeState::Active => &[NodeState::Deploying, NodeState::Error, NodeState::Offline],
            NodeState::Error => &[NodeState::Online, NodeState::Deploying, NodeState::Offline],
            NodeState::Offline => &[NodeState::Online],
        }
    }

    /// Check if transitioning to `target` is valid.
    pub fn can_transition_to(&self, target: &NodeState) -> bool {
        self.valid_transitions().contains(target)
    }
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

impl FromStr for NodeState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "registered" => Ok(NodeState::Registered),
            "online" => Ok(NodeState::Online),
            "deploying" => Ok(NodeState::Deploying),
            "active" => Ok(NodeState::Active),
            "error" => Ok(NodeState::Error),
            "offline" => Ok(NodeState::Offline),
            _ => Err(format!("unknown node state: {}", s)),
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
    pub uptime_seconds: u64,
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
            uptime_seconds: 0,
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

/// Versioned snapshot of a policy deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyVersion {
    pub version: u64,
    pub node_id: String,
    pub rules_yaml: String,
    pub policy_hash: String,
    pub deployed_at: DateTime<Utc>,
    pub counters_enabled: bool,
    pub rate_limit_enabled: bool,
    pub conntrack_enabled: bool,
}

/// Result of a deployment attempt
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentResult {
    Success,
    AgentFailure,
    AgentUnreachable,
    Timeout,
}

impl std::fmt::Display for DeploymentResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeploymentResult::Success => write!(f, "success"),
            DeploymentResult::AgentFailure => write!(f, "agent_failure"),
            DeploymentResult::AgentUnreachable => write!(f, "agent_unreachable"),
            DeploymentResult::Timeout => write!(f, "timeout"),
        }
    }
}

impl FromStr for DeploymentResult {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "success" => Ok(DeploymentResult::Success),
            "agent_failure" => Ok(DeploymentResult::AgentFailure),
            "agent_unreachable" => Ok(DeploymentResult::AgentUnreachable),
            "timeout" => Ok(DeploymentResult::Timeout),
            _ => Err(format!("unknown deployment result: {}", s)),
        }
    }
}

/// Audit trail for a deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub id: String,
    pub node_id: String,
    pub policy_version: u64,
    pub policy_hash: String,
    pub deployed_at: DateTime<Utc>,
    pub result: DeploymentResult,
    pub message: String,
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
        assert_eq!(node.uptime_seconds, 0);
    }

    #[test]
    fn test_node_state_display() {
        assert_eq!(NodeState::Active.to_string(), "active");
        assert_eq!(NodeState::Offline.to_string(), "offline");
    }

    #[test]
    fn test_node_state_fromstr() {
        assert_eq!(NodeState::from_str("active").unwrap(), NodeState::Active);
        assert_eq!(NodeState::from_str("ONLINE").unwrap(), NodeState::Online);
        assert!(NodeState::from_str("invalid").is_err());
    }

    #[test]
    fn test_valid_state_transitions() {
        // Registered → Online: valid
        assert!(NodeState::Registered.can_transition_to(&NodeState::Online));
        // Registered → Active: invalid
        assert!(!NodeState::Registered.can_transition_to(&NodeState::Active));
        // Online → Deploying: valid
        assert!(NodeState::Online.can_transition_to(&NodeState::Deploying));
        // Deploying → Active: valid
        assert!(NodeState::Deploying.can_transition_to(&NodeState::Active));
        // Active → Deploying: valid (redeploy)
        assert!(NodeState::Active.can_transition_to(&NodeState::Deploying));
        // Error → Online: valid (recovery)
        assert!(NodeState::Error.can_transition_to(&NodeState::Online));
        // Offline → Online: valid
        assert!(NodeState::Offline.can_transition_to(&NodeState::Online));
        // Offline → Active: invalid
        assert!(!NodeState::Offline.can_transition_to(&NodeState::Active));
    }

    #[test]
    fn test_deployment_result_roundtrip() {
        for result in &[
            DeploymentResult::Success,
            DeploymentResult::AgentFailure,
            DeploymentResult::AgentUnreachable,
            DeploymentResult::Timeout,
        ] {
            let s = result.to_string();
            let parsed = DeploymentResult::from_str(&s).unwrap();
            assert_eq!(result, &parsed);
        }
    }
}
