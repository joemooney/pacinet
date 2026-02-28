//! Integration tests for PaciNet controller + agent end-to-end flows.
//!
//! Uses ephemeral ports to avoid conflicts. Tests start real gRPC servers.

use std::collections::HashMap;
use std::sync::Arc;

use pacinet_agent::pacgate::PacGateBackend;
use pacinet_agent::service::{AgentService, AgentState};
use pacinet_proto::*;
use pacinet_server::registry::NodeRegistry;
use pacinet_server::service::{ControllerService, ManagementService};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio_stream::wrappers::TcpListenerStream;

/// Start the controller (PaciNetController + PaciNetManagement) on an ephemeral port.
/// Returns the port it's listening on.
async fn start_controller(registry: Arc<NodeRegistry>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let controller_service = ControllerService::new(registry.clone());
    let management_service = ManagementService::new(registry.clone());

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(
                paci_net_controller_server::PaciNetControllerServer::new(controller_service),
            )
            .add_service(
                paci_net_management_server::PaciNetManagementServer::new(management_service),
            )
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give server a moment to bind
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    port
}

/// Start an agent gRPC server on an ephemeral port with given PacGate backend.
/// Returns the port.
async fn start_agent(backend: PacGateBackend) -> (u16, Arc<RwLock<AgentState>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let state = Arc::new(RwLock::new(AgentState {
        node_id: String::new(),
        controller_address: String::new(),
        pacgate: backend,
        active_policy_hash: None,
        active_rules_yaml: None,
        deployed_at: None,
        start_time: tokio::time::Instant::now(),
        counters: vec![],
    }));

    let agent_service = AgentService::new(state.clone());

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(paci_net_agent_server::PaciNetAgentServer::new(agent_service))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (port, state)
}

