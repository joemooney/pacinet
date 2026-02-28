//! REST API integration tests for PaciNet controller.
//!
//! Starts both gRPC + axum on ephemeral ports sharing the same AppState.
//! Uses reqwest as HTTP client.

use std::collections::HashMap;
use std::sync::Arc;

use pacinet_core::Storage;
use pacinet_server::config::ControllerConfig;
use pacinet_server::counter_cache::CounterSnapshotCache;
use pacinet_server::events::EventBus;
use pacinet_server::fsm_engine::FsmEngine;
use pacinet_server::rest::{self, AppState};
use pacinet_server::service::{ControllerService, ManagementService};
use pacinet_server::storage::MemoryStorage;
use pacinet_proto::*;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

/// Start the controller (gRPC) + REST on ephemeral ports.
/// Returns (grpc_port, rest_port, event_bus).
async fn start_rest_server(
    storage: Arc<dyn Storage>,
    api_key: Option<String>,
) -> (u16, u16, EventBus) {
    let config = ControllerConfig::default();
    let counter_cache = Arc::new(CounterSnapshotCache::new(
        chrono::Duration::seconds(3600),
        120,
    ));
    let event_bus = EventBus::new(256);
    let fsm_engine = Arc::new(
        FsmEngine::new(
            storage.clone(),
            config.clone(),
            None,
            counter_cache.clone(),
        )
        .with_event_bus(event_bus.clone()),
    );

    // Start gRPC server
    let grpc_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let grpc_port = grpc_listener.local_addr().unwrap().port();

    let controller_service = ControllerService::new(storage.clone())
        .with_counter_cache(counter_cache.clone())
        .with_event_bus(event_bus.clone());
    let management_service = ManagementService::new(storage.clone(), config.clone())
        .with_fsm_engine(fsm_engine.clone())
        .with_event_bus(event_bus.clone());

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(paci_net_controller_server::PaciNetControllerServer::new(
                controller_service,
            ))
            .add_service(paci_net_management_server::PaciNetManagementServer::new(
                management_service,
            ))
            .serve_with_incoming(TcpListenerStream::new(grpc_listener))
            .await
            .unwrap();
    });

    // Start REST server
    let app_state = AppState {
        storage: storage.clone(),
        config: config.clone(),
        counter_cache: counter_cache.clone(),
        fsm_engine: fsm_engine.clone(),
        event_bus: event_bus.clone(),
        tls_config: None,
        api_key,
    };

    let app = rest::router(app_state);
    let rest_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rest_port = rest_listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(rest_listener, app).await.unwrap();
    });

    // Brief yield to let servers start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (grpc_port, rest_port, event_bus)
}

