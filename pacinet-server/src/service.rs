use crate::config::ControllerConfig;
use crate::counter_cache::CounterSnapshotCache;
use crate::counter_rate;
use crate::events::{
    CounterEvent, CounterRateData, EventBus, FsmEvent as DomainFsmEvent, NodeEvent,
};
use crate::fsm_engine::FsmEngine;
use crate::metrics as m;
use crate::storage::blocking;
use pacinet_core::model::{Node, Policy};
use pacinet_core::{CounterSnapshot, Storage};
use pacinet_proto::*;
use std::pin::Pin;
use std::sync::Arc;
use tokio::task::JoinSet;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

// ============================================================================
// PaciNetController service — agent → controller
// ============================================================================

pub struct ControllerService {
    storage: Arc<dyn Storage>,
    counter_cache: Option<Arc<CounterSnapshotCache>>,
    event_bus: Option<EventBus>,
}

impl ControllerService {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            counter_cache: None,
            event_bus: None,
        }
    }

    pub fn with_counter_cache(mut self, cache: Arc<CounterSnapshotCache>) -> Self {
        self.counter_cache = Some(cache);
        self
    }

    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }
}

#[tonic::async_trait]
impl paci_net_controller_server::PaciNetController for ControllerService {
    #[tracing::instrument(skip(self, request), fields(hostname))]
    async fn register_node(
        &self,
        request: Request<RegisterNodeRequest>,
    ) -> Result<Response<RegisterNodeResponse>, Status> {
        let req = request.into_inner();
        info!(
            hostname = %req.hostname,
            agent_address = %req.agent_address,
            "Node registration request"
        );

        let hostname = req.hostname.clone();
        let labels = req.labels.clone();
        let capabilities = req.capabilities.clone();
        let mut node = Node::new(
            req.hostname,
            req.agent_address,
            req.labels,
            req.pacgate_version,
        );
        node.capabilities = capabilities;
        let node_id = blocking(&self.storage, move |s| s.register_node(node)).await?;

        info!(node_id = %node_id, "Node registered successfully");

        if let Some(ref bus) = self.event_bus {
            bus.emit_node(NodeEvent::Registered {
                node_id: node_id.clone(),
                hostname,
                labels,
                timestamp: chrono::Utc::now(),
            });
        }

        Ok(Response::new(RegisterNodeResponse {
            node_id,
            accepted: true,
            message: "Node registered".to_string(),
        }))
    }

    #[tracing::instrument(skip(self, request), level = "debug")]
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        let new_state = pacinet_core::NodeState::from(
            NodeState::try_from(req.state).unwrap_or(NodeState::Online),
        );
        let uptime = req.uptime_seconds;
        let node_id = req.node_id.clone();
        let node_id_log = req.node_id.clone();

        // Fetch node before update to detect state changes
        let old_node = if self.event_bus.is_some() {
            let nid = req.node_id.clone();
            blocking(&self.storage, move |s| s.get_node(&nid))
                .await
                .ok()
                .flatten()
        } else {
            None
        };

        let state_for_update = new_state.clone();
        let found = blocking(&self.storage, move |s| {
            s.update_heartbeat(&node_id, state_for_update, uptime)
        })
        .await?;

        if !found {
            warn!(node_id = %node_id_log, "Heartbeat from unknown node");
            return Err(Status::not_found("Node not registered"));
        }

        // Emit state change event if state differs
        if let (Some(ref bus), Some(ref node)) = (&self.event_bus, &old_node) {
            if node.state != new_state {
                bus.emit_node(NodeEvent::StateChanged {
                    node_id: node.node_id.clone(),
                    hostname: node.hostname.clone(),
                    labels: node.labels.clone(),
                    old_state: node.state.to_string(),
                    new_state: new_state.to_string(),
                    timestamp: chrono::Utc::now(),
                });
            }
        }