/// Full happy path: register node, deploy policy, push counters, query counters.
#[tokio::test]
async fn test_register_deploy_counters_flow() {
    let registry = Arc::new(NodeRegistry::new());
    let ctrl_port = start_controller(registry.clone()).await;
    let ctrl_addr = format!("http://127.0.0.1:{}", ctrl_port);

    // Start agent with mock PacGate (success)
    let (agent_port, agent_state) = start_agent(PacGateBackend::Mock { should_succeed: true }).await;
    let agent_address = format!("127.0.0.1:{}", agent_port);

    // 1. Register node
    let mut ctrl_client =
        paci_net_controller_client::PaciNetControllerClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let reg_resp = ctrl_client
        .register_node(RegisterNodeRequest {
            hostname: "test-node-1".to_string(),
            agent_address: agent_address.clone(),
            labels: HashMap::from([("env".to_string(), "test".to_string())]),
            pacgate_version: "0.1.0".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(reg_resp.accepted);
    let node_id = reg_resp.node_id;

    // Update agent's node_id
    {
        let mut s = agent_state.write().await;
        s.node_id = node_id.clone();
    }

    // 2. Deploy policy via management API
    let mut mgmt_client =
        paci_net_management_client::PaciNetManagementClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let deploy_resp = mgmt_client
        .deploy_policy(DeployPolicyRequest {
            node_id: node_id.clone(),
            rules_yaml: "rules:\n  - name: allow_ssh\n    action: allow\n    port: 22".to_string(),
            options: Some(CompileOptions {
                counters: true,
                rate_limit: false,
                conntrack: false,
            }),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(deploy_resp.success, "Deploy failed: {}", deploy_resp.message);

    // 3. Verify node state is Active
    let node = registry.get_node(&node_id).unwrap();
    assert!(
        matches!(node.state, pacinet_core::NodeState::Active),
        "Expected Active, got {:?}",
        node.state
    );

    // 4. Agent should have policy hash set
    {
        let s = agent_state.read().await;
        assert!(s.active_policy_hash.is_some());
        assert!(s.active_rules_yaml.is_some());
    }

    // 5. Push counters to controller
    let counters = vec![
        RuleCounter {
            rule_name: "allow_ssh".to_string(),
            match_count: 42,
            byte_count: 3200,
        },
    ];

    let counter_resp = ctrl_client
        .report_counters(ReportCountersRequest {
            node_id: node_id.clone(),
            counters: counters.clone(),
            collected_at: Some(prost_types::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(counter_resp.acknowledged);

    // 6. Query counters back via management API
    let counter_query = mgmt_client
        .get_node_counters(GetNodeCountersRequest {
            node_id: node_id.clone(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(counter_query.counters.len(), 1);
    assert_eq!(counter_query.counters[0].rule_name, "allow_ssh");
    assert_eq!(counter_query.counters[0].match_count, 42);
}

/// Register node with dead agent address, deploy should fail gracefully (not panic).
#[tokio::test]
async fn test_deploy_to_unreachable_agent() {
    let registry = Arc::new(NodeRegistry::new());
    let ctrl_port = start_controller(registry.clone()).await;
    let ctrl_addr = format!("http://127.0.0.1:{}", ctrl_port);

    // Register a node pointing to a port where nothing is listening
    let mut ctrl_client =
        paci_net_controller_client::PaciNetControllerClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let reg_resp = ctrl_client
        .register_node(RegisterNodeRequest {
            hostname: "dead-agent".to_string(),
            agent_address: "127.0.0.1:19999".to_string(),
            labels: HashMap::new(),
            pacgate_version: "0.1.0".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(reg_resp.accepted);
    let node_id = reg_resp.node_id;

    // Deploy should fail gracefully
    let mut mgmt_client =
        paci_net_management_client::PaciNetManagementClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let deploy_resp = mgmt_client
        .deploy_policy(DeployPolicyRequest {
            node_id: node_id.clone(),
            rules_yaml: "rules: []".to_string(),
            options: None,
        })
        .await
        .unwrap()
        .into_inner();

    // Should not panic, should return failure
    assert!(!deploy_resp.success);
    assert!(
        deploy_resp.message.contains("Failed to reach agent")
            || deploy_resp.message.contains("timed out"),
        "Unexpected message: {}",
        deploy_resp.message
    );

    // Node state should be Error
    let node = registry.get_node(&node_id).unwrap();
    assert!(
        matches!(node.state, pacinet_core::NodeState::Error),
        "Expected Error, got {:?}",
        node.state
    );
}

/// Mock PacGate returns failure — verify deploy returns success=false, node state = Error.
#[tokio::test]
async fn test_deploy_with_pacgate_failure() {
    let registry = Arc::new(NodeRegistry::new());
    let ctrl_port = start_controller(registry.clone()).await;
    let ctrl_addr = format!("http://127.0.0.1:{}", ctrl_port);

    // Start agent with mock PacGate that fails
    let (agent_port, _agent_state) =
        start_agent(PacGateBackend::Mock { should_succeed: false }).await;
    let agent_address = format!("127.0.0.1:{}", agent_port);

    // Register node
    let mut ctrl_client =
        paci_net_controller_client::PaciNetControllerClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let reg_resp = ctrl_client
        .register_node(RegisterNodeRequest {
            hostname: "fail-agent".to_string(),
            agent_address,
            labels: HashMap::new(),
            pacgate_version: "0.1.0".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    let node_id = reg_resp.node_id;

    // Deploy — agent accepts the call but PacGate compile fails
    let mut mgmt_client =
        paci_net_management_client::PaciNetManagementClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let deploy_resp = mgmt_client
        .deploy_policy(DeployPolicyRequest {
            node_id: node_id.clone(),
            rules_yaml: "rules: []".to_string(),
            options: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert!(!deploy_resp.success);
    assert!(
        deploy_resp.message.contains("failed"),
        "Expected failure message, got: {}",
        deploy_resp.message
    );

    // Node state should be Error
    let node = registry.get_node(&node_id).unwrap();
    assert!(
        matches!(node.state, pacinet_core::NodeState::Error),
        "Expected Error, got {:?}",
        node.state
    );
}
