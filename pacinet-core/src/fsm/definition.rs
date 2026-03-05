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
/// Order matters for serde(untagged):
/// - Counter first: has required `counter` field, won't match simple/compound
/// - Simple second: matches `all_succeeded`, `any_failed`, `manual`
/// - Compound last: `and`/`or`/`not` are all optional, would match anything
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConditionDefinition {
    Counter(CounterCondition),
    Simple(SimpleCondition),
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
    /// Aggregation mode: "any" (default), "all", or "sum"
    #[serde(default)]
    pub aggregate: Option<String>,
    /// Counter field: "matches" (default) or "bytes"
    #[serde(default)]
    pub field: Option<String>,
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
    #[serde(default)]
    pub axi: bool,
    #[serde(default = "default_ports")]
    pub ports: u32,
    #[serde(default = "default_target")]
    pub target: String,
    #[serde(default)]
    pub dynamic: bool,
    #[serde(default = "default_dynamic_entries")]
    pub dynamic_entries: u32,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default)]
    pub ptp: bool,
    #[serde(default)]
    pub rss: bool,
    #[serde(default = "default_rss_queues")]
    pub rss_queues: u32,
    #[serde(default)]
    pub int: bool,
    #[serde(default)]
    pub int_switch_id: u32,
}

fn default_ports() -> u32 {
    1
}

fn default_target() -> String {
    "standalone".to_string()
}

fn default_dynamic_entries() -> u32 {
    16
}

fn default_width() -> u32 {
    8
}

fn default_rss_queues() -> u32 {
    4
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

/// Alert action with optional webhook delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertAction {
    #[serde(default)]
    pub channel: Option<String>,
    pub message: String,
    #[serde(default)]
    pub webhook: Option<WebhookConfig>,
}

