use crate::pacgate::PacGateBackend;
use pacinet_proto::*;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::info;

pub struct AgentState {
    pub node_id: String,
    pub controller_address: String,
    pub pacgate: PacGateBackend,
    pub active_policy_hash: Option<String>,
    pub active_rules_yaml: Option<String>,
    pub deployed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub start_time: tokio::time::Instant,
    pub counters: Vec<RuleCounter>,
    pub pacgate_version: String,
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
    #[tracing::instrument(skip(self, request))]
    async fn deploy_rules(
        &self,
        request: Request<DeployRulesRequest>,
    ) -> Result<Response<DeployRulesResponse>, Status> {
        let req = request.into_inner();
        info!(
            "Received deploy_rules request ({} bytes)",
            req.rules_yaml.len()
        );

        let options = req.options.unwrap_or_default();

        // Compile with PacGate backend (takes read lock only for compile)
        let compile_result = {
            let state = self.state.read().await;
            state
                .pacgate
                .compile(
                    &req.rules_yaml,
                    options.counters,
                    options.rate_limit,
                    options.conntrack,
                )
                .await
        };

        match compile_result {
            Ok(result) => {
                if result.success {
                    // Update state on success
                    let policy_hash = pacinet_core::policy_hash(&req.rules_yaml);
                    let mut state = self.state.write().await;
                    state.active_policy_hash = Some(policy_hash);
                    state.active_rules_yaml = Some(req.rules_yaml.clone());
                    state.deployed_at = Some(chrono::Utc::now());
                    info!(
                        rules_count = ?result.rules_count,
                        "Rules deployed successfully"
                    );
                }

                Ok(Response::new(DeployRulesResponse {
                    success: result.success,
                    message: result.message,
                    warnings: result.warnings,
                }))
            }
            Err(e) => Ok(Response::new(DeployRulesResponse {
                success: false,
                message: format!("PacGate error: {}", e),
                warnings: vec![],
            })),
        }
    }

    #[tracing::instrument(skip(self, _request))]
    async fn get_counters(
        &self,
        _request: Request<GetCountersRequest>,
    ) -> Result<Response<GetCountersResponse>, Status> {
        let state = self.state.read().await;
        Ok(Response::new(GetCountersResponse {
            counters: state.counters.clone(),
            collected_at: Some(prost_types::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
        }))
    }

    #[tracing::instrument(skip(self, _request))]
    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        let state = self.state.read().await;
        let uptime = state.start_time.elapsed().as_secs();
        let node_state = if state.active_policy_hash.is_some() {
            NodeState::Active
        } else {
            NodeState::Online
        };

        Ok(Response::new(GetStatusResponse {
            state: node_state as i32,
            pacgate_version: state.pacgate_version.clone(),
            active_policy_hash: state.active_policy_hash.clone().unwrap_or_default(),
            uptime_seconds: uptime,
        }))
    }
}
