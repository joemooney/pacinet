use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::definition::CompileOptions;

/// Status of an FSM instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsmInstanceStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for FsmInstanceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsmInstanceStatus::Running => write!(f, "running"),
            FsmInstanceStatus::Completed => write!(f, "completed"),
            FsmInstanceStatus::Failed => write!(f, "failed"),
            FsmInstanceStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for FsmInstanceStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "running" => Ok(FsmInstanceStatus::Running),
            "completed" => Ok(FsmInstanceStatus::Completed),
            "failed" => Ok(FsmInstanceStatus::Failed),
            "cancelled" => Ok(FsmInstanceStatus::Cancelled),
            _ => Err(format!("unknown FSM instance status: {}", s)),
        }
    }
}

/// Runtime state of an FSM execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsmInstance {
    pub instance_id: String,
    pub definition_name: String,
    pub current_state: String,
    pub status: FsmInstanceStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub context: FsmContext,
    pub history: Vec<FsmTransitionRecord>,
}

/// Context carried by the FSM instance during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsmContext {
    #[serde(default)]
    pub rules_yaml: Option<String>,
    #[serde(default)]
    pub policy_hash: Option<String>,
    #[serde(default)]
    pub compile_options: Option<CompileOptions>,
    #[serde(default)]
    pub target_nodes: Vec<String>,
    #[serde(default)]
    pub deployed_nodes: Vec<String>,
    #[serde(default)]
    pub failed_nodes: Vec<String>,
    #[serde(default)]
    pub last_action_result: Option<ActionResult>,
    #[serde(default)]
    pub batch_cursor: u32,
}

/// Result of an action execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub succeeded: u32,
    pub failed: u32,
    pub total: u32,
    pub node_results: Vec<NodeActionResult>,
}

/// Per-node result of an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeActionResult {
    pub node_id: String,
    pub success: bool,
    pub message: String,
}

/// What triggered a transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionTrigger {
    Condition,
    Timer,
    Manual,
    ActionResult,
    Initial,
}

impl std::fmt::Display for TransitionTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionTrigger::Condition => write!(f, "condition"),
            TransitionTrigger::Timer => write!(f, "timer"),
            TransitionTrigger::Manual => write!(f, "manual"),
            TransitionTrigger::ActionResult => write!(f, "action_result"),
            TransitionTrigger::Initial => write!(f, "initial"),
        }
    }
}

/// Record of a state transition in the FSM history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsmTransitionRecord {
    pub from_state: String,
    pub to_state: String,
    pub trigger: TransitionTrigger,
    pub timestamp: DateTime<Utc>,
    pub message: String,
}

impl FsmInstance {
    /// Create a new FSM instance.
    pub fn new(definition_name: String, initial_state: String, context: FsmContext) -> Self {
        let now = Utc::now();
        Self {
            instance_id: uuid::Uuid::new_v4().to_string(),
            definition_name,
            current_state: initial_state.clone(),
            status: FsmInstanceStatus::Running,
            created_at: now,
            updated_at: now,
            context,
            history: vec![FsmTransitionRecord {
                from_state: String::new(),
                to_state: initial_state,
                trigger: TransitionTrigger::Initial,
                timestamp: now,
                message: "FSM instance started".to_string(),
            }],
        }
    }

    /// Record a state transition.
    pub fn transition(
        &mut self,
        to_state: String,
        trigger: TransitionTrigger,
        message: String,
    ) {
        let now = Utc::now();
        self.history.push(FsmTransitionRecord {
            from_state: self.current_state.clone(),
            to_state: to_state.clone(),
            trigger,
            timestamp: now,
            message,
        });
        self.current_state = to_state;
        self.updated_at = now;
    }

    /// Check if the instance is still running.
    pub fn is_running(&self) -> bool {
        self.status == FsmInstanceStatus::Running
    }
}

impl FsmContext {
    /// Create a new context for a deployment FSM.
    pub fn for_deployment(
        rules_yaml: String,
        compile_options: Option<CompileOptions>,
    ) -> Self {
        Self {
            rules_yaml: Some(rules_yaml),
            policy_hash: None,
            compile_options,
            target_nodes: Vec::new(),
            deployed_nodes: Vec::new(),
            failed_nodes: Vec::new(),
            last_action_result: None,
            batch_cursor: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_creation() {
        let ctx = FsmContext::for_deployment("rules: []".to_string(), None);
        let instance = FsmInstance::new("test-def".to_string(), "start".to_string(), ctx);

        assert_eq!(instance.definition_name, "test-def");
        assert_eq!(instance.current_state, "start");
        assert_eq!(instance.status, FsmInstanceStatus::Running);
        assert!(instance.is_running());
        assert_eq!(instance.history.len(), 1);
    }

    #[test]
    fn test_instance_transition() {
        let ctx = FsmContext::for_deployment("rules: []".to_string(), None);
        let mut instance = FsmInstance::new("test-def".to_string(), "start".to_string(), ctx);

        instance.transition(
            "deployed".to_string(),
            TransitionTrigger::ActionResult,
            "deploy succeeded".to_string(),
        );

        assert_eq!(instance.current_state, "deployed");
        assert_eq!(instance.history.len(), 2);
        assert_eq!(instance.history[1].from_state, "start");
        assert_eq!(instance.history[1].to_state, "deployed");
    }

    #[test]
    fn test_status_display_roundtrip() {
        for status in &[
            FsmInstanceStatus::Running,
            FsmInstanceStatus::Completed,
            FsmInstanceStatus::Failed,
            FsmInstanceStatus::Cancelled,
        ] {
            let s = status.to_string();
            let parsed: FsmInstanceStatus = s.parse().unwrap();
            assert_eq!(status, &parsed);
        }
    }
}
