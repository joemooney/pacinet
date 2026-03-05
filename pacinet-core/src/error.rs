use crate::fsm::error::FsmError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PaciNetError {
    #[error("node not found: {0}")]
    NodeNotFound(String),

    #[error("node already registered: {0}")]
    NodeAlreadyRegistered(String),

    #[error("deployment failed: {0}")]
    DeploymentFailed(String),

    #[error("pacgate error: {0}")]
    PacGateError(String),

    #[error("agent unreachable: {0}")]
    AgentUnreachable(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("concurrent deploy in progress for node: {0}")]
    ConcurrentDeploy(String),

    #[error("FSM error: {0}")]
    Fsm(#[from] FsmError),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<PaciNetError> for tonic::Status {
    fn from(err: PaciNetError) -> Self {
        match &err {
            PaciNetError::NodeNotFound(_) => tonic::Status::not_found(err.to_string()),
            PaciNetError::NodeAlreadyRegistered(_) => {
                tonic::Status::already_exists(err.to_string())
            }
            PaciNetError::DeploymentFailed(_) => {
                tonic::Status::failed_precondition(err.to_string())
            }
            PaciNetError::PacGateError(_) => tonic::Status::internal(err.to_string()),
            PaciNetError::AgentUnreachable(_) => tonic::Status::unavailable(err.to_string()),
            PaciNetError::InvalidConfig(_) => tonic::Status::invalid_argument(err.to_string()),
            PaciNetError::InvalidStateTransition { .. } => {
                tonic::Status::failed_precondition(err.to_string())
            }
            PaciNetError::ConcurrentDeploy(_) => tonic::Status::aborted(err.to_string()),
            PaciNetError::Fsm(e) => match e {
                FsmError::InstanceNotFound(_) | FsmError::DefinitionNotFound(_) => {
                    tonic::Status::not_found(err.to_string())
                }
                FsmError::AlreadyCompleted => tonic::Status::failed_precondition(err.to_string()),
                FsmError::InvalidDefinition(_) | FsmError::YamlParse(_) => {
                    tonic::Status::invalid_argument(err.to_string())
                }
                _ => tonic::Status::internal(err.to_string()),
            },
            PaciNetError::Internal(_) => tonic::Status::internal(err.to_string()),
        }
    }
}
