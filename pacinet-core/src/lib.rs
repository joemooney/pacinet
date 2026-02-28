//! PaciNet Core — domain model and error types
//!
//! Shared types used by the controller, agent, and CLI.

pub mod error;
pub mod model;
pub mod storage;

pub use error::PaciNetError;
pub use model::{
    DeploymentRecord, DeploymentResult, Node, NodeState, Policy, PolicyVersion, RuleCounter,
};
pub use storage::{StatusSummary, Storage};
