//! FSM evaluation engine — runs as background task evaluating FSM instances.

use crate::config::ControllerConfig;
use crate::counter_cache::CounterSnapshotCache;
use crate::counter_rate::{self, AggregateMode};
use crate::deploy;
use crate::events::{EventBus, FsmEvent as DomainFsmEvent};
use crate::metrics as m;
use crate::storage::blocking;
use crate::webhook;
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
    counter_cache: Arc<CounterSnapshotCache>,
    event_bus: Option<EventBus>,
}

impl FsmEngine {
    pub fn new(
        storage: Arc<dyn Storage>,
        config: ControllerConfig,
        tls_config: Option<pacinet_core::tls::TlsConfig>,
        counter_cache: Arc<CounterSnapshotCache>,
    ) -> Self {
        Self {
            storage,
            config,
            tls_config,
            counter_cache,
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
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

    /// Public entry point for external evaluation (used by main.rs with leader check).
    pub async fn evaluate_all_public(&self) {
        self.evaluate_all().await;
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
            let definition =
                match blocking(&self.storage, move |s| s.get_fsm_definition(&def_name)).await {
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
                pacinet_core::PaciNetError::Fsm(FsmError::DefinitionNotFound(def_name.to_string()))
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

    /// Start an adaptive policy FSM instance with target nodes selected by label.
    pub async fn start_adaptive_instance(
        &self,
        def_name: &str,
        rules_yaml: Option<String>,
        compile_options: Option<FsmCompileOptions>,
        target_label_filter: &std::collections::HashMap<String, String>,
    ) -> Result<FsmInstance, pacinet_core::PaciNetError> {
        let name = def_name.to_string();
        let definition = blocking(&self.storage, move |s| s.get_fsm_definition(&name))
            .await
            .map_err(|e| pacinet_core::PaciNetError::Internal(e.to_string()))?
            .ok_or_else(|| {
                pacinet_core::PaciNetError::Fsm(FsmError::DefinitionNotFound(def_name.to_string()))
            })?;

        // Select target nodes by label
        let label_filter = target_label_filter.clone();
        let nodes = blocking(&self.storage, move |s| s.list_nodes(&label_filter))
            .await
            .map_err(|e| pacinet_core::PaciNetError::Internal(e.to_string()))?;

        let target_node_ids: Vec<String> = nodes.iter().map(|n| n.node_id.clone()).collect();
        if target_node_ids.is_empty() {
            return Err(pacinet_core::PaciNetError::Internal(
                "No nodes match the target label filter".to_string(),
            ));
        }

        let mut context = FsmContext::for_adaptive_policy(target_node_ids);
        context.rules_yaml = rules_yaml;
        context.compile_options = compile_options;

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
            target_nodes = instance.context.target_nodes.len(),
            "Adaptive policy FSM instance started"
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
                pacinet_core::PaciNetError::Fsm(FsmError::InstanceNotFound(instance_id.to_string()))
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

        let current_state_def =
            definition
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

        self.fire_transition(
            &mut instance,
            &to_state,
            TransitionTrigger::Manual,
            &definition,
        )
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
                pacinet_core::PaciNetError::Fsm(FsmError::InstanceNotFound(instance_id.to_string()))
            })?;

        if !instance.is_running() {
            return Err(pacinet_core::PaciNetError::Fsm(FsmError::AlreadyCompleted));
        }

        let def_name = instance.definition_name.clone();
        let inst_id = instance.instance_id.clone();
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

        // Emit instance completed event (cancelled)
        if let Some(ref bus) = self.event_bus {
            bus.emit_fsm(DomainFsmEvent::InstanceCompleted {
                instance_id: inst_id.clone(),
                definition_name: def_name,
                final_status: "cancelled".to_string(),
                timestamp: chrono::Utc::now(),
            });
        }

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
                let _ = blocking(&self.storage, move |s| s.update_fsm_instance(instance)).await;
                m::record_fsm_instance_status("completed");
            }
            return Ok(());
        }

        // Evaluate transitions in order
        let current_state = instance.current_state.clone();
        for (ti, transition) in current_state_def.transitions.iter().enumerate() {
            let should_fire = if let Some(ref condition) = transition.when {
                self.evaluate_condition(condition, &mut instance, &current_state, ti)
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
                let _ = blocking(&self.storage, move |s| s.update_fsm_instance(inst_clone)).await;

                m::record_fsm_transition();
                return Ok(());
            }
        }

        // If context was modified (e.g., counter_condition_first_true updated), persist
        let inst_clone = instance.clone();
        let _ = blocking(&self.storage, move |s| s.update_fsm_instance(inst_clone)).await;

        Ok(())
    }

