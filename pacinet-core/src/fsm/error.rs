use thiserror::Error;

#[derive(Debug, Error)]
pub enum FsmError {
    #[error("invalid FSM definition: {0}")]
    InvalidDefinition(String),

    #[error("FSM instance not found: {0}")]
    InstanceNotFound(String),

    #[error("FSM definition not found: {0}")]
    DefinitionNotFound(String),

    #[error("invalid state: {0}")]
    InvalidState(String),

    #[error("no valid transition from current state: {0}")]
    NoTransition(String),

    #[error("FSM instance already completed")]
    AlreadyCompleted,

    #[error("FSM action error: {0}")]
    ActionError(String),

    #[error("YAML parse error: {0}")]
    YamlParse(String),
}
