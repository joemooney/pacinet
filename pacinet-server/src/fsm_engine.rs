//! FSM evaluation engine — runs as background task evaluating FSM instances.

use crate::config::ControllerConfig;
use crate::deploy;
use crate::metrics as m;
use crate::storage::blocking;
use pacinet_core::fsm::*;
use pacinet_core::Storage;
use pacinet_proto::CompileOptions;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::{debug, info, warn};

pub struct FsmEngine {
    storage: Arc<dyn Storage>,
    config: ControllerConfig,
    tls_config: Option<pacinet_core::tls::TlsConfig>,
}

impl FsmEngine {
    pub fn new(
        storage: Arc<dyn Storage>,
        config: ControllerConfig,
        tls_config: Option<pacinet_core::tls::TlsConfig>,
    ) -> Self {
        Self {
            storage,
            config,
            tls_config,
        }
    }

    /// Background loop: evaluate all Running instances every 5s.
    pub async fn run(&self, mut shutdown_rx: watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.evaluate_all().await;
                }
                _ = shutdown_rx.changed() => {
                    info!("FSM engine shutting down");
                    return;
                }
            }
        }
    }

    async fn evaluate_all(&self) {
        let instances = match blocking(&self.storage, |s| {
            s.list_fsm_instances(None, Some(FsmInstanceStatus::Running))
        })
        .await
        {
            Ok(instances) => instances,
            Err(e) => {
                warn!("FSM engine: failed to list running instances: {}", e);
                return;
            }
        };

        for instance in instances {
            let def_name = instance.definition_name.clone();
            let definition = match blocking(&self.storage, move |s| s.get_fsm_definition(&def_name))
                .await
            {
                Ok(Some(def)) => def,
                Ok(None) => {
                    warn!(
                        instance_id = %instance.instance_id,
                        "FSM definition '{}' not found, marking failed",
                        instance.definition_name
                    );
                    let mut inst = instance;
                    inst.status = FsmInstanceStatus::Failed;
                    let _ = blocking(&self.storage, move |s| s.update_fsm_instance(inst)).await;
                    continue;
                }
                Err(e) => {
                    warn!("FSM engine: failed to load definition: {}", e);
                    continue;
                }
            };

            if let Err(e) = self.evaluate_instance(instance, &definition).await {
                warn!("FSM engine: evaluation error: {}", e);
            }
        }

        // Update metrics
        m::update_fsm_running_gauge(self.count_running().await);
    }

    async fn count_running(&self) -> usize {
        blocking(&self.storage, |s| {
            s.list_fsm_instances(None, Some(FsmInstanceStatus::Running))
        })
        .await
        .map(|v| v.len())
        .unwrap_or(0)
    }

    /// Start a new FSM instance from a definition.
    pub async fn start_instance(
        &self,
        def_name: &str,
        rules_yaml: String,
        compile_options: Option<FsmCompileOptions>,
    ) -> Result<FsmInstance, pacinet_core::PaciNetError> {
        let name = def_name.to_string();
        let definition = blocking(&self.storage, move |s| s.get_fsm_definition(&name))
            .await
            .map_err(|e| pacinet_core::PaciNetError::Internal(e.to_string()))?
            .ok_or_else(|| {
                pacinet_core::PaciNetError::Fsm(FsmError::DefinitionNotFound(
                    def_name.to_string(),
                ))
            })?;

        let context = FsmContext::for_deployment(rules_yaml, compile_options);
        let mut instance =
            FsmInstance::new(def_name.to_string(), definition.initial.clone(), context);

        // Execute initial state's action if any
        if let Some(state_def) = definition.states.get(&definition.initial) {
            if let Some(ref action) = state_def.action {
                self.execute_action(action, &mut instance).await;
            }
        }

        let inst_clone = instance.clone();
        blocking(&self.storage, move |s| s.store_fsm_instance(inst_clone))
            .await
            .map_err(|e| pacinet_core::PaciNetError::Internal(e.to_string()))?;

        m::record_fsm_instance_status("started");
        info!(
            instance_id = %instance.instance_id,
            definition = %def_name,
            "FSM instance started"
        );

        Ok(instance)
    }

    /// Manually advance an instance (for `manual: true` conditions).
    pub async fn advance_instance(
        &self,
        instance_id: &str,
        target_state: Option<String>,
    ) -> Result<FsmInstance, pacinet_core::PaciNetError> {
        let id = instance_id.to_string();
        let mut instance = blocking(&self.storage, move |s| s.get_fsm_instance(&id))
            .await
            .map_err(|e| pacinet_core::PaciNetError::Internal(e.to_string()))?
            .ok_or_else(|| {
                pacinet_core::PaciNetError::Fsm(FsmError::InstanceNotFound(
                    instance_id.to_string(),
                ))
            })?;

        if !instance.is_running() {
            return Err(pacinet_core::PaciNetError::Fsm(FsmError::AlreadyCompleted));
        }

        let def_name = instance.definition_name.clone();
        let definition = blocking(&self.storage, move |s| s.get_fsm_definition(&def_name))
            .await
            .map_err(|e| pacinet_core::PaciNetError::Internal(e.to_string()))?
            .ok_or_else(|| {
                pacinet_core::PaciNetError::Fsm(FsmError::DefinitionNotFound(
                    instance.definition_name.clone(),
                ))
            })?;

        let current_state_def = definition
            .states
            .get(&instance.current_state)
            .ok_or_else(|| {
                pacinet_core::PaciNetError::Fsm(FsmError::InvalidState(
                    instance.current_state.clone(),
                ))
            })?;

        // Find target: explicit target_state or first transition with manual condition
        let to_state = if let Some(ref target) = target_state {
            // Verify target is a valid transition
            if !current_state_def
                .transitions
                .iter()
                .any(|t| t.to == *target)
            {
                return Err(pacinet_core::PaciNetError::Fsm(FsmError::NoTransition(
                    format!(
                        "no transition from '{}' to '{}'",
                        instance.current_state, target
                    ),
                )));
            }
            target.clone()
        } else {
            // Find first transition with manual condition or first transition
            current_state_def
                .transitions
                .iter()
                .find(|t| {
                    t.when.as_ref().is_some_and(|c| {
                        if let ConditionDefinition::Simple(s) = c {
                            s.manual == Some(true)
                        } else {
                            false
                        }
                    })
                })
                .or_else(|| current_state_def.transitions.first())
                .map(|t| t.to.clone())
                .ok_or_else(|| {
                    pacinet_core::PaciNetError::Fsm(FsmError::NoTransition(
                        instance.current_state.clone(),
                    ))
                })?
        };

        self.fire_transition(&mut instance, &to_state, TransitionTrigger::Manual, &definition)
            .await;

        let inst_clone = instance.clone();
        blocking(&self.storage, move |s| s.update_fsm_instance(inst_clone))
            .await
            .map_err(|e| pacinet_core::PaciNetError::Internal(e.to_string()))?;

        m::record_fsm_transition();
        Ok(instance)
    }

    /// Cancel a running instance.
    pub async fn cancel_instance(
        &self,
        instance_id: &str,
        reason: &str,
    ) -> Result<(), pacinet_core::PaciNetError> {
        let id = instance_id.to_string();
        let mut instance = blocking(&self.storage, move |s| s.get_fsm_instance(&id))
            .await
            .map_err(|e| pacinet_core::PaciNetError::Internal(e.to_string()))?
            .ok_or_else(|| {
                pacinet_core::PaciNetError::Fsm(FsmError::InstanceNotFound(
                    instance_id.to_string(),
                ))
            })?;

        if !instance.is_running() {
            return Err(pacinet_core::PaciNetError::Fsm(FsmError::AlreadyCompleted));
        }

        instance.status = FsmInstanceStatus::Cancelled;
        instance.updated_at = chrono::Utc::now();
        instance.history.push(FsmTransitionRecord {
            from_state: instance.current_state.clone(),
            to_state: String::new(),
            trigger: TransitionTrigger::Manual,
            timestamp: chrono::Utc::now(),
            message: format!("Cancelled: {}", reason),
        });

        blocking(&self.storage, move |s| s.update_fsm_instance(instance))
            .await
            .map_err(|e| pacinet_core::PaciNetError::Internal(e.to_string()))?;

        m::record_fsm_instance_status("cancelled");
        info!(instance_id = %instance_id, "FSM instance cancelled");
        Ok(())
    }

    // ---- Internal evaluation methods ----

    async fn evaluate_instance(
        &self,
        instance: FsmInstance,
        definition: &FsmDefinition,
    ) -> Result<(), String> {
        let mut instance = instance;

        let current_state_def = match definition.states.get(&instance.current_state) {
            Some(def) => def,
            None => {
                warn!(
                    "FSM instance {} in unknown state '{}'",
                    instance.instance_id, instance.current_state
                );
                return Ok(());
            }
        };

        // Terminal state — mark completed
        if current_state_def.terminal {
            if instance.is_running() {
                instance.status = FsmInstanceStatus::Completed;
                instance.updated_at = chrono::Utc::now();
                let _ =
                    blocking(&self.storage, move |s| s.update_fsm_instance(instance)).await;
                m::record_fsm_instance_status("completed");
            }
            return Ok(());
        }

        // Evaluate transitions in order
        for transition in &current_state_def.transitions {
            let should_fire = if let Some(ref condition) = transition.when {
                self.evaluate_condition(condition, &instance)
            } else if let Some(ref after) = transition.after {
                // Timer transition
                if let Ok(duration) = parse_duration(after) {
                    let elapsed = chrono::Utc::now() - instance.updated_at;
                    elapsed >= chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::MAX)
                } else {
                    false
                }
            } else {
                false
            };

            if should_fire {
                let trigger = if transition.after.is_some() {
                    TransitionTrigger::Timer
                } else {
                    TransitionTrigger::Condition
                };

                self.fire_transition(&mut instance, &transition.to, trigger, definition)
                    .await;

                let inst_clone = instance.clone();
                let _ = blocking(&self.storage, move |s| s.update_fsm_instance(inst_clone))
                    .await;

                m::record_fsm_transition();
                return Ok(());
            }
        }

        Ok(())
    }

    fn evaluate_condition(
        &self,
        condition: &ConditionDefinition,
        instance: &FsmInstance,
    ) -> bool {
        match condition {
            ConditionDefinition::Simple(simple) => {
                if simple.all_succeeded == Some(true) {
                    if let Some(ref result) = instance.context.last_action_result {
                        return result.failed == 0 && result.succeeded > 0;
                    }
                    return false;
                }
                if simple.any_failed == Some(true) {
                    if let Some(ref result) = instance.context.last_action_result {
                        return result.failed > 0;
                    }
                    return false;
                }
                // manual: true — only triggered by advance_instance, not by evaluation
                false
            }
            ConditionDefinition::Counter(_) => {
                // Deferred to Phase 5b
                false
            }
            ConditionDefinition::Compound(compound) => {
                if let Some(ref conditions) = compound.and {
                    return conditions
                        .iter()
                        .all(|c: &ConditionDefinition| self.evaluate_condition(c, instance));
                }
                if let Some(ref conditions) = compound.or {
                    return conditions
                        .iter()
                        .any(|c: &ConditionDefinition| self.evaluate_condition(c, instance));
                }
                if let Some(ref inner) = compound.not {
                    return !self.evaluate_condition(inner, instance);
                }
                false
            }
        }
    }

    async fn fire_transition(
        &self,
        instance: &mut FsmInstance,
        to_state: &str,
        trigger: TransitionTrigger,
        definition: &FsmDefinition,
    ) {
        let message = format!(
            "{} -> {} ({})",
            instance.current_state, to_state, trigger
        );
        info!(
            instance_id = %instance.instance_id,
            from = %instance.current_state,
            to = %to_state,
            "FSM transition"
        );

        instance.transition(to_state.to_string(), trigger, message);

        // Execute target state's action if any
        if let Some(state_def) = definition.states.get(to_state) {
            if let Some(ref action) = state_def.action {
                self.execute_action(action, instance).await;
            }

            // Check if terminal
            if state_def.terminal {
                instance.status = FsmInstanceStatus::Completed;
                m::record_fsm_instance_status("completed");
            }
        }
    }

    async fn execute_action(
        &self,
        action: &ActionDefinition,
        instance: &mut FsmInstance,
    ) {
        if let Some(ref deploy_action) = action.deploy {
            self.execute_deploy(deploy_action, instance).await;
        } else if let Some(ref rollback_action) = action.rollback {
            self.execute_rollback(rollback_action, instance).await;
        } else if let Some(ref alert_action) = action.alert {
            self.execute_alert(alert_action, instance);
        }
    }

    async fn execute_deploy(
        &self,
        deploy_action: &DeployAction,
        instance: &mut FsmInstance,
    ) {
        let rules_yaml = match &instance.context.rules_yaml {
            Some(yaml) => yaml.clone(),
            None => {
                warn!(
                    instance_id = %instance.instance_id,
                    "No rules_yaml in FSM context"
                );
                return;
            }
        };

        // Select nodes matching labels
        let label_filter = deploy_action.select.label.clone();
        let nodes = match blocking(&self.storage, move |s| s.list_nodes(&label_filter)).await {
            Ok(nodes) => nodes,
            Err(e) => {
                warn!("FSM deploy: failed to list nodes: {}", e);
                return;
            }
        };

        if nodes.is_empty() {
            debug!(instance_id = %instance.instance_id, "No nodes match selector");
            instance.context.last_action_result = Some(ActionResult {
                succeeded: 0,
                failed: 0,
                total: 0,
                node_results: vec![],
            });
            return;
        }

        // Apply limit
        let mut selected_nodes = nodes;
        if let Some(limit) = deploy_action.select.limit {
            selected_nodes.truncate(limit as usize);
        }

        // Apply batch_percent if set
        if let Some(batch_pct) = deploy_action.batch_percent {
            let total_remaining = selected_nodes.len();
            let batch_size = ((total_remaining as f64 * batch_pct as f64 / 100.0).ceil() as usize)
                .max(1)
                .min(total_remaining);
            let cursor = instance.context.batch_cursor as usize;
            if cursor >= total_remaining {
                // All batches done
                instance.context.last_action_result = Some(ActionResult {
                    succeeded: instance.context.deployed_nodes.len() as u32,
                    failed: instance.context.failed_nodes.len() as u32,
                    total: total_remaining as u32,
                    node_results: vec![],
                });
                return;
            }
            let end = (cursor + batch_size).min(total_remaining);
            selected_nodes = selected_nodes[cursor..end].to_vec();
            instance.context.batch_cursor = end as u32;
        }

        // Store target nodes
        for node in &selected_nodes {
            if !instance.context.target_nodes.contains(&node.node_id) {
                instance.context.target_nodes.push(node.node_id.clone());
            }
        }

        // Build compile options
        let compile_opts = instance
            .context
            .compile_options
            .as_ref()
            .map(|o| CompileOptions {
                counters: o.counters,
                rate_limit: o.rate_limit,
                conntrack: o.conntrack,
            })
            .unwrap_or_default();

        let result = deploy::deploy_to_nodes(
            &self.storage,
            selected_nodes,
            &rules_yaml,
            compile_opts,
            self.config.deploy_timeout,
            &self.tls_config,
        )
        .await;

        // Update context with results
        for nr in &result.node_results {
            if nr.success {
                if !instance.context.deployed_nodes.contains(&nr.node_id) {
                    instance.context.deployed_nodes.push(nr.node_id.clone());
                }
            } else if !instance.context.failed_nodes.contains(&nr.node_id) {
                instance.context.failed_nodes.push(nr.node_id.clone());
            }
        }
        instance.context.last_action_result = Some(result);
    }

    async fn execute_rollback(
        &self,
        _rollback_action: &RollbackAction,
        instance: &mut FsmInstance,
    ) {
        // Rollback all deployed nodes to their previous policy
        let deployed_nodes = instance.context.deployed_nodes.clone();
        if deployed_nodes.is_empty() {
            info!(instance_id = %instance.instance_id, "No deployed nodes to rollback");
            return;
        }

        let mut succeeded = 0u32;
        let mut failed = 0u32;
        let mut node_results = Vec::new();

        for node_id in &deployed_nodes {
            let nid = node_id.clone();
            let versions = match blocking(&self.storage, move |s| s.get_policy_history(&nid, 2))
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    warn!("FSM rollback: failed to get history for {}: {}", node_id, e);
                    failed += 1;
                    node_results.push(NodeActionResult {
                        node_id: node_id.clone(),
                        success: false,
                        message: format!("Failed to get policy history: {}", e),
                    });
                    continue;
                }
            };

            if versions.len() < 2 {
                warn!("FSM rollback: no previous version for node {}", node_id);
                failed += 1;
                node_results.push(NodeActionResult {
                    node_id: node_id.clone(),
                    success: false,
                    message: "No previous policy version".to_string(),
                });
                continue;
            }

            let prev = &versions[1];
            let nid = node_id.clone();
            let node = match blocking(&self.storage, move |s| s.get_node(&nid)).await {
                Ok(Some(n)) => n,
                _ => {
                    failed += 1;
                    node_results.push(NodeActionResult {
                        node_id: node_id.clone(),
                        success: false,
                        message: "Node not found".to_string(),
                    });
                    continue;
                }
            };

            let opts = CompileOptions {
                counters: prev.counters_enabled,
                rate_limit: prev.rate_limit_enabled,
                conntrack: prev.conntrack_enabled,
            };

            let outcome = deploy::deploy_to_node(
                &self.storage,
                &node,
                &prev.rules_yaml,
                opts,
                self.config.deploy_timeout,
                &self.tls_config,
            )
            .await;

            if outcome.success {
                succeeded += 1;
            } else {
                failed += 1;
            }
            node_results.push(NodeActionResult {
                node_id: node_id.clone(),
                success: outcome.success,
                message: outcome.message,
            });
        }

        instance.context.last_action_result = Some(ActionResult {
            succeeded,
            failed,
            total: deployed_nodes.len() as u32,
            node_results,
        });
    }

    fn execute_alert(&self, alert_action: &AlertAction, instance: &FsmInstance) {
        info!(
            instance_id = %instance.instance_id,
            channel = ?alert_action.channel,
            message = %alert_action.message,
            "FSM alert (log-only)"
        );
    }
}
