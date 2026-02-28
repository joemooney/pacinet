use crate::config::ControllerConfig;
use crate::counter_cache::CounterSnapshotCache;
use crate::fsm_engine::FsmEngine;
use crate::metrics as m;
use crate::storage::blocking;
use pacinet_core::model::{Node, Policy};
use pacinet_core::{CounterSnapshot, Storage};
use pacinet_proto::*;
use std::sync::Arc;
use tokio::task::JoinSet;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

// ============================================================================
// PaciNetController service — agent → controller
// ============================================================================

pub struct ControllerService {
    storage: Arc<dyn Storage>,
    counter_cache: Option<Arc<CounterSnapshotCache>>,
}

impl ControllerService {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            counter_cache: None,
        }
    }

    pub fn with_counter_cache(mut self, cache: Arc<CounterSnapshotCache>) -> Self {
        self.counter_cache = Some(cache);
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

        let node = Node::new(
            req.hostname,
            req.agent_address,
            req.labels,
            req.pacgate_version,
        );
        let node_id = blocking(&self.storage, move |s| s.register_node(node)).await?;

        info!(node_id = %node_id, "Node registered successfully");

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
        let state = pacinet_core::NodeState::from(
            NodeState::try_from(req.state).unwrap_or(NodeState::Online),
        );
        let uptime = req.uptime_seconds;
        let node_id = req.node_id.clone();
        let node_id_log = req.node_id.clone();

        let found = blocking(&self.storage, move |s| {
            s.update_heartbeat(&node_id, state, uptime)
        })
        .await?;

        if !found {
            warn!(node_id = %node_id_log, "Heartbeat from unknown node");
            return Err(Status::not_found("Node not registered"));
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
        let node_id = req.node_id.clone();
        let counters: Vec<pacinet_core::RuleCounter> =
            req.counters.into_iter().map(|c| c.into()).collect();

        // Record snapshot in counter cache for rate tracking
        if let Some(ref cache) = self.counter_cache {
            let collected_at = req
                .collected_at
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
        }

        blocking(&self.storage, move |s| s.store_counters(&node_id, counters)).await?;

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
}

impl ManagementService {
    pub fn new(storage: Arc<dyn Storage>, config: ControllerConfig) -> Self {
        Self {
            storage,
            config,
            tls_config: None,
            fsm_engine: None,
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
}

fn node_to_proto(node: &Node, policy: Option<&Policy>) -> NodeInfo {
    let now = chrono::Utc::now();
    let heartbeat_age = (now - node.last_heartbeat).num_milliseconds() as f64 / 1000.0;
    NodeInfo {
        node_id: node.node_id.clone(),
        hostname: node.hostname.clone(),
        agent_address: node.agent_address.clone(),
        labels: node.labels.clone(),
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
        let node_id = req.node_id.clone();
        let removed = blocking(&self.storage, move |s| s.remove_node(&node_id)).await?;

        if removed {
            info!(node_id = %req.node_id, "Node removed");
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
            let options = req.options;
            let deploy_timeout = self.config.deploy_timeout;
            let tls = self.tls_config.clone();

            join_set.spawn(async move {
                let deploy_req = DeployPolicyRequest {
                    node_id: node.node_id.clone(),
                    rules_yaml,
                    options,
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
        });

        // Deploy the rollback via existing deploy flow
        let deploy_req = DeployPolicyRequest {
            node_id: node_id.clone(),
            rules_yaml,
            options,
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
                Ok(Response::new(RollbackPolicyResponse {
                    success: inner.success,
                    message: if inner.success {
                        format!("Rolled back to version {}", target_version)
                    } else {
                        inner.message
                    },
                    rolled_back_to_version: if inner.success { target_version } else { 0 },
                }))
            }
            Err(status) => Ok(Response::new(RollbackPolicyResponse {
                success: false,
                message: status.message().to_string(),
                rolled_back_to_version: 0,
            })),
        }
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

impl ManagementService {
    async fn do_deploy(
        &self,
        req: &DeployPolicyRequest,
        node: &Node,
    ) -> Result<Response<DeployPolicyResponse>, Status> {
        let options = req.options.unwrap_or_default();
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

    let options = req.options.unwrap_or_default();
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
    })
}