    fn evaluate_condition(
        &self,
        condition: &ConditionDefinition,
        instance: &mut FsmInstance,
        current_state: &str,
        transition_idx: usize,
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
            ConditionDefinition::Counter(cc) => {
                self.evaluate_counter_condition(cc, instance, current_state, transition_idx)
            }
            ConditionDefinition::Compound(compound) => {
                if let Some(ref conditions) = compound.and {
                    return conditions.iter().all(|c: &ConditionDefinition| {
                        self.evaluate_condition(c, instance, current_state, transition_idx)
                    });
                }
                if let Some(ref conditions) = compound.or {
                    return conditions.iter().any(|c: &ConditionDefinition| {
                        self.evaluate_condition(c, instance, current_state, transition_idx)
                    });
                }
                if let Some(ref inner) = compound.not {
                    return !self.evaluate_condition(
                        inner,
                        instance,
                        current_state,
                        transition_idx,
                    );
                }
                false
            }
        }
    }

    /// Evaluate a counter condition against cached snapshots.
    fn evaluate_counter_condition(
        &self,
        cc: &CounterCondition,
        instance: &mut FsmInstance,
        current_state: &str,
        transition_idx: usize,
    ) -> bool {
        let condition_key = format!("{}:{}", current_state, transition_idx);
        let aggregate_mode = cc
            .aggregate
            .as_deref()
            .map(counter_rate::parse_aggregate_mode)
            .unwrap_or(AggregateMode::Any);
        let use_bytes = cc.field.as_deref() == Some("bytes");

        // Determine which nodes to check
        let nodes_to_check: Vec<String> = if instance.context.target_nodes.is_empty() {
            // Fall back to deployed nodes or all cached nodes
            if instance.context.deployed_nodes.is_empty() {
                self.counter_cache.node_ids()
            } else {
                instance.context.deployed_nodes.clone()
            }
        } else {
            instance.context.target_nodes.clone()
        };

        if nodes_to_check.is_empty() {
            m::record_counter_eval("no_nodes");
            return false;
        }

        // Check total_above (absolute value check)
        if let Some(threshold) = cc.total_above {
            let met = self.check_total_above(
                &cc.counter,
                threshold,
                use_bytes,
                &nodes_to_check,
                aggregate_mode,
            );
            if !met {
                instance
                    .context
                    .counter_condition_first_true
                    .remove(&condition_key);
                m::record_counter_eval("total_not_met");
                return false;
            }
        }

        // Check rate thresholds
        if cc.rate_above.is_some() || cc.rate_below.is_some() {
            let met = self.check_rate_threshold(
                &cc.counter,
                cc.rate_above,
                cc.rate_below,
                use_bytes,
                &nodes_to_check,
                aggregate_mode,
            );
            if !met {
                instance
                    .context
                    .counter_condition_first_true
                    .remove(&condition_key);
                m::record_counter_eval("rate_not_met");
                return false;
            }
        }

        // If we reach here, the threshold condition is met.
        // Now check for_duration if specified.
        if let Some(ref dur_str) = cc.for_duration {
            let required_duration = match parse_duration(dur_str) {
                Ok(d) => d,
                Err(_) => return false,
            };

            let now = chrono::Utc::now();
            let first_true = instance
                .context
                .counter_condition_first_true
                .entry(condition_key.clone())
                .or_insert(now);

            let elapsed = now - *first_true;
            let required =
                chrono::Duration::from_std(required_duration).unwrap_or(chrono::Duration::MAX);

            if elapsed < required {
                debug!(
                    instance_id = %instance.instance_id,
                    condition = %condition_key,
                    elapsed_secs = elapsed.num_seconds(),
                    required_secs = required.num_seconds(),
                    "Counter condition sustained, waiting for duration"
                );
                m::record_counter_eval("duration_waiting");
                return false;
            }

            m::record_counter_eval("duration_met");
        } else {
            m::record_counter_eval("met");
        }

        true
    }

    /// Check if total counter value exceeds threshold.
    fn check_total_above(
        &self,
        rule_name: &str,
        threshold: u64,
        use_bytes: bool,
        nodes: &[String],
        aggregate: AggregateMode,
    ) -> bool {
        match aggregate {
            AggregateMode::Any => nodes.iter().any(|nid| {
                self.counter_cache
                    .latest(nid)
                    .and_then(|s| counter_rate::get_counter_total(&s, rule_name))
                    .map(|(m, b)| {
                        let val = if use_bytes { b } else { m };
                        val > threshold
                    })
                    .unwrap_or(false)
            }),
            AggregateMode::All => nodes.iter().all(|nid| {
                self.counter_cache
                    .latest(nid)
                    .and_then(|s| counter_rate::get_counter_total(&s, rule_name))
                    .map(|(m, b)| {
                        let val = if use_bytes { b } else { m };
                        val > threshold
                    })
                    .unwrap_or(false)
            }),
            AggregateMode::Sum => {
                let total: u64 = nodes
                    .iter()
                    .filter_map(|nid| {
                        self.counter_cache
                            .latest(nid)
                            .and_then(|s| counter_rate::get_counter_total(&s, rule_name))
                            .map(|(m, b)| if use_bytes { b } else { m })
                    })
                    .sum();
                total > threshold
            }
        }
    }

    /// Check if rate meets the threshold conditions.
    fn check_rate_threshold(
        &self,
        rule_name: &str,
        rate_above: Option<f64>,
        rate_below: Option<f64>,
        use_bytes: bool,
        nodes: &[String],
        aggregate: AggregateMode,
    ) -> bool {
        // Collect per-node rates
        let rates: Vec<f64> = nodes
            .iter()
            .filter_map(|nid| {
                let (older, newer) = self.counter_cache.latest_pair(nid)?;
                let rate = counter_rate::calculate_rate(&older, &newer, rule_name)?;
                Some(if use_bytes {
                    rate.bytes_per_second
                } else {
                    rate.matches_per_second
                })
            })
            .collect();

        if rates.is_empty() {
            return false;
        }

        match aggregate {
            AggregateMode::Any => rates
                .iter()
                .any(|r| rate_matches(*r, rate_above, rate_below)),
            AggregateMode::All => rates
                .iter()
                .all(|r| rate_matches(*r, rate_above, rate_below)),
            AggregateMode::Sum => {
                let sum: f64 = rates.iter().sum();
                rate_matches(sum, rate_above, rate_below)
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
        let message = format!("{} -> {} ({})", instance.current_state, to_state, trigger);
        info!(
            instance_id = %instance.instance_id,
            from = %instance.current_state,
            to = %to_state,
            "FSM transition"
        );

        // Clear counter_condition_first_true entries for the old state
        let old_state = instance.current_state.clone();
        instance
            .context
            .counter_condition_first_true
            .retain(|k, _| !k.starts_with(&format!("{}:", old_state)));

        let from_state_for_event = instance.current_state.clone();
        let trigger_str = trigger.to_string();
        instance.transition(to_state.to_string(), trigger, message.clone());

        // Emit transition event
        if let Some(ref bus) = self.event_bus {
            bus.emit_fsm(DomainFsmEvent::Transition {
                instance_id: instance.instance_id.clone(),
                definition_name: instance.definition_name.clone(),
                from_state: from_state_for_event,
                to_state: to_state.to_string(),
                trigger: trigger_str,
                message,
                timestamp: chrono::Utc::now(),
            });
        }

        // Execute target state's action if any
        if let Some(state_def) = definition.states.get(to_state) {
            if let Some(ref action) = state_def.action {
                self.execute_action(action, instance).await;
            }

            // Check if terminal
            if state_def.terminal {
                instance.status = FsmInstanceStatus::Completed;
                m::record_fsm_instance_status("completed");

                // Emit instance completed event
                if let Some(ref bus) = self.event_bus {
                    bus.emit_fsm(DomainFsmEvent::InstanceCompleted {
                        instance_id: instance.instance_id.clone(),
                        definition_name: instance.definition_name.clone(),
                        final_status: "completed".to_string(),
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }
    }

    async fn execute_action(&self, action: &ActionDefinition, instance: &mut FsmInstance) {
        if let Some(ref deploy_action) = action.deploy {
            self.execute_deploy(deploy_action, instance).await;
        } else if let Some(ref rollback_action) = action.rollback {
            self.execute_rollback(rollback_action, instance).await;
        } else if let Some(ref alert_action) = action.alert {
            self.execute_alert(alert_action, instance);
        }
    }

    async fn execute_deploy(&self, deploy_action: &DeployAction, instance: &mut FsmInstance) {
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
                axi: o.axi,
                ports: o.ports,
                target: o.target.clone(),
                dynamic: o.dynamic,
                dynamic_entries: o.dynamic_entries,
                width: o.width,
                ptp: o.ptp,
                rss: o.rss,
                rss_queues: o.rss_queues,
                int_enabled: o.int,
                int_switch_id: o.int_switch_id,
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

        // Emit deploy progress event
        if let Some(ref bus) = self.event_bus {
            bus.emit_fsm(DomainFsmEvent::DeployProgress {
                instance_id: instance.instance_id.clone(),
                definition_name: instance.definition_name.clone(),
                deployed_nodes: instance.context.deployed_nodes.len() as u32,
                failed_nodes: instance.context.failed_nodes.len() as u32,
                target_nodes: instance.context.target_nodes.len() as u32,
                timestamp: chrono::Utc::now(),
            });
        }
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
            let versions =
                match blocking(&self.storage, move |s| s.get_policy_history(&nid, 2)).await {
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
                axi: prev.axi_enabled,
                ports: prev.ports,
                target: prev.target.clone(),
                dynamic: prev.dynamic,
                dynamic_entries: prev.dynamic_entries,
                width: prev.width,
                ptp: prev.ptp,
                rss: prev.rss,
                rss_queues: prev.rss_queues,
                int_enabled: prev.int,
                int_switch_id: prev.int_switch_id,
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
            "FSM alert"
        );

        // Deliver webhook if configured
        if let Some(ref wh_config) = alert_action.webhook {
            let payload = webhook::WebhookPayload {
                event: "fsm_alert".to_string(),
                instance_id: instance.instance_id.clone(),
                definition_name: instance.definition_name.clone(),
                current_state: instance.current_state.clone(),
                message: alert_action.message.clone(),
                timestamp: chrono::Utc::now(),
                deployed_nodes: instance.context.deployed_nodes.clone(),
            };
            let config = wh_config.clone();
            let storage = self.storage.clone();
            let iid = instance.instance_id.clone();
            tokio::spawn(async move {
                webhook::deliver_webhook(&config, &payload, Some(&storage), &iid).await;
            });
        }
    }
}

/// Check if a rate value matches the given threshold conditions.
fn rate_matches(rate: f64, above: Option<f64>, below: Option<f64>) -> bool {
    if let Some(threshold) = above {
        if rate <= threshold {
            return false;
        }
    }
    if let Some(threshold) = below {
        if rate >= threshold {
            return false;
        }
    }
    true
}
