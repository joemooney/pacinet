use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::error::FsmError;
use super::parse_duration;

/// Kind of FSM — determines which actions are available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsmKind {
    Deployment,
    AdaptivePolicy,
}

impl std::fmt::Display for FsmKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsmKind::Deployment => write!(f, "deployment"),
            FsmKind::AdaptivePolicy => write!(f, "adaptive_policy"),
        }
    }
}

impl std::str::FromStr for FsmKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "deployment" => Ok(FsmKind::Deployment),
            "adaptive_policy" => Ok(FsmKind::AdaptivePolicy),
            _ => Err(format!("unknown FSM kind: {}", s)),
        }
    }
}

/// YAML-parseable FSM definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsmDefinition {
    pub name: String,
    pub description: String,
    pub kind: FsmKind,
    pub initial: String,
    pub states: HashMap<String, StateDefinition>,
}

/// A single state in the FSM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDefinition {
    #[serde(default)]
    pub action: Option<ActionDefinition>,
    #[serde(default)]
    pub transitions: Vec<TransitionDefinition>,
    #[serde(default)]
    pub terminal: bool,
}

/// A transition from one state to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionDefinition {
    pub to: String,
    #[serde(default)]
    pub when: Option<ConditionDefinition>,
    #[serde(default)]
    pub after: Option<String>,
}

/// Conditions that trigger transitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConditionDefinition {
    Simple(SimpleCondition),
    Counter(CounterCondition),
    Compound(CompoundCondition),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleCondition {
    #[serde(default)]
    pub all_succeeded: Option<bool>,
    #[serde(default)]
    pub any_failed: Option<bool>,
    #[serde(default)]
    pub manual: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterCondition {
    pub counter: String,
    #[serde(default)]
    pub rate_above: Option<f64>,
    #[serde(default)]
    pub rate_below: Option<f64>,
    #[serde(default)]
    pub total_above: Option<u64>,
    #[serde(default, rename = "for")]
    pub for_duration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompoundCondition {
    #[serde(default)]
    pub and: Option<Vec<ConditionDefinition>>,
    #[serde(default)]
    pub or: Option<Vec<ConditionDefinition>>,
    #[serde(default)]
    pub not: Option<Box<ConditionDefinition>>,
}

/// Actions executed when entering a state.
/// Uses optional fields rather than an enum because serde_yaml 0.9 requires
/// YAML tags for externally tagged enums, but we want natural map-style YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDefinition {
    #[serde(default)]
    pub deploy: Option<DeployAction>,
    #[serde(default)]
    pub rollback: Option<RollbackAction>,
    #[serde(default)]
    pub alert: Option<AlertAction>,
}

impl ActionDefinition {
    pub fn kind(&self) -> &str {
        if self.deploy.is_some() {
            "deploy"
        } else if self.rollback.is_some() {
            "rollback"
        } else if self.alert.is_some() {
            "alert"
        } else {
            "none"
        }
    }
}

/// Deploy action — select nodes and deploy rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployAction {
    pub select: NodeSelector,
    #[serde(default)]
    pub batch_percent: Option<u32>,
    #[serde(default)]
    pub options: Option<CompileOptions>,
}

/// Node selector for deploy actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSelector {
    #[serde(default)]
    pub label: HashMap<String, String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Compile options matching proto CompileOptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileOptions {
    #[serde(default)]
    pub counters: bool,
    #[serde(default)]
    pub rate_limit: bool,
    #[serde(default)]
    pub conntrack: bool,
}

/// Rollback action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackAction {
    pub target: RollbackTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackTarget {
    Previous,
    Version(u64),
}

/// Alert action (log-only for now).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertAction {
    #[serde(default)]
    pub channel: Option<String>,
    pub message: String,
}

impl FsmDefinition {
    /// Parse an FSM definition from YAML.
    pub fn from_yaml(yaml: &str) -> Result<Self, FsmError> {
        serde_yaml::from_str(yaml).map_err(|e| FsmError::YamlParse(e.to_string()))
    }

