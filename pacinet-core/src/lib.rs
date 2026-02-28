//! PaciNet Core — domain model and error types
//!
//! Shared types used by the controller, agent, and CLI.

pub mod error;
pub mod model;

pub use error::PaciNetError;
pub use model::{Node, NodeState, Policy, RuleCounter};
