use crate::config::ControllerConfig;
use crate::storage::blocking;
use pacinet_core::model::{DeploymentRecord, DeploymentResult, Node, Policy};
use pacinet_core::Storage;
use pacinet_proto::*;
use std::sync::Arc;
use tokio::task::JoinSet;
use tonic::{Request, Response, Status};
use tracing::{debug, info, warn};

// ============================================================================
// PaciNetController service — agent → controller
// ============================================================================

pub struct ControllerService {
    storage: Arc<dyn Storage>,
}

impl ControllerService {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
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

        let found =
            blocking(&self.storage, move |s| s.update_heartbeat(&node_id, state, uptime))
                .await?;

        if !found {
            warn!(node_id = %node_id_log, "Heartbeat from unknown node");
            return Err(Status::not_found("Node not registered"));
        }

        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
        }))
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

        blocking(&self.storage, move |s| s.store_counters(&node_id, counters)).await?;

        Ok(Response::new(ReportCountersResponse {
            acknowledged: true,
        }))
    }
}

// ============================================================================
// PaciNetManagement service — CLI → controller
// ============================================================================

pub struct ManagementService {
    storage: Arc<dyn Storage>,
    config: ControllerConfig,
}

impl ManagementService {
    pub fn new(storage: Arc<dyn Storage>, config: ControllerConfig) -> Self {
        Self { storage, config }
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

        Ok(Response::new(ListNodesResponse {
            nodes: proto_nodes,
        }))
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

            join_set.spawn(async move {
                let deploy_req = DeployPolicyRequest {
                    node_id: node.node_id.clone(),
                    rules_yaml,
                    options,
                };

                // Try deploy with per-node guard and timeout
                let result =
                    deploy_single_node(&storage, &deploy_req, &node, deploy_timeout).await;
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
            *nodes_by_state
                .entry(node.state.to_string())
                .or_insert(0) += 1;
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
}

impl ManagementService {
    async fn do_deploy(
        &self,
        req: &DeployPolicyRequest,
        node: &Node,
    ) -> Result<Response<DeployPolicyResponse>, Status> {
        let policy_hash = format!("{:x}", md5_hash(&req.rules_yaml));
        let options = req.options.unwrap_or_default();

        // Store policy
        let policy = Policy {
            node_id: req.node_id.clone(),
            rules_yaml: req.rules_yaml.clone(),
            policy_hash: policy_hash.clone(),
            deployed_at: chrono::Utc::now(),
            counters_enabled: options.counters,
            rate_limit_enabled: options.rate_limit,
            conntrack_enabled: options.conntrack,
        };
        let node_id = req.node_id.clone();
        let policy_clone = policy.clone();
        let version = blocking(&self.storage, move |s| s.store_policy(policy_clone)).await?;

        // Set node to Deploying state
        let node_id_clone = req.node_id.clone();
        let _ = blocking(&self.storage, move |s| {
            s.update_node_state(&node_id_clone, pacinet_core::NodeState::Deploying)
        })
        .await;

        // Forward deploy request to agent via gRPC
        let agent_addr = format!("http://{}", node.agent_address);
        info!(node_id = %req.node_id, agent = %agent_addr, "Forwarding deploy to agent");

        let deploy_timeout = self.config.deploy_timeout;
        let agent_result = tokio::time::timeout(
            deploy_timeout,
            Self::forward_deploy_to_agent(&agent_addr, &req.rules_yaml, req.options),
        )
        .await;

        let (response, deploy_result) = match agent_result {
            Ok(Ok(response)) => {
                if response.success {
                    let nid = req.node_id.clone();
                    let _ = blocking(&self.storage, move |s| {
                        s.update_node_state(&nid, pacinet_core::NodeState::Active)
                    })
                    .await;
                    info!(node_id = %req.node_id, "Policy deployed successfully to agent");
                    let resp = DeployPolicyResponse {
                        success: true,
                        message: response.message,
                        warnings: response.warnings,
                    };
                    (resp, DeploymentResult::Success)
                } else {
                    let nid = req.node_id.clone();
                    let _ = blocking(&self.storage, move |s| {
                        s.update_node_state(&nid, pacinet_core::NodeState::Error)
                    })
                    .await;
                    warn!(node_id = %req.node_id, msg = %response.message, "Agent deploy failed");
                    let resp = DeployPolicyResponse {
                        success: false,
                        message: response.message,
                        warnings: response.warnings,
                    };
                    (resp, DeploymentResult::AgentFailure)
                }
            }
            Ok(Err(e)) => {
                let nid = req.node_id.clone();
                let _ = blocking(&self.storage, move |s| {
                    s.update_node_state(&nid, pacinet_core::NodeState::Error)
                })
                .await;
                warn!(node_id = %req.node_id, error = %e, "Failed to connect to agent");
                let resp = DeployPolicyResponse {
                    success: false,
                    message: format!("Failed to reach agent: {}", e),
                    warnings: vec!["Policy stored locally but agent unreachable".to_string()],
                };
                (resp, DeploymentResult::AgentUnreachable)
            }
            Err(_) => {
                let nid = req.node_id.clone();
                let _ = blocking(&self.storage, move |s| {
                    s.update_node_state(&nid, pacinet_core::NodeState::Error)
                })
                .await;
                let timeout_secs = deploy_timeout.as_secs();
                warn!(node_id = %req.node_id, "Agent deploy timed out after {}s", timeout_secs);
                let resp = DeployPolicyResponse {
                    success: false,
                    message: format!("Agent communication timed out ({}s)", timeout_secs),
                    warnings: vec!["Policy stored locally but agent timed out".to_string()],
                };
                (resp, DeploymentResult::Timeout)
            }
        };

        // Record deployment audit
        let record = DeploymentRecord {
            id: uuid::Uuid::new_v4().to_string(),
            node_id,
            policy_version: version,
            policy_hash,
            deployed_at: policy.deployed_at,
            result: deploy_result,
            message: response.message.clone(),
        };
        let _ = blocking(&self.storage, move |s| s.record_deployment(record)).await;

        Ok(Response::new(response))
    }

    async fn forward_deploy_to_agent(
        agent_addr: &str,
        rules_yaml: &str,
        options: Option<CompileOptions>,
    ) -> Result<DeployRulesResponse, Box<dyn std::error::Error + Send + Sync>> {
        let mut client =
            paci_net_agent_client::PaciNetAgentClient::connect(agent_addr.to_string()).await?;

        let response = client
            .deploy_rules(DeployRulesRequest {
                rules_yaml: rules_yaml.to_string(),
                options,
            })
            .await?;

        Ok(response.into_inner())
    }
}

/// Helper for batch deploy — single node deploy with guard
async fn deploy_single_node(
    storage: &Arc<dyn Storage>,
    req: &DeployPolicyRequest,
    node: &Node,
    deploy_timeout: std::time::Duration,
) -> Result<DeployPolicyResponse, Status> {
    // Acquire deploy guard
    let node_id = req.node_id.clone();
    blocking(storage, move |s| s.begin_deploy(&node_id)).await?;

    let result = do_deploy_for_batch(storage, req, node, deploy_timeout).await;

    // Release deploy guard
    storage.end_deploy(&req.node_id);

    result
}

async fn do_deploy_for_batch(
    storage: &Arc<dyn Storage>,
    req: &DeployPolicyRequest,
    node: &Node,
    deploy_timeout: std::time::Duration,
) -> Result<DeployPolicyResponse, Status> {
    let policy_hash = format!("{:x}", md5_hash(&req.rules_yaml));
    let options = req.options.unwrap_or_default();

    let policy = Policy {
        node_id: req.node_id.clone(),
        rules_yaml: req.rules_yaml.clone(),
        policy_hash: policy_hash.clone(),
        deployed_at: chrono::Utc::now(),
        counters_enabled: options.counters,
        rate_limit_enabled: options.rate_limit,
        conntrack_enabled: options.conntrack,
    };
    let policy_clone = policy.clone();
    let node_id = req.node_id.clone();
    let version = blocking(storage, move |s| s.store_policy(policy_clone)).await?;

    // Set Deploying state
    let nid = req.node_id.clone();
    let _ = blocking(storage, move |s| {
        s.update_node_state(&nid, pacinet_core::NodeState::Deploying)
    })
    .await;

    let agent_addr = format!("http://{}", node.agent_address);
    debug!(node_id = %req.node_id, agent = %agent_addr, "Forwarding batch deploy to agent");

    let agent_result = tokio::time::timeout(
        deploy_timeout,
        ManagementService::forward_deploy_to_agent(&agent_addr, &req.rules_yaml, req.options),
    )
    .await;

    let (response, deploy_result) = match agent_result {
        Ok(Ok(response)) => {
            if response.success {
                let nid = req.node_id.clone();
                let _ = blocking(storage, move |s| {
                    s.update_node_state(&nid, pacinet_core::NodeState::Active)
                })
                .await;
                (
                    DeployPolicyResponse {
                        success: true,
                        message: response.message,
                        warnings: response.warnings,
                    },
                    DeploymentResult::Success,
                )
            } else {
                let nid = req.node_id.clone();
                let _ = blocking(storage, move |s| {
                    s.update_node_state(&nid, pacinet_core::NodeState::Error)
                })
                .await;
                (
                    DeployPolicyResponse {
                        success: false,
                        message: response.message,
                        warnings: response.warnings,
                    },
                    DeploymentResult::AgentFailure,
                )
            }
        }
        Ok(Err(e)) => {
            let nid = req.node_id.clone();
            let _ = blocking(storage, move |s| {
                s.update_node_state(&nid, pacinet_core::NodeState::Error)
            })
            .await;
            (
                DeployPolicyResponse {
                    success: false,
                    message: format!("Failed to reach agent: {}", e),
                    warnings: vec!["Policy stored locally but agent unreachable".to_string()],
                },
                DeploymentResult::AgentUnreachable,
            )
        }
        Err(_) => {
            let nid = req.node_id.clone();
            let _ = blocking(storage, move |s| {
                s.update_node_state(&nid, pacinet_core::NodeState::Error)
            })
            .await;
            (
                DeployPolicyResponse {
                    success: false,
                    message: format!("Agent communication timed out ({}s)", deploy_timeout.as_secs()),
                    warnings: vec!["Policy stored locally but agent timed out".to_string()],
                },
                DeploymentResult::Timeout,
            )
        }
    };

    // Record deployment
    let record = DeploymentRecord {
        id: uuid::Uuid::new_v4().to_string(),
        node_id,
        policy_version: version,
        policy_hash,
        deployed_at: policy.deployed_at,
        result: deploy_result,
        message: response.message.clone(),
    };
    let _ = blocking(storage, move |s| s.record_deployment(record)).await;

    Ok(response)
}

/// Simple hash for policy content (not cryptographic, just for identity)
fn md5_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