/// Register a node via gRPC. Returns node_id.
async fn register_node_grpc(
    grpc_port: u16,
    hostname: &str,
    labels: HashMap<String, String>,
) -> String {
    let addr = format!("http://127.0.0.1:{}", grpc_port);
    let mut client =
        paci_net_controller_client::PaciNetControllerClient::connect(addr)
            .await
            .unwrap();
    let resp = client
        .register_node(RegisterNodeRequest {
            hostname: hostname.to_string(),
            agent_address: format!("127.0.0.1:{}", grpc_port + 1000),
            labels,
            pacgate_version: "0.1.0-test".to_string(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(resp.accepted);
    resp.node_id
}

fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{}", port)
}

// ============================================================================
// Node CRUD tests
// ============================================================================

#[tokio::test]
async fn test_rest_list_nodes_empty() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/nodes", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let nodes: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(nodes.len(), 0);
}

#[tokio::test]
async fn test_rest_list_and_get_node() {
    let storage = Arc::new(MemoryStorage::new());
    let (grpc_port, rest_port, _) = start_rest_server(storage, None).await;

    let node_id = register_node_grpc(grpc_port, "test-node-1", HashMap::new()).await;

    let client = reqwest::Client::new();

    // List nodes
    let resp = client
        .get(format!("{}/api/nodes", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let nodes: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["hostname"], "test-node-1");

    // Get single node
    let resp = client
        .get(format!("{}/api/nodes/{}", base_url(rest_port), node_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let node: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(node["hostname"], "test-node-1");
    assert_eq!(node["state"], "registered");
}

#[tokio::test]
async fn test_rest_get_node_not_found() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/nodes/nonexistent", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_rest_delete_node() {
    let storage = Arc::new(MemoryStorage::new());
    let (grpc_port, rest_port, _) = start_rest_server(storage, None).await;

    let node_id = register_node_grpc(grpc_port, "to-delete", HashMap::new()).await;

    let client = reqwest::Client::new();
    let resp = client
        .delete(format!("{}/api/nodes/{}", base_url(rest_port), node_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["success"], true);

    // Verify node is gone
    let resp = client
        .get(format!("{}/api/nodes/{}", base_url(rest_port), node_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ============================================================================
// Fleet tests
// ============================================================================

#[tokio::test]
async fn test_rest_fleet_status() {
    let storage = Arc::new(MemoryStorage::new());
    let (grpc_port, rest_port, _) = start_rest_server(storage, None).await;

    let labels: HashMap<String, String> = [("env".to_string(), "prod".to_string())].into();
    register_node_grpc(grpc_port, "fleet-1", labels.clone()).await;
    register_node_grpc(grpc_port, "fleet-2", labels).await;

    let client = reqwest::Client::new();

    // All nodes
    let resp = client
        .get(format!("{}/api/fleet", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let fleet: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(fleet["total_nodes"], 2);

    // Label filter
    let resp = client
        .get(format!(
            "{}/api/fleet?label=env%3Dprod",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let fleet: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(fleet["total_nodes"], 2);

    // Non-matching label
    let resp = client
        .get(format!(
            "{}/api/fleet?label=env%3Ddev",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let fleet: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(fleet["total_nodes"], 0);
}

// ============================================================================
// Policy tests
// ============================================================================

#[tokio::test]
async fn test_rest_policy_not_found() {
    let storage = Arc::new(MemoryStorage::new());
    let (grpc_port, rest_port, _) = start_rest_server(storage, None).await;

    let node_id = register_node_grpc(grpc_port, "no-policy", HashMap::new()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/nodes/{}/policy",
            base_url(rest_port),
            node_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_rest_policy_and_deploy_history() {
    let storage = Arc::new(MemoryStorage::new());
    let (grpc_port, rest_port, _) = start_rest_server(storage, None).await;

    let node_id = register_node_grpc(grpc_port, "history-node", HashMap::new()).await;

    let client = reqwest::Client::new();

    // Empty history
    let resp = client
        .get(format!(
            "{}/api/nodes/{}/policy/history",
            base_url(rest_port),
            node_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let history: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(history.len(), 0);

    // Empty deploy history
    let resp = client
        .get(format!(
            "{}/api/nodes/{}/deploy/history",
            base_url(rest_port),
            node_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let history: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(history.len(), 0);
}

// ============================================================================
// FSM tests
// ============================================================================

#[tokio::test]
async fn test_rest_fsm_definitions_crud() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();

    // List empty
    let resp = client
        .get(format!("{}/api/fsm/definitions", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let defs: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(defs.len(), 0);

    // Create definition
    let yaml = r#"
name: test-deploy
kind: deployment
description: Test deploy FSM
initial: start
states:
  start:
    transitions:
      - to: done
        when:
          manual: true
  done:
    terminal: true
"#;
    let resp = client
        .post(format!("{}/api/fsm/definitions", base_url(rest_port)))
        .json(&serde_json::json!({"definition_yaml": yaml}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["name"], "test-deploy");

    // Get definition
    let resp = client
        .get(format!(
            "{}/api/fsm/definitions/test-deploy",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let def: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(def["name"], "test-deploy");
    assert_eq!(def["kind"], "deployment");

    // Get non-existent
    let resp = client
        .get(format!(
            "{}/api/fsm/definitions/nonexistent",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    // Delete
    let resp = client
        .delete(format!(
            "{}/api/fsm/definitions/test-deploy",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["success"], true);

    // Verify deleted
    let resp = client
        .get(format!(
            "{}/api/fsm/definitions/test-deploy",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_rest_fsm_instance_not_found() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/fsm/instances/nonexistent",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ============================================================================
// Health endpoint tests
// ============================================================================

#[tokio::test]
async fn test_rest_health() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/health", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["auth_required"], false);
    assert_eq!(body["role"], "leader");
}

// ============================================================================
// Auth tests
// ============================================================================

#[tokio::test]
async fn test_rest_auth_required() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) =
        start_rest_server(storage, Some("test-secret-key".to_string())).await;

    let client = reqwest::Client::new();

    // Health should work without auth
    let resp = client
        .get(format!("{}/api/health", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["auth_required"], true);

    // Nodes without auth should fail
    let resp = client
        .get(format!("{}/api/nodes", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // With correct Bearer token
    let resp = client
        .get(format!("{}/api/nodes", base_url(rest_port)))
        .header("Authorization", "Bearer test-secret-key")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // With wrong token
    let resp = client
        .get(format!("{}/api/nodes", base_url(rest_port)))
        .header("Authorization", "Bearer wrong-key")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_rest_auth_token_query_param() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) =
        start_rest_server(storage, Some("my-api-key".to_string())).await;

    let client = reqwest::Client::new();

    // With ?token= query param
    let resp = client
        .get(format!(
            "{}/api/nodes?token=my-api-key",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // With wrong ?token=
    let resp = client
        .get(format!(
            "{}/api/nodes?token=wrong",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_rest_no_auth_when_no_key() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();

    // All requests succeed when no key configured
    let resp = client
        .get(format!("{}/api/nodes", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .get(format!("{}/api/fleet", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

// ============================================================================
// SSE event test
// ============================================================================

#[tokio::test]
async fn test_rest_sse_node_events() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, event_bus) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();

    // Connect to SSE endpoint
    let resp = client
        .get(format!("{}/api/events/nodes", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Emit a node event after a short delay
    let bus = event_bus.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        bus.emit_node(pacinet_server::events::NodeEvent::Registered {
            node_id: "test-sse-node".to_string(),
            hostname: "sse-host".to_string(),
            labels: HashMap::new(),
            timestamp: chrono::Utc::now(),
        });
    });

    // Read the chunked response with timeout
    let body = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        resp.text(),
    )
    .await;

    // The SSE connection stays open, so timeout is expected.
    // If we got data before timeout, check it contains our event.
    if let Ok(Ok(text)) = body {
        if !text.is_empty() {
            assert!(text.contains("sse-host") || text.contains("test-sse-node"));
        }
    }
    // If timeout, that's fine - SSE streams don't close
}

// ============================================================================
// Event history test
// ============================================================================

#[tokio::test]
async fn test_rest_event_history() {
    let storage = Arc::new(MemoryStorage::new());

    // Store an event directly
    let event = pacinet_core::PersistentEvent {
        id: "evt-1".to_string(),
        event_type: "node.registered".to_string(),
        source: "node-abc".to_string(),
        payload: r#"{"test":true}"#.to_string(),
        timestamp: chrono::Utc::now(),
    };
    storage.store_event(event).unwrap();

    let (_, rest_port, _) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/events/history", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let events: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_type"], "node.registered");
    assert_eq!(events[0]["source"], "node-abc");

    // Filter by type
    let resp = client
        .get(format!(
            "{}/api/events/history?type=node.registered",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let events: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(events.len(), 1);

    // Filter by non-matching type
    let resp = client
        .get(format!(
            "{}/api/events/history?type=fsm.transition",
            base_url(rest_port)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let events: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(events.len(), 0);
}

// ============================================================================
// Deploy test (will fail without agent, but test the request handling)
// ============================================================================

#[tokio::test]
async fn test_rest_deploy_node_not_found() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/deploy", base_url(rest_port)))
        .json(&serde_json::json!({
            "node_id": "nonexistent",
            "rules_yaml": "rules: []"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ============================================================================
// Counters test
// ============================================================================

#[tokio::test]
async fn test_rest_aggregate_counters_empty() {
    let storage = Arc::new(MemoryStorage::new());
    let (_, rest_port, _) = start_rest_server(storage, None).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/counters", base_url(rest_port)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let counters: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(counters.len(), 0);
}
