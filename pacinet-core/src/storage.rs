use crate::error::PaciNetError;
use crate::fsm::{FsmDefinition, FsmInstance, FsmInstanceStatus, FsmKind};
use crate::model::*;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Summary of fleet status: (total_nodes, nodes_by_state, oldest_heartbeat)
pub type StatusSummary = (usize, HashMap<String, usize>, Option<DateTime<Utc>>);

/// Storage trait for PaciNet controller state.
///
/// All methods are synchronous — callers should wrap in `spawn_blocking` for async contexts.
/// Designed to be object-safe (`Arc<dyn Storage>`).
pub trait Storage: Send + Sync {
    // ---- Node operations ----

    /// Register a node. Returns the node_id.
    fn register_node(&self, node: Node) -> Result<String, PaciNetError>;

    /// Get a node by ID.
    fn get_node(&self, node_id: &str) -> Result<Option<Node>, PaciNetError>;

    /// List nodes, optionally filtered by labels.
    fn list_nodes(&self, label_filter: &HashMap<String, String>)
        -> Result<Vec<Node>, PaciNetError>;

    /// Remove a node and its associated data. Returns true if the node existed.
    fn remove_node(&self, node_id: &str) -> Result<bool, PaciNetError>;

    /// Update heartbeat timestamp, state, and uptime for a node.
    fn update_heartbeat(
        &self,
        node_id: &str,
        state: NodeState,
        uptime: u64,
    ) -> Result<bool, PaciNetError>;

    /// Update just the state of a node.
    fn update_node_state(&self, node_id: &str, state: NodeState) -> Result<bool, PaciNetError>;

    // ---- Counter operations ----

    /// Store counters for a node (replaces previous).
    fn store_counters(&self, node_id: &str, counters: Vec<RuleCounter>)
        -> Result<(), PaciNetError>;

    /// Get counters for a node.
    fn get_counters(&self, node_id: &str) -> Result<Option<Vec<RuleCounter>>, PaciNetError>;

    // ---- Policy operations (with versioning) ----

    /// Store a policy. Returns the version number.
    fn store_policy(&self, policy: Policy) -> Result<u64, PaciNetError>;

    /// Get current policy for a node.
    fn get_policy(&self, node_id: &str) -> Result<Option<Policy>, PaciNetError>;

    /// Get policy version history for a node.
    fn get_policy_history(
        &self,
        node_id: &str,
        limit: u32,
    ) -> Result<Vec<PolicyVersion>, PaciNetError>;

    /// Get current policies for multiple nodes at once.
    fn get_policies_for_nodes(
        &self,
        node_ids: &[String],
    ) -> Result<HashMap<String, Policy>, PaciNetError>;

    // ---- Deploy audit ----

    /// Record a deployment attempt.
    fn record_deployment(&self, record: DeploymentRecord) -> Result<(), PaciNetError>;

    /// Get deployment history for a node.
    fn get_deployments(
        &self,
        node_id: &str,
        limit: u32,
    ) -> Result<Vec<DeploymentRecord>, PaciNetError>;

    // ---- Fleet operations ----

    /// Mark that a deploy is in progress for a node.
    /// Returns error if a deploy is already in progress.
    fn begin_deploy(&self, node_id: &str) -> Result<(), PaciNetError>;

    /// Mark that a deploy has completed for a node.
    fn end_deploy(&self, node_id: &str);

    /// Mark nodes as Offline if their last heartbeat is older than `threshold`.
    /// Returns the list of node_ids that were marked stale.
    fn mark_stale_nodes(&self, threshold: chrono::Duration) -> Result<Vec<String>, PaciNetError>;

    /// Return (total_nodes, nodes_by_state, oldest_heartbeat).
    fn status_summary(&self) -> Result<StatusSummary, PaciNetError>;

    // ---- FSM operations ----

    /// Store an FSM definition (upsert by name).
    fn store_fsm_definition(&self, def: FsmDefinition) -> Result<(), PaciNetError>;

    /// Get an FSM definition by name.
    fn get_fsm_definition(&self, name: &str) -> Result<Option<FsmDefinition>, PaciNetError>;

    /// List FSM definitions, optionally filtered by kind.
    fn list_fsm_definitions(
        &self,
        kind: Option<FsmKind>,
    ) -> Result<Vec<FsmDefinition>, PaciNetError>;

    /// Delete an FSM definition by name. Returns true if it existed.
    fn delete_fsm_definition(&self, name: &str) -> Result<bool, PaciNetError>;

    /// Store a new FSM instance.
    fn store_fsm_instance(&self, instance: FsmInstance) -> Result<(), PaciNetError>;

    /// Get an FSM instance by ID.
    fn get_fsm_instance(&self, id: &str) -> Result<Option<FsmInstance>, PaciNetError>;

    /// Update an existing FSM instance.
    fn update_fsm_instance(&self, instance: FsmInstance) -> Result<(), PaciNetError>;

    /// List FSM instances, optionally filtered by definition name and/or status.
    fn list_fsm_instances(
        &self,
        def_name: Option<&str>,
        status: Option<FsmInstanceStatus>,
    ) -> Result<Vec<FsmInstance>, PaciNetError>;
}