/// Webhook delivery configuration (per-alert in FSM YAML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub bearer_token: Option<String>,
    #[serde(default)]
    pub basic_auth: Option<BasicAuth>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub max_retries: Option<u32>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Basic auth credentials for webhooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
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

        // Validate counter conditions
        for (state_name, state_def) in &self.states {
            for (ti, transition) in state_def.transitions.iter().enumerate() {
                if let Some(ref cond) = transition.when {
                    self.validate_condition(cond, state_name, ti)?;
                }
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

    fn validate_condition(
        &self,
        cond: &ConditionDefinition,
        state_name: &str,
        transition_idx: usize,
    ) -> Result<(), FsmError> {
        match cond {
            ConditionDefinition::Counter(cc) => {
                if cc.rate_above.is_none() && cc.rate_below.is_none() && cc.total_above.is_none() {
                    return Err(FsmError::InvalidDefinition(format!(
                        "state '{}' transition {} counter condition must set at least one of rate_above, rate_below, total_above",
                        state_name, transition_idx
                    )));
                }
                if let Some(ref agg) = cc.aggregate {
                    match agg.as_str() {
                        "any" | "all" | "sum" => {}
                        _ => {
                            return Err(FsmError::InvalidDefinition(format!(
                                "state '{}' transition {} invalid aggregate mode '{}' (use any/all/sum)",
                                state_name, transition_idx, agg
                            )));
                        }
                    }
                }
                if let Some(ref field) = cc.field {
                    match field.as_str() {
                        "matches" | "bytes" => {}
                        _ => {
                            return Err(FsmError::InvalidDefinition(format!(
                                "state '{}' transition {} invalid field '{}' (use matches/bytes)",
                                state_name, transition_idx, field
                            )));
                        }
                    }
                }
                if let Some(ref dur) = cc.for_duration {
                    super::parse_duration(dur).map_err(|e| {
                        FsmError::InvalidDefinition(format!(
                            "state '{}' transition {} invalid for_duration '{}': {}",
                            state_name, transition_idx, dur, e
                        ))
                    })?;
                }
                Ok(())
            }
            ConditionDefinition::Compound(compound) => {
                if let Some(ref conditions) = compound.and {
                    for c in conditions {
                        self.validate_condition(c, state_name, transition_idx)?;
                    }
                }
                if let Some(ref conditions) = compound.or {
                    for c in conditions {
                        self.validate_condition(c, state_name, transition_idx)?;
                    }
                }
                if let Some(ref inner) = compound.not {
                    self.validate_condition(inner, state_name, transition_idx)?;
                }
                Ok(())
            }
            ConditionDefinition::Simple(_) => Ok(()),
        }
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
        assert_eq!(
            "deployment".parse::<FsmKind>().unwrap(),
            FsmKind::Deployment
        );
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

    #[test]
    fn test_counter_condition_parsing() {
        let yaml = r#"
name: counter-test
kind: adaptive_policy
description: test counter conditions
initial: monitoring
states:
  monitoring:
    transitions:
      - to: escalated
        when:
          counter: drop_all
          rate_above: 1000.0
          for: 30s
          aggregate: any
          field: matches
  escalated:
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        def.validate().unwrap();

        let monitoring = &def.states["monitoring"];
        let transition = &monitoring.transitions[0];
        if let Some(ConditionDefinition::Counter(cc)) = &transition.when {
            assert_eq!(cc.counter, "drop_all");
            assert_eq!(cc.rate_above, Some(1000.0));
            assert_eq!(cc.for_duration.as_deref(), Some("30s"));
            assert_eq!(cc.aggregate.as_deref(), Some("any"));
            assert_eq!(cc.field.as_deref(), Some("matches"));
        } else {
            panic!("Expected counter condition");
        }
    }

    #[test]
    fn test_counter_condition_validation_no_threshold() {
        let yaml = r#"
name: bad
kind: adaptive_policy
description: test
initial: m
states:
  m:
    transitions:
      - to: e
        when:
          counter: drop_all
  e:
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        let err = def.validate().unwrap_err();
        assert!(err.to_string().contains("at least one of"));
    }

    #[test]
    fn test_counter_condition_validation_bad_aggregate() {
        let yaml = r#"
name: bad
kind: adaptive_policy
description: test
initial: m
states:
  m:
    transitions:
      - to: e
        when:
          counter: drop_all
          rate_above: 100.0
          aggregate: average
  e:
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        let err = def.validate().unwrap_err();
        assert!(err.to_string().contains("invalid aggregate mode"));
    }

    #[test]
    fn test_webhook_config_parsing() {
        let yaml = r#"
name: webhook-test
kind: adaptive_policy
description: test webhook
initial: alert
states:
  alert:
    action:
      alert:
        message: "DDoS detected"
        webhook:
          url: https://hooks.example.com/alerts
          bearer_token: secret-token
          timeout_seconds: 5
          max_retries: 3
          headers:
            X-Custom: value
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        def.validate().unwrap();

        let alert_state = &def.states["alert"];
        let action = alert_state.action.as_ref().unwrap();
        let alert = action.alert.as_ref().unwrap();
        assert_eq!(alert.message, "DDoS detected");
        let wh = alert.webhook.as_ref().unwrap();
        assert_eq!(wh.url, "https://hooks.example.com/alerts");
        assert_eq!(wh.bearer_token.as_deref(), Some("secret-token"));
        assert_eq!(wh.timeout_seconds, Some(5));
        assert_eq!(wh.max_retries, Some(3));
        assert_eq!(wh.headers.get("X-Custom").unwrap(), "value");
    }

    #[test]
    fn test_webhook_basic_auth_parsing() {
        let yaml = r#"
name: auth-test
kind: adaptive_policy
description: test
initial: alert
states:
  alert:
    action:
      alert:
        message: "test"
        webhook:
          url: https://hooks.example.com
          basic_auth:
            username: user
            password: pass
    terminal: true
"#;
        let def = FsmDefinition::from_yaml(yaml).unwrap();
        def.validate().unwrap();

        let alert = def.states["alert"]
            .action
            .as_ref()
            .unwrap()
            .alert
            .as_ref()
            .unwrap();
        let wh = alert.webhook.as_ref().unwrap();
        let auth = wh.basic_auth.as_ref().unwrap();
        assert_eq!(auth.username, "user");
        assert_eq!(auth.password, "pass");
    }
}
