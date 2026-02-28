//! Domain event types and broadcast channel wrapper for streaming RPCs.

use chrono::{DateTime, Utc};
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
#[derive(Debug, Clone)]
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
}

/// Counter update event with calculated rates.
#[derive(Debug, Clone)]
pub struct CounterEvent {
    pub node_id: String,
    pub counters: Vec<CounterRateData>,
    pub collected_at: DateTime<Utc>,
}

/// Per-rule counter data with rates.
#[derive(Debug, Clone)]
pub struct CounterRateData {
    pub rule_name: String,
    pub match_count: u64,
    pub byte_count: u64,
    pub matches_per_second: f64,
    pub bytes_per_second: f64,
}

/// Node lifecycle events.
#[derive(Debug, Clone)]
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
}
