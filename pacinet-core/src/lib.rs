//! PaciNet Core — domain model and error types
//!
//! Shared types used by the controller, agent, and CLI.

pub mod error;
pub mod fsm;
pub mod hash;
pub mod model;
pub mod storage;
pub mod tls;

pub use error::PaciNetError;
pub use fsm::{FsmDefinition, FsmError, FsmInstance, FsmInstanceStatus, FsmKind};
pub use hash::policy_hash;
pub use model::{
    CounterSnapshot, DeploymentRecord, DeploymentResult, Node, NodeState, PersistentEvent, Policy,
    PolicyVersion, RuleCounter,
};
pub use storage::{LeaderInfo, StatusSummary, Storage};