        m::record_heartbeat();
        Ok(Response::new(HeartbeatResponse { acknowledged: true }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn report_counters(
        &self,
        request: Request<ReportCountersRequest>,
    ) -> Result<Response<ReportCountersResponse>, Status> {
        let req = request.into_inner();
        let ReportCountersRequest {
            node_id,
            counters,
            collected_at,
            flow_counters,
        } = req;
        let counters: Vec<pacinet_core::RuleCounter> =
            counters.into_iter().map(|c| c.into()).collect();
        let flow_counters: Vec<pacinet_core::FlowCounter> =
            flow_counters.into_iter().map(|c| c.into()).collect();

        // Record snapshot in counter cache for rate tracking
        if let Some(ref cache) = self.counter_cache {
            let collected_at = collected_at
                .as_ref()
                .and_then(|t| chrono::DateTime::from_timestamp(t.seconds, 0))
                .unwrap_or_else(chrono::Utc::now);

            let snapshot = CounterSnapshot {
                node_id: node_id.clone(),
                collected_at,
                counters: counters.clone(),
            };
            cache.record(snapshot);
            m::record_counter_snapshot();

            // Emit counter event with rates
            if let Some(ref bus) = self.event_bus {
                let rate_data: Vec<CounterRateData> = if let Some((older, newer)) =
                    cache.latest_pair(&node_id)
                {
                    counters
                        .iter()
                        .map(|c| {
                            let rate = counter_rate::calculate_rate(&older, &newer, &c.rule_name);
                            CounterRateData {
                                rule_name: c.rule_name.clone(),
                                match_count: c.match_count,
                                byte_count: c.byte_count,
                                matches_per_second: rate
                                    .as_ref()
                                    .map(|r| r.matches_per_second)
                                    .unwrap_or(0.0),
                                bytes_per_second: rate
                                    .as_ref()
                                    .map(|r| r.bytes_per_second)
                                    .unwrap_or(0.0),
                            }
                        })
                        .collect()
                } else {
                    counters
                        .iter()
                        .map(|c| CounterRateData {
                            rule_name: c.rule_name.clone(),
                            match_count: c.match_count,
                            byte_count: c.byte_count,
                            matches_per_second: 0.0,
                            bytes_per_second: 0.0,
                        })
                        .collect()
                };

                bus.emit_counter(CounterEvent {
                    node_id: node_id.clone(),
                    counters: rate_data,
                    collected_at,
                });
            }
        }

        let counters_node_id = node_id.clone();
        blocking(&self.storage, move |s| {
            s.store_counters(&counters_node_id, counters)
        })
        .await?;
        let flow_node_id = node_id.clone();
        blocking(&self.storage, move |s| {
            s.store_flow_counters(&flow_node_id, flow_counters)
        })
        .await?;

        Ok(Response::new(ReportCountersResponse { acknowledged: true }))
    }
}

// ============================================================================
// PaciNetManagement service — CLI → controller
// ============================================================================

pub struct ManagementService {
    storage: Arc<dyn Storage>,
    config: ControllerConfig,
    tls_config: Option<pacinet_core::tls::TlsConfig>,
    fsm_engine: Option<Arc<FsmEngine>>,
    event_bus: Option<EventBus>,
}

impl ManagementService {
    pub fn new(storage: Arc<dyn Storage>, config: ControllerConfig) -> Self {
        Self {
            storage,
            config,
            tls_config: None,
            fsm_engine: None,
            event_bus: None,
        }
    }

    pub fn with_tls(mut self, tls_config: Option<pacinet_core::tls::TlsConfig>) -> Self {
        self.tls_config = tls_config;
        self
    }

    pub fn with_fsm_engine(mut self, engine: Arc<FsmEngine>) -> Self {
        self.fsm_engine = Some(engine);
        self
    }

    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }
}

fn node_to_proto(node: &Node, policy: Option<&Policy>) -> NodeInfo {
    let now = chrono::Utc::now();
    let heartbeat_age = (now - node.last_heartbeat).num_milliseconds() as f64 / 1000.0;
    NodeInfo {
        node_id: node.node_id.clone(),
        hostname: node.hostname.clone(),
        agent_address: node.agent_address.clone(),
        labels: node.labels.clone(),
        capabilities: node.capabilities.clone(),
        state: pacinet_proto::NodeState::from(node.state.clone()) as i32,
        registered_at: Some(prost_types::Timestamp {
            seconds: node.registered_at.timestamp(),
            nanos: 0,
        }),
        last_heartbeat: Some(prost_types::Timestamp {
            seconds: node.last_heartbeat.timestamp(),
            nanos: 0,
        }),
        pacgate_version: node.pacgate_version.clone(),
        policy_hash: policy.map(|p| p.policy_hash.clone()).unwrap_or_default(),
        uptime_seconds: node.uptime_seconds,
        last_heartbeat_age_seconds: heartbeat_age,
        annotations: node.annotations.clone(),
    }
}

#[tonic::async_trait]
impl paci_net_management_server::PaciNetManagement for ManagementService {
    #[tracing::instrument(skip(self, request))]
    async fn list_nodes(
        &self,
        request: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        let req = request.into_inner();
        let label_filter = req.label_filter.clone();
        let nodes = blocking(&self.storage, move |s| s.list_nodes(&label_filter)).await?;

        // Batch fetch policies for enrichment
        let node_ids: Vec<String> = nodes.iter().map(|n| n.node_id.clone()).collect();
        let policies =
            blocking(&self.storage, move |s| s.get_policies_for_nodes(&node_ids)).await?;

        let proto_nodes: Vec<NodeInfo> = nodes
            .iter()
            .map(|n| node_to_proto(n, policies.get(&n.node_id)))
            .collect();

        Ok(Response::new(ListNodesResponse { nodes: proto_nodes }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_node(
        &self,
        request: Request<GetNodeRequest>,
    ) -> Result<Response<GetNodeResponse>, Status> {
        let req = request.into_inner();
        let node_id = req.node_id.clone();
        let node_id2 = req.node_id.clone();
        let node = blocking(&self.storage, move |s| s.get_node(&node_id))
            .await?
            .ok_or_else(|| Status::not_found(format!("Node {} not found", req.node_id)))?;

        let policy = blocking(&self.storage, move |s| s.get_policy(&node_id2)).await?;

        Ok(Response::new(GetNodeResponse {
            node: Some(node_to_proto(&node, policy.as_ref())),
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn remove_node(
        &self,
        request: Request<RemoveNodeRequest>,
    ) -> Result<Response<RemoveNodeResponse>, Status> {
        let req = request.into_inner();

        // Fetch node before removal for event emission
        let node_before = if self.event_bus.is_some() {
            let nid = req.node_id.clone();
            blocking(&self.storage, move |s| s.get_node(&nid))
                .await
                .ok()
                .flatten()
        } else {
            None
        };

        let node_id = req.node_id.clone();
        let removed = blocking(&self.storage, move |s| s.remove_node(&node_id)).await?;

        if removed {
            info!(node_id = %req.node_id, "Node removed");
            if let (Some(ref bus), Some(ref node)) = (&self.event_bus, &node_before) {
                bus.emit_node(NodeEvent::Removed {
                    node_id: node.node_id.clone(),
                    hostname: node.hostname.clone(),
                    labels: node.labels.clone(),
                    timestamp: chrono::Utc::now(),
                });
            }
        }

        Ok(Response::new(RemoveNodeResponse {
            success: removed,
            message: if removed {
                "Node removed".to_string()
            } else {
                "Node not found".to_string()
            },
        }))
    }

    #[tracing::instrument(skip(self, request), fields(node_id))]
    async fn deploy_policy(
        &self,
        request: Request<DeployPolicyRequest>,
    ) -> Result<Response<DeployPolicyResponse>, Status> {
        let req = request.into_inner();

        // Verify node exists
        let node_id = req.node_id.clone();
        let node = blocking(&self.storage, move |s| s.get_node(&node_id))
            .await?
            .ok_or_else(|| Status::not_found(format!("Node {} not found", req.node_id)))?;

        // Acquire deploy guard
        let node_id = req.node_id.clone();
        blocking(&self.storage, move |s| s.begin_deploy(&node_id)).await?;

        let result = self.do_deploy(&req, &node).await;

        // Release deploy guard
        let node_id = req.node_id.clone();
        self.storage.end_deploy(&node_id);

        result
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_policy(
        &self,
        request: Request<GetPolicyRequest>,
    ) -> Result<Response<GetPolicyResponse>, Status> {
        let req = request.into_inner();
        let node_id = req.node_id.clone();
        let policy = blocking(&self.storage, move |s| s.get_policy(&node_id))
            .await?
            .ok_or_else(|| Status::not_found(format!("No policy for node {}", req.node_id)))?;

        Ok(Response::new(GetPolicyResponse {
            node_id: policy.node_id,
            rules_yaml: policy.rules_yaml,
            policy_hash: policy.policy_hash,
            deployed_at: Some(prost_types::Timestamp {
                seconds: policy.deployed_at.timestamp(),
                nanos: 0,
            }),
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_node_counters(
        &self,
        request: Request<GetNodeCountersRequest>,
    ) -> Result<Response<GetNodeCountersResponse>, Status> {
        let req = request.into_inner();
        let node_id = req.node_id.clone();
        let counters = blocking(&self.storage, move |s| s.get_counters(&node_id))
            .await?
            .unwrap_or_default();
        let proto_counters: Vec<pacinet_proto::RuleCounter> =
            counters.into_iter().map(|c| c.into()).collect();

        Ok(Response::new(GetNodeCountersResponse {
            node_id: req.node_id,
            counters: proto_counters,
            collected_at: Some(prost_types::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_aggregate_counters(
        &self,
        request: Request<GetAggregateCountersRequest>,
    ) -> Result<Response<GetAggregateCountersResponse>, Status> {
        let req = request.into_inner();
        let label_filter = req.label_filter.clone();
        let nodes = blocking(&self.storage, move |s| s.list_nodes(&label_filter)).await?;

        let mut node_counters = Vec::new();
        for node in &nodes {
            let nid = node.node_id.clone();
            if let Some(counters) = blocking(&self.storage, move |s| s.get_counters(&nid)).await? {
                let proto_counters: Vec<pacinet_proto::RuleCounter> =
                    counters.into_iter().map(|c| c.into()).collect();
                node_counters.push(NodeCounterSet {
                    node_id: node.node_id.clone(),
                    counters: proto_counters,
                    collected_at: Some(prost_types::Timestamp {
                        seconds: chrono::Utc::now().timestamp(),
                        nanos: 0,
                    }),
                });
            }
        }

        Ok(Response::new(GetAggregateCountersResponse {
            node_counters,
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn batch_deploy_policy(
        &self,
        request: Request<BatchDeployPolicyRequest>,
    ) -> Result<Response<BatchDeployPolicyResponse>, Status> {
        let req = request.into_inner();
        let label_filter = req.label_filter.clone();
        let nodes = blocking(&self.storage, move |s| s.list_nodes(&label_filter)).await?;

        let total_nodes = nodes.len() as u32;
        if total_nodes == 0 {
            return Ok(Response::new(BatchDeployPolicyResponse {
                total_nodes: 0,
                succeeded: 0,
                failed: 0,
                results: vec![],
            }));
        }

        let mut join_set = JoinSet::new();

        for node in nodes {
            let storage = self.storage.clone();
            let rules_yaml = req.rules_yaml.clone();
            let options = req.options.clone();
            let deploy_timeout = self.config.deploy_timeout;
            let tls = self.tls_config.clone();

            join_set.spawn(async move {
                let deploy_req = DeployPolicyRequest {
                    node_id: node.node_id.clone(),
                    rules_yaml,
                    options,
                    dry_run: false,
                };

                // Try deploy with per-node guard and timeout
                let result =
                    deploy_single_node(&storage, &deploy_req, &node, deploy_timeout, &tls).await;
                let (success, message, warnings) = match result {
                    Ok(resp) => (resp.success, resp.message, resp.warnings),
                    Err(status) => (false, status.message().to_string(), vec![]),
                };

                NodeDeployResult {
                    node_id: node.node_id.clone(),
                    hostname: node.hostname.clone(),
                    success,
                    message,
                    warnings,
                }
            });
        }

        let mut results = Vec::new();
        while let Some(res) = join_set.join_next().await {
            match res {
                Ok(r) => results.push(r),
                Err(e) => {
                    warn!("Batch deploy task panicked: {}", e);
                }
            }
        }

        let succeeded = results.iter().filter(|r| r.success).count() as u32;
        let failed = total_nodes - succeeded;

        m::record_batch_deploy(succeeded, failed);

        Ok(Response::new(BatchDeployPolicyResponse {
            total_nodes,
            succeeded,
            failed,
            results,
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_fleet_status(
        &self,
        request: Request<GetFleetStatusRequest>,
    ) -> Result<Response<GetFleetStatusResponse>, Status> {
        let req = request.into_inner();
        let label_filter = req.label_filter.clone();
        let nodes = blocking(&self.storage, move |s| s.list_nodes(&label_filter)).await?;

        let node_ids: Vec<String> = nodes.iter().map(|n| n.node_id.clone()).collect();
        let policies =
            blocking(&self.storage, move |s| s.get_policies_for_nodes(&node_ids)).await?;

        let total_nodes = nodes.len() as u32;
        let mut nodes_by_state: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        let mut summaries = Vec::new();

        let now = chrono::Utc::now();
        for node in &nodes {
            *nodes_by_state.entry(node.state.to_string()).or_insert(0) += 1;
            let policy = policies.get(&node.node_id);
            let heartbeat_age = (now - node.last_heartbeat).num_milliseconds() as f64 / 1000.0;
            summaries.push(FleetNodeSummary {
                node_id: node.node_id.clone(),
                hostname: node.hostname.clone(),
                state: pacinet_proto::NodeState::from(node.state.clone()) as i32,
                policy_hash: policy.map(|p| p.policy_hash.clone()).unwrap_or_default(),
                uptime_seconds: node.uptime_seconds,
                last_heartbeat_age_seconds: heartbeat_age,
                last_deploy_time: policy.map(|p| prost_types::Timestamp {
                    seconds: p.deployed_at.timestamp(),
                    nanos: 0,
                }),
            });
        }

        Ok(Response::new(GetFleetStatusResponse {
            total_nodes,
            nodes_by_state,
            nodes: summaries,
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_policy_history(
        &self,
        request: Request<GetPolicyHistoryRequest>,
    ) -> Result<Response<GetPolicyHistoryResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit == 0 { 10 } else { req.limit };
        let node_id = req.node_id.clone();
        let versions = blocking(&self.storage, move |s| {
            s.get_policy_history(&node_id, limit)
        })
        .await?;

        let proto_versions: Vec<PolicyVersionInfo> = versions
            .into_iter()
            .map(|v| PolicyVersionInfo {
                version: v.version,
                node_id: v.node_id,
                policy_hash: v.policy_hash,
                deployed_at: Some(prost_types::Timestamp {
                    seconds: v.deployed_at.timestamp(),
                    nanos: 0,
                }),
                rules_yaml: v.rules_yaml,
            })
            .collect();

        Ok(Response::new(GetPolicyHistoryResponse {
            versions: proto_versions,
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_deployment_history(
        &self,
        request: Request<GetDeploymentHistoryRequest>,
    ) -> Result<Response<GetDeploymentHistoryResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit == 0 { 20 } else { req.limit };
        let node_id = req.node_id.clone();
        let records = blocking(&self.storage, move |s| s.get_deployments(&node_id, limit)).await?;

        let proto_deployments: Vec<DeploymentInfo> = records
            .into_iter()
            .map(|r| DeploymentInfo {
                id: r.id,
                node_id: r.node_id,
                policy_version: r.policy_version,
                policy_hash: r.policy_hash,
                deployed_at: Some(prost_types::Timestamp {
                    seconds: r.deployed_at.timestamp(),
                    nanos: 0,
                }),
                result: r.result.to_string(),
                message: r.message,
            })
            .collect();

        Ok(Response::new(GetDeploymentHistoryResponse {
            deployments: proto_deployments,
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn rollback_policy(
        &self,
        request: Request<RollbackPolicyRequest>,
    ) -> Result<Response<RollbackPolicyResponse>, Status> {
        let req = request.into_inner();
        let node_id = req.node_id.clone();

        // Verify node exists
        let node_id_check = req.node_id.clone();
        let node = blocking(&self.storage, move |s| s.get_node(&node_id_check))
            .await?
            .ok_or_else(|| Status::not_found(format!("Node {} not found", req.node_id)))?;

        // Get policy history
        let node_id_hist = req.node_id.clone();
        let versions = blocking(&self.storage, move |s| {
            s.get_policy_history(&node_id_hist, 10)
        })
        .await?;

        if versions.is_empty() {
            return Ok(Response::new(RollbackPolicyResponse {
                success: false,
                message: "No policy history available".to_string(),
                rolled_back_to_version: 0,
            }));
        }

        // Find target version
        let target = if req.target_version == 0 {
            // Rollback to previous = second entry (first is current)
            if versions.len() < 2 {
                return Ok(Response::new(RollbackPolicyResponse {
                    success: false,
                    message: "No previous version to rollback to".to_string(),
                    rolled_back_to_version: 0,
                }));
            }
            &versions[1]
        } else {
            versions
                .iter()
                .find(|v| v.version == req.target_version)
                .ok_or_else(|| {
                    Status::not_found(format!("Policy version {} not found", req.target_version))
                })?
        };

        let target_version = target.version;
        let rules_yaml = target.rules_yaml.clone();
        let options = Some(CompileOptions {
            counters: target.counters_enabled,
            rate_limit: target.rate_limit_enabled,
            conntrack: target.conntrack_enabled,
            axi: target.axi_enabled,
            ports: target.ports,
            target: target.target.clone(),
            dynamic: target.dynamic,
            dynamic_entries: target.dynamic_entries,
            width: target.width,
            ptp: target.ptp,
            rss: target.rss,
            rss_queues: target.rss_queues,
            int_enabled: target.int,
            int_switch_id: target.int_switch_id,
        });

        // Deploy the rollback via existing deploy flow
        let deploy_req = DeployPolicyRequest {
            node_id: node_id.clone(),
            rules_yaml,
            options,
            dry_run: false,
        };

        // Acquire deploy guard
        let nid_guard = node_id.clone();
        blocking(&self.storage, move |s| s.begin_deploy(&nid_guard)).await?;

        let result = self.do_deploy(&deploy_req, &node).await;

        // Release deploy guard
        self.storage.end_deploy(&node_id);

        match result {
            Ok(resp) => {
                let inner = resp.into_inner();
                let no_change = inner.message.starts_with("No change:");
                Ok(Response::new(RollbackPolicyResponse {
                    success: inner.success,
                    message: if inner.success {
                        if no_change {
                            inner.message
                        } else {
                            format!("Rolled back to version {}", target_version)
                        }
                    } else {
                        inner.message
                    },
                    rolled_back_to_version: if inner.success {
                        if no_change {
                            versions.first().map(|v| v.version).unwrap_or(target_version)
                        } else {
                            target_version
                        }
                    } else {
                        0
                    },
                }))
            }
            Err(status) => Ok(Response::new(RollbackPolicyResponse {
                success: false,
                message: status.message().to_string(),
                rolled_back_to_version: 0,
            })),
        }
    }

    // ---- Phase 6: Streaming RPCs ----

    type WatchFsmEventsStream =
        Pin<Box<dyn Stream<Item = Result<FsmEvent, Status>> + Send + 'static>>;
    type WatchCountersStream =
        Pin<Box<dyn Stream<Item = Result<CounterUpdate, Status>> + Send + 'static>>;
    type WatchNodeEventsStream =
        Pin<Box<dyn Stream<Item = Result<pacinet_proto::NodeEvent, Status>> + Send + 'static>>;

    async fn watch_fsm_events(
        &self,
        request: Request<WatchFsmEventsRequest>,
    ) -> Result<Response<Self::WatchFsmEventsStream>, Status> {
        let req = request.into_inner();
        let filter_instance = if req.instance_id.is_empty() {
            None
        } else {
            Some(req.instance_id)
        };

        let bus = self
            .event_bus
            .as_ref()
            .ok_or_else(|| Status::unavailable("Event streaming not available"))?;

        let mut rx = bus.fsm_tx.subscribe();

        let stream = async_stream::try_stream! {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Some(ref id) = filter_instance {
                            if event.instance_id() != id {
                                continue;
                            }
                        }
                        yield domain_fsm_to_proto(&event);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "FSM event stream lagged");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    async fn watch_counters(
        &self,
        request: Request<WatchCountersRequest>,
    ) -> Result<Response<Self::WatchCountersStream>, Status> {
        let req = request.into_inner();
        let filter_node = if req.node_id.is_empty() {
            None
        } else {
            Some(req.node_id)
        };

        let bus = self
            .event_bus
            .as_ref()
            .ok_or_else(|| Status::unavailable("Event streaming not available"))?;

        let mut rx = bus.counter_tx.subscribe();

        let stream = async_stream::try_stream! {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Some(ref id) = filter_node {
                            if event.node_id != *id {
                                continue;
                            }
                        }
                        yield domain_counter_to_proto(&event);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "Counter event stream lagged");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    async fn watch_node_events(
        &self,
        request: Request<WatchNodeEventsRequest>,
    ) -> Result<Response<Self::WatchNodeEventsStream>, Status> {
        let req = request.into_inner();
        let label_filter = if req.label_filter.is_empty() {
            None
        } else {
            Some(req.label_filter)
        };

        let bus = self
            .event_bus
            .as_ref()
            .ok_or_else(|| Status::unavailable("Event streaming not available"))?;

        let mut rx = bus.node_tx.subscribe();

        let stream = async_stream::try_stream! {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Some(ref filter) = label_filter {
                            let event_labels = event.labels();
                            let matches = filter.iter().all(|(k, v)| {
                                event_labels.get(k).map(|ev| ev == v).unwrap_or(false)
                            });
                            if !matches {
                                continue;
                            }
                        }
                        yield domain_node_to_proto(&event);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "Node event stream lagged");
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    // ---- Phase 5: FSM RPCs ----

    #[tracing::instrument(skip(self, request))]
    async fn create_fsm_definition(
        &self,
        request: Request<CreateFsmDefinitionRequest>,
    ) -> Result<Response<CreateFsmDefinitionResponse>, Status> {
        let req = request.into_inner();
        let def = pacinet_core::fsm::FsmDefinition::from_yaml(&req.definition_yaml)
            .map_err(|e| Status::invalid_argument(format!("Invalid YAML: {}", e)))?;
        def.validate()
            .map_err(|e| Status::invalid_argument(format!("Invalid definition: {}", e)))?;
        let name = def.name.clone();

        blocking(&self.storage, move |s| s.store_fsm_definition(def)).await?;

        info!(name = %name, "FSM definition created");
        Ok(Response::new(CreateFsmDefinitionResponse {
            success: true,
            name: name.clone(),
            message: format!("FSM definition '{}' created", name),
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_fsm_definition(
        &self,
        request: Request<GetFsmDefinitionRequest>,
    ) -> Result<Response<GetFsmDefinitionResponse>, Status> {
        let req = request.into_inner();
        let name = req.name.clone();
        let def = blocking(&self.storage, move |s| s.get_fsm_definition(&name))
            .await?
            .ok_or_else(|| Status::not_found(format!("FSM definition '{}' not found", req.name)))?;

        let yaml = serde_yaml::to_string(&def)
            .map_err(|e| Status::internal(format!("Failed to serialize: {}", e)))?;

        Ok(Response::new(GetFsmDefinitionResponse {
            name: def.name,
            kind: def.kind.to_string(),
            description: def.description,
            definition_yaml: yaml,
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn list_fsm_definitions(
        &self,
        request: Request<ListFsmDefinitionsRequest>,
    ) -> Result<Response<ListFsmDefinitionsResponse>, Status> {
        let req = request.into_inner();
        let kind = if req.kind.is_empty() {
            None
        } else {
            Some(
                req.kind
                    .parse::<pacinet_core::FsmKind>()
                    .map_err(Status::invalid_argument)?,
            )
        };

        let defs = blocking(&self.storage, move |s| s.list_fsm_definitions(kind)).await?;

        let summaries = defs
            .into_iter()
            .map(|d| FsmDefinitionSummary {
                name: d.name,
                kind: d.kind.to_string(),
                description: d.description.clone(),
                state_count: d.states.len() as u32,
                initial_state: d.initial,
            })
            .collect();

        Ok(Response::new(ListFsmDefinitionsResponse {
            definitions: summaries,
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn delete_fsm_definition(
        &self,
        request: Request<DeleteFsmDefinitionRequest>,
    ) -> Result<Response<DeleteFsmDefinitionResponse>, Status> {
        let req = request.into_inner();
        let name = req.name.clone();
        let deleted = blocking(&self.storage, move |s| s.delete_fsm_definition(&name)).await?;

        Ok(Response::new(DeleteFsmDefinitionResponse {
            success: deleted,
            message: if deleted {
                format!("FSM definition '{}' deleted", req.name)
            } else {
                format!("FSM definition '{}' not found", req.name)
            },
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn start_fsm(
        &self,
        request: Request<StartFsmRequest>,
    ) -> Result<Response<StartFsmResponse>, Status> {
        let req = request.into_inner();
        let engine = self
            .fsm_engine
            .as_ref()
            .ok_or_else(|| Status::internal("FSM engine not available"))?;

        let compile_opts = req.options.map(|o| pacinet_core::fsm::FsmCompileOptions {
            counters: o.counters,
            rate_limit: o.rate_limit,
            conntrack: o.conntrack,
            axi: o.axi,
            ports: o.ports,
            target: o.target,
            dynamic: o.dynamic,
            dynamic_entries: o.dynamic_entries,
            width: o.width,
            ptp: o.ptp,
            rss: o.rss,
            rss_queues: o.rss_queues,
            int: o.int_enabled,
            int_switch_id: o.int_switch_id,
        });

        // If target_label_filter is provided, start as adaptive policy FSM
        let result = if !req.target_label_filter.is_empty() {
            let rules = if req.rules_yaml.is_empty() {
                None
            } else {
                Some(req.rules_yaml)
            };
            engine
                .start_adaptive_instance(
                    &req.definition_name,
                    rules,
                    compile_opts,
                    &req.target_label_filter,
                )
                .await
        } else {
            engine
                .start_instance(&req.definition_name, req.rules_yaml, compile_opts)
                .await
        };

        match result {
            Ok(instance) => Ok(Response::new(StartFsmResponse {
                success: true,
                instance_id: instance.instance_id,
                message: "FSM instance started".to_string(),
            })),
            Err(e) => Ok(Response::new(StartFsmResponse {
                success: false,
                instance_id: String::new(),
                message: e.to_string(),
            })),
        }
    }

    #[tracing::instrument(skip(self, request))]
    async fn get_fsm_instance(
        &self,
        request: Request<GetFsmInstanceRequest>,
    ) -> Result<Response<GetFsmInstanceResponse>, Status> {
        let req = request.into_inner();
        let id = req.instance_id.clone();
        let instance = blocking(&self.storage, move |s| s.get_fsm_instance(&id))
            .await?
            .ok_or_else(|| {
                Status::not_found(format!("FSM instance '{}' not found", req.instance_id))
            })?;

        Ok(Response::new(GetFsmInstanceResponse {
            instance: Some(instance_to_proto(&instance)),
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn list_fsm_instances(
        &self,
        request: Request<ListFsmInstancesRequest>,
    ) -> Result<Response<ListFsmInstancesResponse>, Status> {
        let req = request.into_inner();
        let def_name = if req.definition_name.is_empty() {
            None
        } else {
            Some(req.definition_name.clone())
        };
        let status = if req.status.is_empty() {
            None
        } else {
            Some(
                req.status
                    .parse::<pacinet_core::FsmInstanceStatus>()
                    .map_err(Status::invalid_argument)?,
            )
        };

        let def_name_ref = def_name.clone();
        let instances = blocking(&self.storage, move |s| {
            s.list_fsm_instances(def_name_ref.as_deref(), status)
        })
        .await?;

        let proto_instances: Vec<FsmInstanceInfo> =
            instances.iter().map(instance_to_proto).collect();

        Ok(Response::new(ListFsmInstancesResponse {
            instances: proto_instances,
        }))
    }

    #[tracing::instrument(skip(self, request))]
    async fn advance_fsm(
        &self,
        request: Request<AdvanceFsmRequest>,
    ) -> Result<Response<AdvanceFsmResponse>, Status> {
        let req = request.into_inner();
        let engine = self
            .fsm_engine
            .as_ref()
            .ok_or_else(|| Status::internal("FSM engine not available"))?;

        let target = if req.target_state.is_empty() {
            None
        } else {
            Some(req.target_state.clone())
        };

        match engine.advance_instance(&req.instance_id, target).await {
            Ok(instance) => Ok(Response::new(AdvanceFsmResponse {
                success: true,
                current_state: instance.current_state,
                message: "FSM advanced".to_string(),
            })),
            Err(e) => Ok(Response::new(AdvanceFsmResponse {
                success: false,
                current_state: String::new(),
                message: e.to_string(),
            })),
        }
    }

    #[tracing::instrument(skip(self, request))]
    async fn cancel_fsm(
        &self,
        request: Request<CancelFsmRequest>,
    ) -> Result<Response<CancelFsmResponse>, Status> {
        let req = request.into_inner();
        let engine = self
            .fsm_engine
            .as_ref()
            .ok_or_else(|| Status::internal("FSM engine not available"))?;

        match engine.cancel_instance(&req.instance_id, &req.reason).await {
            Ok(()) => Ok(Response::new(CancelFsmResponse {
                success: true,
                message: "FSM instance cancelled".to_string(),
            })),
            Err(e) => Ok(Response::new(CancelFsmResponse {
                success: false,
                message: e.to_string(),
            })),
        }
    }

    // ---- Phase 9: Node Annotations ----

    async fn set_node_annotations(
        &self,
        request: Request<SetNodeAnnotationsRequest>,
    ) -> Result<Response<SetNodeAnnotationsResponse>, Status> {
        let req = request.into_inner();
        let node_id = req.node_id.clone();
        let set = req.annotations;
        let remove = req.remove_keys;
        blocking(&self.storage, move |s| {
            s.update_annotations(&node_id, set, &remove)
        })
        .await?;

        Ok(Response::new(SetNodeAnnotationsResponse {
            success: true,
            message: "Annotations updated".to_string(),
        }))
    }

    // ---- Phase 9: Audit Log ----

    async fn query_audit_log(
        &self,
        request: Request<QueryAuditLogRequest>,
    ) -> Result<Response<QueryAuditLogResponse>, Status> {
        let req = request.into_inner();
        let action = if req.action.is_empty() {
            None
        } else {
            Some(req.action)
        };
        let resource_type = if req.resource_type.is_empty() {
            None
        } else {
            Some(req.resource_type)
        };
        let resource_id = if req.resource_id.is_empty() {
            None
        } else {
            Some(req.resource_id)
        };
        let since = req.since.map(|ts| {
            chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                .unwrap_or_else(chrono::Utc::now)
        });
        let limit = if req.limit == 0 { 50 } else { req.limit };

        let entries = blocking(&self.storage, move |s| {
            s.query_audit(
                action.as_deref(),
                resource_type.as_deref(),
                resource_id.as_deref(),
                since,
                limit,
            )
        })
        .await?;

        let proto_entries: Vec<AuditEntryInfo> = entries
            .into_iter()
            .map(|e| AuditEntryInfo {
                id: e.id,
                timestamp: Some(prost_types::Timestamp {
                    seconds: e.timestamp.timestamp(),
                    nanos: 0,
                }),
                actor: e.actor,
                action: e.action,
                resource_type: e.resource_type,
                resource_id: e.resource_id,
                details: e.details,
            })
            .collect();

        Ok(Response::new(QueryAuditLogResponse {
            entries: proto_entries,
        }))
    }

    // ---- Phase 9: Policy Templates ----

    async fn create_policy_template(
        &self,
        request: Request<CreatePolicyTemplateRequest>,
    ) -> Result<Response<CreatePolicyTemplateResponse>, Status> {
        let req = request.into_inner();
        let now = chrono::Utc::now();
        let template = pacinet_core::PolicyTemplate {
            name: req.name.clone(),
            description: req.description,
            rules_yaml: req.rules_yaml,
            tags: req.tags,
            created_at: now,
            updated_at: now,
        };
        let name = template.name.clone();

        blocking(&self.storage, move |s| s.store_template(template)).await?;

        Ok(Response::new(CreatePolicyTemplateResponse {
            success: true,
            name: name.clone(),
            message: format!("Template '{}' created", name),
        }))
    }

    async fn get_policy_template(
        &self,
        request: Request<GetPolicyTemplateRequest>,
    ) -> Result<Response<GetPolicyTemplateResponse>, Status> {
        let req = request.into_inner();
        let name = req.name.clone();
        let template = blocking(&self.storage, move |s| s.get_template(&name))
            .await?
            .ok_or_else(|| Status::not_found(format!("Template '{}' not found", req.name)))?;

        Ok(Response::new(GetPolicyTemplateResponse {
            name: template.name,
            description: template.description,
            rules_yaml: template.rules_yaml,
            tags: template.tags,
            created_at: Some(prost_types::Timestamp {
                seconds: template.created_at.timestamp(),
                nanos: 0,
            }),
            updated_at: Some(prost_types::Timestamp {
                seconds: template.updated_at.timestamp(),
                nanos: 0,
            }),
        }))
    }

    async fn list_policy_templates(
        &self,
        request: Request<ListPolicyTemplatesRequest>,
    ) -> Result<Response<ListPolicyTemplatesResponse>, Status> {
        let req = request.into_inner();
        let tag = if req.tag.is_empty() {
            None
        } else {
            Some(req.tag)
        };
        let templates = blocking(&self.storage, move |s| s.list_templates(tag.as_deref())).await?;

        let summaries: Vec<PolicyTemplateSummary> = templates
            .into_iter()
            .map(|t| PolicyTemplateSummary {
                name: t.name,
                description: t.description,
                tags: t.tags,
                created_at: Some(prost_types::Timestamp {
                    seconds: t.created_at.timestamp(),
                    nanos: 0,
                }),
                updated_at: Some(prost_types::Timestamp {
                    seconds: t.updated_at.timestamp(),
                    nanos: 0,
                }),
            })
            .collect();

        Ok(Response::new(ListPolicyTemplatesResponse {
            templates: summaries,
        }))
    }

    async fn delete_policy_template(
        &self,
        request: Request<DeletePolicyTemplateRequest>,
    ) -> Result<Response<DeletePolicyTemplateResponse>, Status> {
        let req = request.into_inner();
        let name = req.name.clone();
        let deleted = blocking(&self.storage, move |s| s.delete_template(&name)).await?;

        Ok(Response::new(DeletePolicyTemplateResponse {
            success: deleted,
            message: if deleted {
                format!("Template '{}' deleted", req.name)
            } else {
                format!("Template '{}' not found", req.name)
            },
        }))
    }

    // ---- Phase 9: Webhook Delivery History ----

    async fn query_webhook_deliveries(
        &self,
        request: Request<QueryWebhookDeliveriesRequest>,
    ) -> Result<Response<QueryWebhookDeliveriesResponse>, Status> {
        let req = request.into_inner();
        let instance_id = if req.instance_id.is_empty() {
            None
        } else {
            Some(req.instance_id)
        };
        let limit = if req.limit == 0 { 50 } else { req.limit };

        let deliveries = blocking(&self.storage, move |s| {
            s.query_webhook_deliveries(instance_id.as_deref(), limit)
        })
        .await?;

        let proto_deliveries: Vec<WebhookDeliveryInfo> = deliveries
            .into_iter()
            .map(|d| WebhookDeliveryInfo {
                id: d.id,
                instance_id: d.instance_id,
                url: d.url,
                method: d.method,
                status_code: d.status_code.map(|c| c as u32).unwrap_or(0),
                success: d.success,
                duration_ms: d.duration_ms,
                error: d.error.unwrap_or_default(),
                attempt: d.attempt,
                timestamp: Some(prost_types::Timestamp {
                    seconds: d.timestamp.timestamp(),
                    nanos: 0,
                }),
            })
            .collect();

        Ok(Response::new(QueryWebhookDeliveriesResponse {
            deliveries: proto_deliveries,
        }))
    }
}

fn instance_to_proto(instance: &pacinet_core::FsmInstance) -> FsmInstanceInfo {
    FsmInstanceInfo {
        instance_id: instance.instance_id.clone(),
        definition_name: instance.definition_name.clone(),
        current_state: instance.current_state.clone(),
        status: instance.status.to_string(),
        created_at: Some(prost_types::Timestamp {
            seconds: instance.created_at.timestamp(),
            nanos: 0,
        }),
        updated_at: Some(prost_types::Timestamp {
            seconds: instance.updated_at.timestamp(),
            nanos: 0,
        }),
        history: instance
            .history
            .iter()
            .map(|t| FsmTransitionInfo {
                from_state: t.from_state.clone(),
                to_state: t.to_state.clone(),
                trigger: t.trigger.to_string(),
                timestamp: Some(prost_types::Timestamp {
                    seconds: t.timestamp.timestamp(),
                    nanos: 0,
                }),
                message: t.message.clone(),
            })
            .collect(),
        deployed_nodes: instance.context.deployed_nodes.len() as u32,
        failed_nodes: instance.context.failed_nodes.len() as u32,
        target_nodes: instance.context.target_nodes.len() as u32,
    }
}

// ---- Proto conversion helpers for streaming events ----

fn domain_fsm_to_proto(event: &DomainFsmEvent) -> FsmEvent {
    match event {
        DomainFsmEvent::Transition {
            instance_id,
            definition_name,
            from_state,
            to_state,
            trigger,
            message,
            timestamp,
        } => FsmEvent {
            event_type: FsmEventType::FsmEventTransition.into(),
            instance_id: instance_id.clone(),
            definition_name: definition_name.clone(),
            from_state: from_state.clone(),
            to_state: to_state.clone(),
            trigger: trigger.clone(),
            message: message.clone(),
            deployed_nodes: 0,
            failed_nodes: 0,
            target_nodes: 0,
            final_status: String::new(),
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.timestamp(),
                nanos: 0,
            }),
        },
        DomainFsmEvent::DeployProgress {
            instance_id,
            definition_name,
            deployed_nodes,
            failed_nodes,
            target_nodes,
            timestamp,
        } => FsmEvent {
            event_type: FsmEventType::FsmEventDeployProgress.into(),
            instance_id: instance_id.clone(),
            definition_name: definition_name.clone(),
            from_state: String::new(),
            to_state: String::new(),
            trigger: String::new(),
            message: String::new(),
            deployed_nodes: *deployed_nodes,
            failed_nodes: *failed_nodes,
            target_nodes: *target_nodes,
            final_status: String::new(),
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.timestamp(),
                nanos: 0,
            }),
        },
        DomainFsmEvent::InstanceCompleted {
            instance_id,
            definition_name,
            final_status,
            timestamp,
        } => FsmEvent {
            event_type: FsmEventType::FsmEventInstanceCompleted.into(),
            instance_id: instance_id.clone(),
            definition_name: definition_name.clone(),
            from_state: String::new(),
            to_state: String::new(),
            trigger: String::new(),
            message: String::new(),
            deployed_nodes: 0,
            failed_nodes: 0,
            target_nodes: 0,
            final_status: final_status.clone(),
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.timestamp(),
                nanos: 0,
            }),
        },
    }
}

fn domain_counter_to_proto(event: &CounterEvent) -> CounterUpdate {
    CounterUpdate {
        node_id: event.node_id.clone(),
        counters: event
            .counters
            .iter()
            .map(|c| CounterRateInfo {
                rule_name: c.rule_name.clone(),
                match_count: c.match_count,
                byte_count: c.byte_count,
                matches_per_second: c.matches_per_second,
                bytes_per_second: c.bytes_per_second,
            })
            .collect(),
        collected_at: Some(prost_types::Timestamp {
            seconds: event.collected_at.timestamp(),
            nanos: 0,
        }),
    }
}

fn domain_node_to_proto(event: &NodeEvent) -> pacinet_proto::NodeEvent {
    match event {
        NodeEvent::Registered {
            node_id,
            hostname,
            labels,
            timestamp,
        } => pacinet_proto::NodeEvent {
            event_type: NodeEventType::NodeEventRegistered.into(),
            node_id: node_id.clone(),
            hostname: hostname.clone(),
            labels: labels.clone(),
            old_state: NodeState::Unspecified.into(),
            new_state: NodeState::Registered.into(),
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.timestamp(),
                nanos: 0,
            }),
        },
        NodeEvent::StateChanged {
            node_id,
            hostname,
            labels,
            old_state,
            new_state,
            timestamp,
        } => {
            let old = old_state
                .parse::<pacinet_core::NodeState>()
                .map(|s| pacinet_proto::NodeState::from(s) as i32)
                .unwrap_or(0);
            let new = new_state
                .parse::<pacinet_core::NodeState>()
                .map(|s| pacinet_proto::NodeState::from(s) as i32)
                .unwrap_or(0);
            pacinet_proto::NodeEvent {
                event_type: NodeEventType::NodeEventStateChanged.into(),
                node_id: node_id.clone(),
                hostname: hostname.clone(),
                labels: labels.clone(),
                old_state: old,
                new_state: new,
                timestamp: Some(prost_types::Timestamp {
                    seconds: timestamp.timestamp(),
                    nanos: 0,
                }),
            }
        }
        NodeEvent::HeartbeatStale {
            node_id,
            hostname,
            labels,
            timestamp,
        } => pacinet_proto::NodeEvent {
            event_type: NodeEventType::NodeEventHeartbeatStale.into(),
            node_id: node_id.clone(),
            hostname: hostname.clone(),
            labels: labels.clone(),
            old_state: NodeState::Unspecified.into(),
            new_state: NodeState::Offline.into(),
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.timestamp(),
                nanos: 0,
            }),
        },
        NodeEvent::Removed {
            node_id,
            hostname,
            labels,
            timestamp,
        } => pacinet_proto::NodeEvent {
            event_type: NodeEventType::NodeEventRemoved.into(),
            node_id: node_id.clone(),
            hostname: hostname.clone(),
            labels: labels.clone(),
            old_state: NodeState::Unspecified.into(),
            new_state: NodeState::Unspecified.into(),
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.timestamp(),
                nanos: 0,
            }),
        },
    }
}

impl ManagementService {
    async fn do_deploy(
        &self,
        req: &DeployPolicyRequest,
        node: &Node,
    ) -> Result<Response<DeployPolicyResponse>, Status> {
        let options = req.options.clone().unwrap_or_default();
        let outcome = crate::deploy::deploy_to_node(
            &self.storage,
            node,
            &req.rules_yaml,
            options,
            self.config.deploy_timeout,
            &self.tls_config,
        )
        .await;

        Ok(Response::new(DeployPolicyResponse {
            success: outcome.success,
            message: outcome.message,
            warnings: outcome.warnings,
            dry_run_result: None,
        }))
    }
}

/// Helper for batch deploy — single node deploy with guard
async fn deploy_single_node(
    storage: &Arc<dyn Storage>,
    req: &DeployPolicyRequest,
    node: &Node,
    deploy_timeout: std::time::Duration,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<DeployPolicyResponse, Status> {
    // Acquire deploy guard
    let node_id = req.node_id.clone();
    blocking(storage, move |s| s.begin_deploy(&node_id)).await?;

    let options = req.options.clone().unwrap_or_default();
    let outcome = crate::deploy::deploy_to_node(
        storage,
        node,
        &req.rules_yaml,
        options,
        deploy_timeout,
        tls_config,
    )
    .await;

    // Release deploy guard
    storage.end_deploy(&req.node_id);

    Ok(DeployPolicyResponse {
        success: outcome.success,
        message: outcome.message,
        warnings: outcome.warnings,
        dry_run_result: None,
    })
}
