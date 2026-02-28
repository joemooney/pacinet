use crate::registry::NodeRegistry;
use pacinet_core::model::Node;
use pacinet_proto::*;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

// ============================================================================
// PaciNetController service — agent → controller
// ============================================================================

pub struct ControllerService {
    registry: Arc<NodeRegistry>,
}

impl ControllerService {
    pub fn new(registry: Arc<NodeRegistry>) -> Self {
        Self { registry }
    }
}

#[tonic::async_trait]
impl paci_net_controller_server::PaciNetController for ControllerService {
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
        let node_id = self.registry.register_node(node);

        info!(node_id = %node_id, "Node registered successfully");

        Ok(Response::new(RegisterNodeResponse {
            node_id,
            accepted: true,
            message: "Node registered".to_string(),
        }))
    }

    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        let state = pacinet_core::NodeState::from(
            NodeState::try_from(req.state).unwrap_or(NodeState::Online),
        );

        if !self.registry.update_heartbeat(&req.node_id, state) {
            warn!(node_id = %req.node_id, "Heartbeat from unknown node");
            return Err(Status::not_found("Node not registered"));
        }

        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
        }))
    }

    async fn report_counters(
        &self,
        request: Request<ReportCountersRequest>,
    ) -> Result<Response<ReportCountersResponse>, Status> {
        let req = request.into_inner();
        let counters: Vec<pacinet_core::RuleCounter> =
            req.counters.into_iter().map(|c| c.into()).collect();

        self.registry.store_counters(&req.node_id, counters);

        Ok(Response::new(ReportCountersResponse {
            acknowledged: true,
        }))
    }
}

// ============================================================================
// PaciNetManagement service — CLI → controller
// ============================================================================

pub struct ManagementService {
    registry: Arc<NodeRegistry>,
}

impl ManagementService {
    pub fn new(registry: Arc<NodeRegistry>) -> Self {
        Self { registry }
    }
}

fn node_to_proto(node: &pacinet_core::model::Node) -> NodeInfo {
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
    }
}

#[tonic::async_trait]
impl paci_net_management_server::PaciNetManagement for ManagementService {
    async fn list_nodes(
        &self,
        request: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        let req = request.into_inner();
        let nodes = self.registry.list_nodes(&req.label_filter);
        let proto_nodes: Vec<NodeInfo> = nodes.iter().map(node_to_proto).collect();

        Ok(Response::new(ListNodesResponse {
            nodes: proto_nodes,
        }))
    }

    async fn get_node(
        &self,
        request: Request<GetNodeRequest>,
    ) -> Result<Response<GetNodeResponse>, Status> {
        let req = request.into_inner();
        let node = self
            .registry
            .get_node(&req.node_id)
            .ok_or_else(|| Status::not_found(format!("Node {} not found", req.node_id)))?;

        Ok(Response::new(GetNodeResponse {
            node: Some(node_to_proto(&node)),
        }))
    }

    async fn remove_node(
        &self,
        request: Request<RemoveNodeRequest>,
    ) -> Result<Response<RemoveNodeResponse>, Status> {
        let req = request.into_inner();
        let removed = self.registry.remove_node(&req.node_id);

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

    async fn deploy_policy(
        &self,
        request: Request<DeployPolicyRequest>,
    ) -> Result<Response<DeployPolicyResponse>, Status> {
        let req = request.into_inner();

        // Verify node exists
        let node = self
            .registry
            .get_node(&req.node_id)
            .ok_or_else(|| Status::not_found(format!("Node {} not found", req.node_id)))?;

        let policy_hash = format!("{:x}", md5_hash(&req.rules_yaml));
        let options = req.options.unwrap_or_default();

        // Store policy locally
        let policy = pacinet_core::model::Policy {
            node_id: req.node_id.clone(),
            rules_yaml: req.rules_yaml.clone(),
            policy_hash,
            deployed_at: chrono::Utc::now(),
            counters_enabled: options.counters,
            rate_limit_enabled: options.rate_limit,
            conntrack_enabled: options.conntrack,
        };
        self.registry.store_policy(policy);

        // Set node to Deploying state
        self.registry
            .update_node_state(&req.node_id, pacinet_core::NodeState::Deploying);

        // Forward deploy request to agent via gRPC
        let agent_addr = format!("http://{}", node.agent_address);
        info!(node_id = %req.node_id, agent = %agent_addr, "Forwarding deploy to agent");

        let agent_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Self::forward_deploy_to_agent(&agent_addr, &req.rules_yaml, req.options),
        )
        .await;

        match agent_result {
            Ok(Ok(response)) => {
                if response.success {
                    self.registry
                        .update_node_state(&req.node_id, pacinet_core::NodeState::Active);
                    info!(node_id = %req.node_id, "Policy deployed successfully to agent");
                } else {
                    self.registry
                        .update_node_state(&req.node_id, pacinet_core::NodeState::Error);
                    warn!(node_id = %req.node_id, msg = %response.message, "Agent deploy failed");
                }
                Ok(Response::new(DeployPolicyResponse {
                    success: response.success,
                    message: response.message,
                    warnings: response.warnings,
                }))
            }
            Ok(Err(e)) => {
                self.registry
                    .update_node_state(&req.node_id, pacinet_core::NodeState::Error);
                warn!(node_id = %req.node_id, error = %e, "Failed to connect to agent");
                Ok(Response::new(DeployPolicyResponse {
                    success: false,
                    message: format!("Failed to reach agent: {}", e),
                    warnings: vec!["Policy stored locally but agent unreachable".to_string()],
                }))
            }
            Err(_) => {
                self.registry
                    .update_node_state(&req.node_id, pacinet_core::NodeState::Error);
                warn!(node_id = %req.node_id, "Agent deploy timed out after 30s");
                Ok(Response::new(DeployPolicyResponse {
                    success: false,
                    message: "Agent communication timed out (30s)".to_string(),
                    warnings: vec!["Policy stored locally but agent timed out".to_string()],
                }))
            }
        }
    }

    async fn get_policy(
        &self,
        request: Request<GetPolicyRequest>,
    ) -> Result<Response<GetPolicyResponse>, Status> {
        let req = request.into_inner();
        let policy = self
            .registry
            .get_policy(&req.node_id)
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

    async fn get_node_counters(
        &self,
        request: Request<GetNodeCountersRequest>,
    ) -> Result<Response<GetNodeCountersResponse>, Status> {
        let req = request.into_inner();
        let counters = self.registry.get_counters(&req.node_id).unwrap_or_default();
        let proto_counters: Vec<RuleCounter> =
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

    async fn get_aggregate_counters(
        &self,
        request: Request<GetAggregateCountersRequest>,
    ) -> Result<Response<GetAggregateCountersResponse>, Status> {
        let req = request.into_inner();
        let nodes = self.registry.list_nodes(&req.label_filter);

        let mut node_counters = Vec::new();
        for node in &nodes {
            if let Some(counters) = self.registry.get_counters(&node.node_id) {
                let proto_counters: Vec<RuleCounter> =
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
}

impl ManagementService {
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

/// Simple hash for policy content (not cryptographic, just for identity)
fn md5_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
