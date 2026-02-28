use crate::pacgate::PacGateRunner;
use pacinet_proto::*;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::info;

pub struct AgentState {
    pub node_id: String,
    pub controller_address: String,
    pub pacgate_runner: PacGateRunner,
}

pub struct AgentService {
    state: Arc<RwLock<AgentState>>,
}

impl AgentService {
    pub fn new(state: Arc<RwLock<AgentState>>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl paci_net_agent_server::PaciNetAgent for AgentService {
    async fn deploy_rules(
        &self,
        request: Request<DeployRulesRequest>,
    ) -> Result<Response<DeployRulesResponse>, Status> {
        let req = request.into_inner();
        info!("Received deploy_rules request ({} bytes)", req.rules_yaml.len());

        let state = self.state.read().await;
        let options = req.options.unwrap_or_default();

        match state
            .pacgate_runner
            .compile(&req.rules_yaml, options.counters, options.rate_limit, options.conntrack)
            .await
        {
            Ok(result) => Ok(Response::new(DeployRulesResponse {
                success: result.success,
                message: result.message,
                warnings: result.warnings,
            })),
            Err(e) => Ok(Response::new(DeployRulesResponse {
                success: false,
                message: format!("PacGate error: {}", e),
                warnings: vec![],
            })),
        }
    }

    async fn get_counters(
        &self,
        _request: Request<GetCountersRequest>,
    ) -> Result<Response<GetCountersResponse>, Status> {
        // TODO: Read counters from FPGA/simulation
        Ok(Response::new(GetCountersResponse {
            counters: vec![],
            collected_at: Some(prost_types::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
        }))
    }

    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        Ok(Response::new(GetStatusResponse {
            state: NodeState::Online as i32,
            pacgate_version: "0.1.0".to_string(),
            active_policy_hash: String::new(),
            uptime_seconds: 0,
        }))
    }
}
