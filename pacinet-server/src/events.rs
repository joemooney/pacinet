//! Domain event types and broadcast channel wrapper for streaming RPCs.

use chrono::{DateTime, Utc};
use pacinet_core::PersistentEvent;
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::broadcast;

/// Central event bus wrapping broadcast channels for FSM, counter, and node events.
#[derive(Clone)]
pub struct EventBus {
    pub fsm_tx: broadcast::Sender<FsmEvent>,
    pub counter_tx: broadcast::Sender<CounterEvent>,
    pub node_tx: broadcast::Sender<NodeEvent>,
}

impl EventBus {
    pub fn new(buffer_size: usize) -> Self {
        let (fsm_tx, _) = broadcast::channel(buffer_size);
        let (counter_tx, _) = broadcast::channel(buffer_size);
        let (node_tx, _) = broadcast::channel(buffer_size);
        Self {
            fsm_tx,
            counter_tx,
            node_tx,
        }
    }

    /// Emit an FSM event. Silently drops if no receivers.
    pub fn emit_fsm(&self, event: FsmEvent) {
        let _ = self.fsm_tx.send(event);
    }

    /// Emit a counter event. Silently drops if no receivers.
    pub fn emit_counter(&self, event: CounterEvent) {
        let _ = self.counter_tx.send(event);
    }

    /// Emit a node event. Silently drops if no receivers.
    pub fn emit_node(&self, event: NodeEvent) {
        let _ = self.node_tx.send(event);
    }
}

/// FSM lifecycle events.
#[derive(Debug, Clone, Serialize)]
pub enum FsmEvent {
    Transition {
        instance_id: String,
        definition_name: String,
        from_state: String,
        to_state: String,
        trigger: String,
        message: String,
        timestamp: DateTime<Utc>,
    },
    DeployProgress {
        instance_id: String,
        definition_name: String,
        deployed_nodes: u32,
        failed_nodes: u32,
        target_nodes: u32,
        timestamp: DateTime<Utc>,
    },
    InstanceCompleted {
        instance_id: String,
        definition_name: String,
        final_status: String,
        timestamp: DateTime<Utc>,
    },
}

impl FsmEvent {
    pub fn instance_id(&self) -> &str {
        match self {
            FsmEvent::Transition { instance_id, .. } => instance_id,
            FsmEvent::DeployProgress { instance_id, .. } => instance_id,
            FsmEvent::InstanceCompleted { instance_id, .. } => instance_id,
        }
    }

    pub fn to_persistent(&self) -> PersistentEvent {
        let (event_type, source, timestamp) = match self {
            FsmEvent::Transition {
                instance_id,
                timestamp,
                ..
            } => ("fsm.transition", instance_id.as_str(), *timestamp),
            FsmEvent::DeployProgress {
                instance_id,
                timestamp,
                ..
            } => ("fsm.deploy_progress", instance_id.as_str(), *timestamp),
            FsmEvent::InstanceCompleted {
                instance_id,
                timestamp,
                ..
            } => ("fsm.completed", instance_id.as_str(), *timestamp),
        };
        PersistentEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: event_type.to_string(),
            source: source.to_string(),
            payload: serde_json::to_string(self).unwrap_or_default(),
            timestamp,
        }
    }
}

/// Counter update event with calculated rates.
#[derive(Debug, Clone, Serialize)]
pub struct CounterEvent {
    pub node_id: String,
    pub counters: Vec<CounterRateData>,
    pub collected_at: DateTime<Utc>,
}

/// Per-rule counter data with rates.
#[derive(Debug, Clone, Serialize)]
pub struct CounterRateData {
    pub rule_name: String,
    pub match_count: u64,
    pub byte_count: u64,
    pub matches_per_second: f64,
    pub bytes_per_second: f64,
}

/// Node lifecycle events.
#[derive(Debug, Clone, Serialize)]
pub enum NodeEvent {
    Registered {
        node_id: String,
        hostname: String,
        labels: HashMap<String, String>,
        timestamp: DateTime<Utc>,
    },
    StateChanged {
        node_id: String,
        hostname: String,
        labels: HashMap<String, String>,
        old_state: String,
        new_state: String,
        timestamp: DateTime<Utc>,
    },
    HeartbeatStale {
        node_id: String,
        hostname: String,
        labels: HashMap<String, String>,
        timestamp: DateTime<Utc>,
    },
    Removed {
        node_id: String,
        hostname: String,
        labels: HashMap<String, String>,
        timestamp: DateTime<Utc>,
    },
}

impl CounterEvent {
    pub fn to_persistent(&self) -> PersistentEvent {
        PersistentEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: "counter.update".to_string(),
            source: self.node_id.clone(),
            payload: serde_json::to_string(self).unwrap_or_default(),
            timestamp: self.collected_at,
        }
    }
}

impl NodeEvent {
    pub fn node_id(&self) -> &str {
        match self {
            NodeEvent::Registered { node_id, .. } => node_id,
            NodeEvent::StateChanged { node_id, .. } => node_id,
            NodeEvent::HeartbeatStale { node_id, .. } => node_id,
            NodeEvent::Removed { node_id, .. } => node_id,
        }
    }

    pub fn labels(&self) -> &HashMap<String, String> {
        match self {
            NodeEvent::Registered { labels, .. } => labels,
            NodeEvent::StateChanged { labels, .. } => labels,
            NodeEvent::HeartbeatStale { labels, .. } => labels,
            NodeEvent::Removed { labels, .. } => labels,
        }
    }

    pub fn to_persistent(&self) -> PersistentEvent {
        let (event_type, node_id, timestamp) = match self {
            NodeEvent::Registered {
                node_id, timestamp, ..
            } => ("node.registered", node_id.as_str(), *timestamp),
            NodeEvent::StateChanged {
                node_id, timestamp, ..
            } => ("node.state_changed", node_id.as_str(), *timestamp),
            NodeEvent::HeartbeatStale {
                node_id, timestamp, ..
            } => ("node.heartbeat_stale", node_id.as_str(), *timestamp),
            NodeEvent::Removed {
                node_id, timestamp, ..
            } => ("node.removed", node_id.as_str(), *timestamp),
        };
        PersistentEvent {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: event_type.to_string(),
            source: node_id.to_string(),
            payload: serde_json::to_string(self).unwrap_or_default(),
            timestamp,
        }
    }
}
