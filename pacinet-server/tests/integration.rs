//! Integration tests for PaciNet controller + agent end-to-end flows.
//!
//! Uses ephemeral ports to avoid conflicts. Tests start real gRPC servers.

use std::collections::HashMap;
use std::sync::Arc;

use pacinet_agent::pacgate::PacGateBackend;
use pacinet_agent::service::{AgentService, AgentState};
use pacinet_core::Storage;
use pacinet_proto::*;
use pacinet_server::config::ControllerConfig;
use pacinet_server::service::{ControllerService, ManagementService};
use pacinet_server::storage::MemoryStorage;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio_stream::wrappers::TcpListenerStream;

/// Start the controller (PaciNetController + PaciNetManagement) on an ephemeral port.
/// Returns the port it's listening on.
async fn start_controller(storage: Arc<dyn Storage>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let controller_service = ControllerService::new(storage.clone());
    let config = ControllerConfig::default();
    let management_service = ManagementService::new(storage.clone(), config);

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
        pacgate_version: "0.1.0".to_string(),
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

/// Register a node via the controller gRPC and return the node_id.
async fn register_node(
    ctrl_addr: &str,
    hostname: &str,
    agent_address: &str,
    labels: HashMap<String, String>,
) -> String {
    let mut ctrl_client =
        paci_net_controller_client::PaciNetControllerClient::connect(ctrl_addr.to_string())
            .await
            .unwrap();

    let reg_resp = ctrl_client
        .register_node(RegisterNodeRequest {
            hostname: hostname.to_string(),
            agent_address: agent_address.to_string(),
            labels,
            pacgate_version: "0.1.0".to_string(),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(reg_resp.accepted);
    reg_resp.node_id
}

/// Full happy path: register node, deploy policy, push counters, query counters.
#[tokio::test]
async fn test_register_deploy_counters_flow() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let ctrl_port = start_controller(storage.clone()).await;
    let ctrl_addr = format!("http://127.0.0.1:{}", ctrl_port);

    // Start agent with mock PacGate (success)
    let (agent_port, agent_state) =
        start_agent(PacGateBackend::Mock { should_succeed: true }).await;
    let agent_address = format!("127.0.0.1:{}", agent_port);

    // 1. Register node
    let node_id = register_node(
        &ctrl_addr,
        "test-node-1",
        &agent_address,
        HashMap::from([("env".to_string(), "test".to_string())]),
    )
    .await;

    // Update agent's node_id
    {
        let mut s = agent_state.write().await;
        s.node_id = node_id.clone();
    }

    // Need to transition Registered → Online before deploy can go to Deploying
    storage
        .update_node_state(&node_id, pacinet_core::NodeState::Online)
        .unwrap();

    // 2. Deploy policy via management API
    let mut mgmt_client =
        paci_net_management_client::PaciNetManagementClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let deploy_resp = mgmt_client
        .deploy_policy(DeployPolicyRequest {
            node_id: node_id.clone(),
            rules_yaml: "rules:\n  - name: allow_ssh\n    action: allow\n    port: 22"
                .to_string(),
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
    let node = storage.get_node(&node_id).unwrap().unwrap();
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
    let mut ctrl_client =
        paci_net_controller_client::PaciNetControllerClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let counters = vec![RuleCounter {
        rule_name: "allow_ssh".to_string(),
        match_count: 42,
        byte_count: 3200,
    }];

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
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let ctrl_port = start_controller(storage.clone()).await;
    let ctrl_addr = format!("http://127.0.0.1:{}", ctrl_port);

    // Register a node pointing to a port where nothing is listening
    let node_id = register_node(
        &ctrl_addr,
        "dead-agent",
        "127.0.0.1:19999",
        HashMap::new(),
    )
    .await;

    // Transition to Online so deploy can work
    storage
        .update_node_state(&node_id, pacinet_core::NodeState::Online)
        .unwrap();

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
    let node = storage.get_node(&node_id).unwrap().unwrap();
    assert!(
        matches!(node.state, pacinet_core::NodeState::Error),
        "Expected Error, got {:?}",
        node.state
    );
}

/// Mock PacGate returns failure — verify deploy returns success=false, node state = Error.
#[tokio::test]
async fn test_deploy_with_pacgate_failure() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let ctrl_port = start_controller(storage.clone()).await;
    let ctrl_addr = format!("http://127.0.0.1:{}", ctrl_port);

    // Start agent with mock PacGate that fails
    let (agent_port, _agent_state) =
        start_agent(PacGateBackend::Mock { should_succeed: false }).await;
    let agent_address = format!("127.0.0.1:{}", agent_port);

    // Register node
    let node_id = register_node(&ctrl_addr, "fail-agent", &agent_address, HashMap::new()).await;

    // Transition to Online
    storage
        .update_node_state(&node_id, pacinet_core::NodeState::Online)
        .unwrap();

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
    let node = storage.get_node(&node_id).unwrap().unwrap();
    assert!(
        matches!(node.state, pacinet_core::NodeState::Error),
        "Expected Error, got {:?}",
        node.state
    );
}

/// Batch deploy to multiple nodes — all succeed.
#[tokio::test]
async fn test_batch_deploy_to_multiple_nodes() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let ctrl_port = start_controller(storage.clone()).await;
    let ctrl_addr = format!("http://127.0.0.1:{}", ctrl_port);

    // Start 3 agents
    let (agent_port_1, _) = start_agent(PacGateBackend::Mock { should_succeed: true }).await;
    let (agent_port_2, _) = start_agent(PacGateBackend::Mock { should_succeed: true }).await;
    let (agent_port_3, _) = start_agent(PacGateBackend::Mock { should_succeed: true }).await;

    let labels = HashMap::from([("env".to_string(), "prod".to_string())]);

    // Register 3 nodes
    let nid1 = register_node(
        &ctrl_addr,
        "node-1",
        &format!("127.0.0.1:{}", agent_port_1),
        labels.clone(),
    )
    .await;
    let nid2 = register_node(
        &ctrl_addr,
        "node-2",
        &format!("127.0.0.1:{}", agent_port_2),
        labels.clone(),
    )
    .await;
    let nid3 = register_node(
        &ctrl_addr,
        "node-3",
        &format!("127.0.0.1:{}", agent_port_3),
        labels.clone(),
    )
    .await;

    // Transition all to Online
    for nid in [&nid1, &nid2, &nid3] {
        storage
            .update_node_state(nid, pacinet_core::NodeState::Online)
            .unwrap();
    }

    // Batch deploy
    let mut mgmt_client =
        paci_net_management_client::PaciNetManagementClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let resp = mgmt_client
        .batch_deploy_policy(BatchDeployPolicyRequest {
            label_filter: HashMap::from([("env".to_string(), "prod".to_string())]),
            rules_yaml: "rules:\n  - name: block_all\n    action: deny".to_string(),
            options: Some(CompileOptions {
                counters: true,
                rate_limit: false,
                conntrack: false,
            }),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.total_nodes, 3);
    assert_eq!(resp.succeeded, 3);
    assert_eq!(resp.failed, 0);
    assert_eq!(resp.results.len(), 3);
    for result in &resp.results {
        assert!(result.success, "Node {} failed: {}", result.node_id, result.message);
    }
}

/// Batch deploy with partial failure — one dead agent.
#[tokio::test]
async fn test_batch_deploy_partial_failure() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let ctrl_port = start_controller(storage.clone()).await;
    let ctrl_addr = format!("http://127.0.0.1:{}", ctrl_port);

    // One working agent, one dead
    let (agent_port, _) = start_agent(PacGateBackend::Mock { should_succeed: true }).await;

    let labels = HashMap::from([("env".to_string(), "staging".to_string())]);

    let nid1 = register_node(
        &ctrl_addr,
        "good-node",
        &format!("127.0.0.1:{}", agent_port),
        labels.clone(),
    )
    .await;
    let nid2 = register_node(
        &ctrl_addr,
        "dead-node",
        "127.0.0.1:19998",
        labels.clone(),
    )
    .await;

    for nid in [&nid1, &nid2] {
        storage
            .update_node_state(nid, pacinet_core::NodeState::Online)
            .unwrap();
    }

    let mut mgmt_client =
        paci_net_management_client::PaciNetManagementClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let resp = mgmt_client
        .batch_deploy_policy(BatchDeployPolicyRequest {
            label_filter: HashMap::from([("env".to_string(), "staging".to_string())]),
            rules_yaml: "rules: []".to_string(),
            options: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.total_nodes, 2);
    assert_eq!(resp.succeeded, 1);
    assert_eq!(resp.failed, 1);

    let good = resp.results.iter().find(|r| r.hostname == "good-node").unwrap();
    assert!(good.success);

    let bad = resp.results.iter().find(|r| r.hostname == "dead-node").unwrap();
    assert!(!bad.success);
}

/// Fleet status shows node counts by state and enriched summaries.
#[tokio::test]
async fn test_fleet_status() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let ctrl_port = start_controller(storage.clone()).await;
    let ctrl_addr = format!("http://127.0.0.1:{}", ctrl_port);

    // Register nodes
    let nid1 = register_node(&ctrl_addr, "online-node", "127.0.0.1:9001", HashMap::new()).await;
    let nid2 = register_node(&ctrl_addr, "error-node", "127.0.0.1:9002", HashMap::new()).await;
    let _nid3 = register_node(&ctrl_addr, "registered-node", "127.0.0.1:9003", HashMap::new()).await;

    // Transition nodes to various states
    storage
        .update_node_state(&nid1, pacinet_core::NodeState::Online)
        .unwrap();
    storage
        .update_node_state(&nid2, pacinet_core::NodeState::Online)
        .unwrap();
    storage
        .update_node_state(&nid2, pacinet_core::NodeState::Error)
        .unwrap();

    let mut mgmt_client =
        paci_net_management_client::PaciNetManagementClient::connect(ctrl_addr.clone())
            .await
            .unwrap();

    let resp = mgmt_client
        .get_fleet_status(GetFleetStatusRequest {
            label_filter: HashMap::new(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.total_nodes, 3);
    assert_eq!(resp.nodes.len(), 3);

    // Check nodes_by_state
    let online_count = resp.nodes_by_state.get("online").copied().unwrap_or(0);
    let error_count = resp.nodes_by_state.get("error").copied().unwrap_or(0);
    let registered_count = resp.nodes_by_state.get("registered").copied().unwrap_or(0);
    assert_eq!(online_count, 1);
    assert_eq!(error_count, 1);
    assert_eq!(registered_count, 1);
}

/// Stale node detection — node goes offline after missed heartbeats.
#[tokio::test]
async fn test_stale_node_detection() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());

    // Register node directly and set heartbeat to 5 minutes ago
    let mut node = pacinet_core::Node::new(
        "stale-node".to_string(),
        "127.0.0.1:9999".to_string(),
        HashMap::new(),
        "0.1.0".to_string(),
    );
    node.last_heartbeat = chrono::Utc::now() - chrono::Duration::minutes(5);
    node.state = pacinet_core::NodeState::Online;
    let node_id = storage.register_node(node).unwrap();

    // Mark stale with 2 minute threshold
    let stale = storage.mark_stale_nodes(chrono::Duration::minutes(2)).unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0], node_id);

    let node = storage.get_node(&node_id).unwrap().unwrap();
    assert_eq!(node.state, pacinet_core::NodeState::Offline);
}