    /// Validate the FSM definition for consistency.
    pub fn validate(&self) -> Result<(), FsmError> {
        // Check initial state exists
        if !self.states.contains_key(&self.initial) {
            return Err(FsmError::InvalidDefinition(format!(
                "initial state '{}' not found in states",
                self.initial
            )));
        }

        // Check all transition targets exist
        for (state_name, state_def) in &self.states {
            for transition in &state_def.transitions {
                if !self.states.contains_key(&transition.to) {
                    return Err(FsmError::InvalidDefinition(format!(
                        "state '{}' has transition to unknown state '{}'",
                        state_name, transition.to
                    )));
                }

                // Validate duration strings
                if let Some(ref after) = transition.after {
                    parse_duration(after).map_err(|e| {
                        FsmError::InvalidDefinition(format!(
                            "state '{}' has invalid duration '{}': {}",
                            state_name, after, e
                        ))
                    })?;
                }
            }

            // Terminal states must have no transitions
            if state_def.terminal && !state_def.transitions.is_empty() {
                return Err(FsmError::InvalidDefinition(format!(
                    "terminal state '{}' must not have transitions",
                    state_name
                )));
            }
        }

        // Must have at least one terminal state
        let has_terminal = self.states.values().any(|s| s.terminal);
        if !has_terminal {
            return Err(FsmError::InvalidDefinition(
                "FSM must have at least one terminal state".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANARY_YAML: &str = r#"
name: canary-rollout
kind: deployment
description: Canary then staged rollout with auto-rollback
initial: canary
states:
  canary:
    action:
      deploy:
        select: { label: { canary: "true" }, limit: 1 }
    transitions:
      - to: validate
        when: { all_succeeded: true }
      - to: rollback
        when: { any_failed: true }
  validate:
    transitions:
      - to: staged
        after: 5m
      - to: rollback
        when: { manual: true }
  staged:
    action:
      deploy:
        select: { label: { env: prod } }
        batch_percent: 25
    transitions:
      - to: complete
        when: { all_succeeded: true }
      - to: rollback
        when: { any_failed: true }
  rollback:
    action:
      rollback: { target: previous }
    terminal: true
  complete:
    terminal: true
"#;

    #[test]
    fn test_parse_canary_yaml() {
        let def = FsmDefinition::from_yaml(CANARY_YAML).unwrap();
        assert_eq!(def.name, "canary-rollout");
        assert_eq!(def.kind, FsmKind::Deployment);
        assert_eq!(def.initial, "canary");
        assert_eq!(def.states.len(), 5);

        // Check canary state
        let canary = &def.states["canary"];
        assert!(!canary.terminal);
        assert_eq!(canary.transitions.len(), 2);
        assert!(canary.action.is_some());

        // Check terminal states
        assert!(def.states["rollback"].terminal);
        assert!(def.states["complete"].terminal);
    }

    #[test]
    fn test_validate_canary() {
        let def = FsmDefinition::from_yaml(CANARY_YAML).unwrap();
        def.validate().unwrap();
    }

    #[test]
    fn test_validate_missing_initial() {
        let yaml = r#"
name: bad
kind: deployment
description: test
initial: nonexistent
states:
  start:
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        let err = def.validate().unwrap_err();
        assert!(err.to_string().contains("initial state 'nonexistent'"));
    }

    #[test]
    fn test_validate_bad_transition_target() {
        let yaml = r#"
name: bad
kind: deployment
description: test
initial: start
states:
  start:
    transitions:
      - to: nowhere
  end:
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        let err = def.validate().unwrap_err();
        assert!(err.to_string().contains("unknown state 'nowhere'"));
    }

    #[test]
    fn test_validate_terminal_with_transitions() {
        let yaml = r#"
name: bad
kind: deployment
description: test
initial: start
states:
  start:
    terminal: true
    transitions:
      - to: start
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        let err = def.validate().unwrap_err();
        assert!(err.to_string().contains("terminal state 'start' must not"));
    }

    #[test]
    fn test_validate_no_terminal() {
        let yaml = r#"
name: bad
kind: deployment
description: test
initial: start
states:
  start:
    transitions:
      - to: start
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        let err = def.validate().unwrap_err();
        assert!(err.to_string().contains("at least one terminal state"));
    }

    #[test]
    fn test_validate_bad_duration() {
        let yaml = r#"
name: bad
kind: deployment
description: test
initial: start
states:
  start:
    transitions:
      - to: end
        after: "5x"
  end:
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        let err = def.validate().unwrap_err();
        assert!(err.to_string().contains("invalid duration"));
    }

    #[test]
    fn test_fsm_kind_display_roundtrip() {
        assert_eq!(FsmKind::Deployment.to_string(), "deployment");
        assert_eq!(FsmKind::AdaptivePolicy.to_string(), "adaptive_policy");
        assert_eq!("deployment".parse::<FsmKind>().unwrap(), FsmKind::Deployment);
    }

    #[test]
    fn test_deploy_action_parsing() {
        let yaml = r#"
name: simple
kind: deployment
description: test
initial: deploy
states:
  deploy:
    action:
      deploy:
        select:
          label: { env: prod }
          limit: 5
        batch_percent: 25
        options:
          counters: true
    transitions:
      - to: done
        when: { all_succeeded: true }
  done:
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        def.validate().unwrap();

        let deploy_state = &def.states["deploy"];
        let action = deploy_state.action.as_ref().unwrap();
        let deploy = action.deploy.as_ref().expect("expected deploy action");
        assert_eq!(deploy.select.label.get("env").unwrap(), "prod");
        assert_eq!(deploy.select.limit, Some(5));
        assert_eq!(deploy.batch_percent, Some(25));
        assert!(deploy.options.as_ref().unwrap().counters);
    }

    #[test]
    fn test_rollback_action_parsing() {
        let yaml = r#"
name: rb
kind: deployment
description: test
initial: rollback
states:
  rollback:
    action:
      rollback: { target: previous }
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        def.validate().unwrap();

        let rb_state = &def.states["rollback"];
        let action = rb_state.action.as_ref().unwrap();
        let rollback = action.rollback.as_ref().expect("expected rollback action");
        assert!(matches!(rollback.target, RollbackTarget::Previous));
    }

    #[test]
    fn test_alert_action_parsing() {
        let yaml = r#"
name: alert-test
kind: deployment
description: test
initial: alert
states:
  alert:
    action:
      alert: { channel: ops, message: "Deployment started" }
    transitions:
      - to: done
        after: 1s
  done:
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        def.validate().unwrap();

        let alert_state = &def.states["alert"];
        let action = alert_state.action.as_ref().unwrap();
        let alert = action.alert.as_ref().expect("expected alert action");
        assert_eq!(alert.channel.as_deref(), Some("ops"));
        assert_eq!(alert.message, "Deployment started");
    }
}
