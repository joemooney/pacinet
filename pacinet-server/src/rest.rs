//! Axum REST API for the PaciNet web dashboard.
//!
//! Shares the same storage, counter cache, FSM engine, and event bus
//! as the gRPC services — no self-calls.

use crate::config::ControllerConfig;
use crate::counter_cache::CounterSnapshotCache;
use crate::deploy;
use crate::events::{CounterEvent, EventBus, FsmEvent, NodeEvent};
use crate::fsm_engine::FsmEngine;
use crate::storage::blocking;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use pacinet_core::Storage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

// ============================================================================
// Application state shared across all REST handlers
// ============================================================================

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage>,
    pub config: ControllerConfig,
    pub counter_cache: Arc<CounterSnapshotCache>,
    pub fsm_engine: Arc<FsmEngine>,
    pub event_bus: EventBus,
    pub tls_config: Option<pacinet_core::tls::TlsConfig>,
    pub api_key: Option<String>,
}

// ============================================================================
// JSON response types
// ============================================================================

#[derive(Serialize)]
pub struct NodeJson {
    pub node_id: String,
    pub hostname: String,
    pub agent_address: String,
    pub labels: HashMap<String, String>,
    pub state: String,
    pub registered_at: String,
    pub last_heartbeat: String,
    pub pacgate_version: String,
    pub uptime_seconds: u64,
    pub policy_hash: String,
    pub last_heartbeat_age_seconds: f64,
    pub annotations: HashMap<String, String>,
    pub capabilities: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct PolicyJson {
    pub node_id: String,
    pub rules_yaml: String,
    pub policy_hash: String,
    pub deployed_at: String,
    pub counters_enabled: bool,
    pub rate_limit_enabled: bool,
    pub conntrack_enabled: bool,
    pub axi_enabled: bool,
    pub ports: u32,
    pub target: String,
    pub dynamic: bool,
    pub dynamic_entries: u32,
    pub width: u32,
    pub ptp: bool,
    pub rss: bool,
    pub rss_queues: u32,
    pub int: bool,
    pub int_switch_id: u32,
}

#[derive(Serialize)]
pub struct CounterJson {
    pub node_id: String,
    pub counters: Vec<RuleCounterJson>,
    pub collected_at: String,
}

#[derive(Serialize)]
pub struct RuleCounterJson {
    pub rule_name: String,
    pub match_count: u64,
    pub byte_count: u64,
}

#[derive(Serialize)]
pub struct FlowCounterJson {
    pub flow_key: String,
    pub packet_count: u64,
    pub byte_count: u64,
    pub state: String,
}

#[derive(Serialize)]
pub struct FlowCounterSetJson {
    pub node_id: String,
    pub flow_counters: Vec<FlowCounterJson>,
    pub collected_at: String,
}

#[derive(Serialize)]
pub struct PolicyVersionJson {
    pub version: u64,
    pub node_id: String,
    pub rules_yaml: String,
    pub policy_hash: String,
    pub deployed_at: String,
    pub counters_enabled: bool,
    pub rate_limit_enabled: bool,
    pub conntrack_enabled: bool,
    pub axi_enabled: bool,
    pub ports: u32,
    pub target: String,
    pub dynamic: bool,
    pub dynamic_entries: u32,
    pub width: u32,
    pub ptp: bool,
    pub rss: bool,
    pub rss_queues: u32,
    pub int: bool,
    pub int_switch_id: u32,
}

#[derive(Serialize)]
pub struct DeploymentJson {
    pub id: String,
    pub node_id: String,
    pub policy_version: u64,
    pub policy_hash: String,
    pub deployed_at: String,
    pub result: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct FleetStatusJson {
    pub total_nodes: u32,
    pub nodes_by_state: HashMap<String, u32>,
    pub nodes: Vec<FleetNodeJson>,
}

#[derive(Serialize)]
pub struct FleetNodeJson {
    pub node_id: String,
    pub hostname: String,
    pub state: String,
    pub policy_hash: String,
    pub uptime_seconds: u64,
    pub last_heartbeat_age_seconds: f64,
    pub last_deploy_time: Option<String>,
}

#[derive(Serialize)]
pub struct NodeCounterSetJson {
    pub node_id: String,
    pub counters: Vec<RuleCounterJson>,
    pub collected_at: String,
}

#[derive(Serialize)]
pub struct BatchDeployResultJson {
    pub total_nodes: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub results: Vec<NodeDeployResultJson>,
}

#[derive(Serialize)]
pub struct NodeDeployResultJson {
    pub node_id: String,
    pub hostname: String,
    pub success: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct FsmDefSummaryJson {
    pub name: String,
    pub kind: String,
    pub description: String,
    pub state_count: u32,
    pub initial_state: String,
}

#[derive(Serialize)]
pub struct FsmDefJson {
    pub name: String,
    pub kind: String,
    pub description: String,
    pub definition_yaml: String,
}

#[derive(Serialize)]
pub struct FsmInstanceJson {
    pub instance_id: String,
    pub definition_name: String,
    pub current_state: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub deployed_nodes: u32,
    pub failed_nodes: u32,
    pub target_nodes: u32,
    pub history: Vec<FsmTransitionJson>,
}

#[derive(Serialize)]
pub struct FsmTransitionJson {
    pub from_state: String,
    pub to_state: String,
    pub trigger: String,
    pub timestamp: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Serialize)]
pub struct DeployResponse {
    pub success: bool,
    pub message: String,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run_result: Option<DryRunResultJson>,
}

#[derive(Serialize)]
pub struct DryRunResultJson {
    pub valid: bool,
    pub validation_errors: Vec<String>,
    pub target_nodes: Vec<DryRunNodeJson>,
}

#[derive(Serialize)]
pub struct DryRunNodeJson {
    pub node_id: String,
    pub hostname: String,
    pub current_policy_hash: String,
    pub new_policy_hash: String,
    pub policy_changed: bool,
}

#[derive(Serialize)]
pub struct AuditEntryJson {
    pub id: String,
    pub timestamp: String,
    pub actor: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub details: serde_json::Value,
}

#[derive(Serialize)]
pub struct PolicyTemplateJson {
    pub name: String,
    pub description: String,
    pub rules_yaml: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct PolicyTemplateSummaryJson {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct WebhookDeliveryJson {
    pub id: String,
    pub instance_id: String,
    pub url: String,
    pub method: String,
    pub status_code: Option<u16>,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub attempt: u32,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct RollbackResponse {
    pub success: bool,
    pub message: String,
    pub version: u64,
}

#[derive(Serialize)]
pub struct CreateFsmDefResponse {
    pub success: bool,
    pub name: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct StartFsmResponse {
    pub success: bool,
    pub instance_id: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct AdvanceFsmResponse {
    pub success: bool,
    pub state: String,
    pub message: String,
}

// ============================================================================
// Request body types
// ============================================================================

#[derive(Deserialize)]
pub struct DeployRequest {
    pub node_id: String,
    pub rules_yaml: String,
    #[serde(default)]
    pub counters: bool,
    #[serde(default)]
    pub rate_limit: bool,
    #[serde(default)]
    pub conntrack: bool,
    #[serde(default)]
    pub axi: bool,
    #[serde(default = "default_ports")]
    pub ports: u32,
    #[serde(default = "default_target")]
    pub target: String,
    #[serde(default)]
    pub dynamic: bool,
    #[serde(default = "default_dynamic_entries")]
    pub dynamic_entries: u32,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default)]
    pub ptp: bool,
    #[serde(default)]
    pub rss: bool,
    #[serde(default = "default_rss_queues")]
    pub rss_queues: u32,
    #[serde(default)]
    pub int: bool,
    #[serde(default)]
    pub int_switch_id: u32,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Deserialize)]
pub struct BatchDeployRequest {
    pub label_filter: HashMap<String, String>,
    pub rules_yaml: String,
    #[serde(default)]
    pub counters: bool,
    #[serde(default)]
    pub rate_limit: bool,
    #[serde(default)]
    pub conntrack: bool,
    #[serde(default)]
    pub axi: bool,
    #[serde(default = "default_ports")]
    pub ports: u32,
    #[serde(default = "default_target")]
    pub target: String,
    #[serde(default)]
    pub dynamic: bool,
    #[serde(default = "default_dynamic_entries")]
    pub dynamic_entries: u32,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default)]
    pub ptp: bool,
    #[serde(default)]
    pub rss: bool,
    #[serde(default = "default_rss_queues")]
    pub rss_queues: u32,
    #[serde(default)]
    pub int: bool,
    #[serde(default)]
    pub int_switch_id: u32,
    #[serde(default)]
    pub dry_run: bool,
}

fn default_ports() -> u32 {
    1
}

fn default_target() -> String {
    "standalone".to_string()
}

fn default_dynamic_entries() -> u32 {
    16
}

fn default_width() -> u32 {
    8
}

fn default_rss_queues() -> u32 {
    4
}

#[derive(Deserialize)]
pub struct SetAnnotationsRequest {
    #[serde(default)]
    pub annotations: HashMap<String, String>,
    #[serde(default)]
    pub remove_keys: Vec<String>,
}

#[derive(Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub rules_yaml: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
pub struct TemplateQuery {
    #[serde(default)]
    pub tag: Option<String>,
}

#[derive(Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub resource_id: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default = "default_audit_limit")]
    pub limit: u32,
}

fn default_audit_limit() -> u32 {
    50
}

#[derive(Deserialize)]
pub struct WebhookQuery {
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default = "default_audit_limit")]
    pub limit: u32,
}

#[derive(Deserialize)]
pub struct RollbackRequest {
    #[serde(default)]
    pub target_version: u64,
}

#[derive(Deserialize)]
pub struct CreateFsmDefRequest {
    pub definition_yaml: String,
}

#[derive(Deserialize)]
pub struct StartFsmRequest {
    pub definition_name: String,
    #[serde(default)]
    pub rules_yaml: String,
    #[serde(default)]
    pub counters: bool,
    #[serde(default)]
    pub rate_limit: bool,
    #[serde(default)]
    pub conntrack: bool,
    #[serde(default)]
    pub axi: bool,
    #[serde(default = "default_ports")]
    pub ports: u32,
    #[serde(default = "default_target")]
    pub target: String,
    #[serde(default)]
    pub dynamic: bool,
    #[serde(default = "default_dynamic_entries")]
    pub dynamic_entries: u32,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default)]
    pub ptp: bool,
    #[serde(default)]
    pub rss: bool,
    #[serde(default = "default_rss_queues")]
    pub rss_queues: u32,
    #[serde(default)]
    pub int: bool,
    #[serde(default)]
    pub int_switch_id: u32,
    #[serde(default)]
    pub target_label_filter: HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct AdvanceFsmRequest {
    #[serde(default)]
    pub target_state: Option<String>,
}

#[derive(Deserialize)]
pub struct CancelFsmRequest {
    #[serde(default)]
    pub reason: String,
}

// ============================================================================
// Query parameter types
// ============================================================================

#[derive(Deserialize)]
pub struct LabelQuery {
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    10
}

#[derive(Deserialize)]
pub struct FsmDefQuery {
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Deserialize)]
pub struct FsmInstanceQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub definition: Option<String>,
}

#[derive(Deserialize)]
pub struct SseNodeQuery {
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Deserialize)]
pub struct SseCounterQuery {
    #[serde(default)]
    pub node: Option<String>,
}

#[derive(Deserialize)]
pub struct SseFsmQuery {
    #[serde(default)]
    pub instance: Option<String>,
}

// ============================================================================
// Error handling
// ============================================================================

pub struct AppError(StatusCode, String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

impl From<tonic::Status> for AppError {
    fn from(status: tonic::Status) -> Self {
        let code = match status.code() {
            tonic::Code::NotFound => StatusCode::NOT_FOUND,
            tonic::Code::InvalidArgument => StatusCode::BAD_REQUEST,
            tonic::Code::AlreadyExists => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        AppError(code, status.message().to_string())
    }
}

// ============================================================================
// Helper: parse "key=val,key2=val2" label query param into HashMap
// ============================================================================

fn parse_label_filter(label: &Option<String>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(ref s) = label {
        for pair in s.split(',') {
            if let Some((k, v)) = pair.split_once('=') {
                map.insert(k.to_string(), v.to_string());
            }
        }
    }
    map
}

// ============================================================================
// Health response
// ============================================================================

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub auth_required: bool,
    pub role: String,
}

// ============================================================================
// Event history types
// ============================================================================

#[derive(Serialize)]
pub struct PersistentEventJson {
    pub id: String,
    pub event_type: String,
    pub source: String,
    pub payload: serde_json::Value,
    pub timestamp: String,
}

#[derive(Deserialize)]
pub struct EventHistoryQuery {
    #[serde(default, rename = "type")]
    pub event_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default = "default_event_limit")]
    pub limit: u32,
    #[serde(default)]
    pub token: Option<String>,
}

fn default_event_limit() -> u32 {
    50
}

// ============================================================================
// SSE token query (for auth via query param)
// ============================================================================

#[derive(Deserialize)]
pub struct SseTokenQuery {
    #[serde(default)]
    pub token: Option<String>,
}

// ============================================================================
// Auth middleware
// ============================================================================

async fn auth_middleware(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    if let Some(ref expected_key) = state.api_key {
        // Check Authorization header first
        let auth_ok = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|v| {
                v.strip_prefix("Bearer ")
                    .map(|token| token == expected_key)
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if !auth_ok {
            // Check ?token= query param (needed for EventSource/SSE)
            let token_ok = req
                .uri()
                .query()
                .and_then(|q| {
                    q.split('&')
                        .find(|p| p.starts_with("token="))
                        .map(|p| &p[6..] == expected_key)
                })
                .unwrap_or(false);

            if !token_ok {
                return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
            }
        }
    }

    next.run(req).await
}

// ============================================================================
// Leader guard helper
// ============================================================================

fn require_leader(config: &ControllerConfig) -> Result<(), AppError> {
    if !config.is_leader() {
        Err(AppError(
            StatusCode::SERVICE_UNAVAILABLE,
            "This controller is in standby mode. Write operations must go to the leader."
                .to_string(),
        ))
    } else {
        Ok(())
    }
}

// ============================================================================
// Router
// ============================================================================

pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Health endpoint — always accessible (no auth)
    let health_routes = Router::new()
        .route("/api/health", get(health_check))
        .with_state(state.clone());

    // All other API endpoints — auth required when key configured
    let api_routes = Router::new()
        // Node endpoints
        .route("/api/nodes", get(list_nodes))
        .route("/api/nodes/{id}", get(get_node).delete(remove_node))
        .route("/api/nodes/{id}/policy", get(get_policy))
        .route("/api/nodes/{id}/counters", get(get_node_counters))
        .route("/api/nodes/{id}/flow-counters", get(get_node_flow_counters))
        .route("/api/nodes/{id}/policy/history", get(get_policy_history))
        .route(
            "/api/nodes/{id}/deploy/history",
            get(get_deployment_history),
        )
        .route("/api/nodes/{id}/policy/rollback", post(rollback_policy))
        .route(
            "/api/nodes/{id}/annotations",
            axum::routing::put(set_annotations),
        )
        // Fleet
        .route("/api/fleet", get(get_fleet_status))
        // Counters
        .route("/api/counters", get(get_aggregate_counters))
        .route("/api/flow-counters", get(get_aggregate_flow_counters))
        // Deploy
        .route("/api/deploy", post(deploy_policy))
        .route("/api/deploy/batch", post(batch_deploy_policy))
        // FSM definitions
        .route(
            "/api/fsm/definitions",
            get(list_fsm_definitions).post(create_fsm_definition),
        )
        .route(
            "/api/fsm/definitions/{name}",
            get(get_fsm_definition).delete(delete_fsm_definition),
        )
        // FSM instances
        .route(
            "/api/fsm/instances",
            get(list_fsm_instances).post(start_fsm),
        )
        .route("/api/fsm/instances/{id}", get(get_fsm_instance))
        .route("/api/fsm/instances/{id}/advance", post(advance_fsm))
        .route("/api/fsm/instances/{id}/cancel", post(cancel_fsm))
        // SSE
        .route("/api/events/nodes", get(sse_node_events))
        .route("/api/events/counters", get(sse_counter_events))
        .route("/api/events/fsm", get(sse_fsm_events))
        // Event history
        .route("/api/events/history", get(get_event_history))
        // Phase 9: Audit
        .route("/api/audit", get(get_audit_log))
        // Phase 9: Templates
        .route("/api/templates", get(list_templates).post(create_template))
        .route(
            "/api/templates/{name}",
            get(get_template).delete(delete_template),
        )
        // Phase 9: Webhook history
        .route("/api/webhooks/history", get(get_webhook_history))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state);

    health_routes.merge(api_routes).layer(cors)
}

// ============================================================================
// Node handlers
// ============================================================================

async fn list_nodes(
    State(state): State<AppState>,
    Query(q): Query<LabelQuery>,
) -> Result<Json<Vec<NodeJson>>, AppError> {
    let label_filter = parse_label_filter(&q.label);
    let nodes = blocking(&state.storage, move |s| s.list_nodes(&label_filter)).await?;

    let node_ids: Vec<String> = nodes.iter().map(|n| n.node_id.clone()).collect();
    let policies = blocking(&state.storage, move |s| s.get_policies_for_nodes(&node_ids)).await?;

    let now = chrono::Utc::now();
    let result: Vec<NodeJson> = nodes
        .iter()
        .map(|n| {
            let policy = policies.get(&n.node_id);
            let heartbeat_age = (now - n.last_heartbeat).num_milliseconds() as f64 / 1000.0;
            NodeJson {
                node_id: n.node_id.clone(),
                hostname: n.hostname.clone(),
                agent_address: n.agent_address.clone(),
                labels: n.labels.clone(),
                state: n.state.to_string(),
                registered_at: n.registered_at.to_rfc3339(),
                last_heartbeat: n.last_heartbeat.to_rfc3339(),
                pacgate_version: n.pacgate_version.clone(),
                uptime_seconds: n.uptime_seconds,
                policy_hash: policy.map(|p| p.policy_hash.clone()).unwrap_or_default(),
                last_heartbeat_age_seconds: heartbeat_age,
                annotations: n.annotations.clone(),
                capabilities: n.capabilities.clone(),
            }
        })
        .collect();

    Ok(Json(result))
}

async fn get_node(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NodeJson>, AppError> {
    let node_id = id.clone();
    let node = blocking(&state.storage, move |s| s.get_node(&node_id))
        .await?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, format!("Node {} not found", id)))?;

    let node_id = id.clone();
    let policy = blocking(&state.storage, move |s| s.get_policy(&node_id)).await?;

    let now = chrono::Utc::now();
    let heartbeat_age = (now - node.last_heartbeat).num_milliseconds() as f64 / 1000.0;
    Ok(Json(NodeJson {
        node_id: node.node_id,
        hostname: node.hostname,
        agent_address: node.agent_address,
        labels: node.labels.clone(),
        state: node.state.to_string(),
        registered_at: node.registered_at.to_rfc3339(),
        last_heartbeat: node.last_heartbeat.to_rfc3339(),
        pacgate_version: node.pacgate_version,
        uptime_seconds: node.uptime_seconds,
        policy_hash: policy.map(|p| p.policy_hash).unwrap_or_default(),
        last_heartbeat_age_seconds: heartbeat_age,
        annotations: node.annotations,
        capabilities: node.capabilities,
    }))
}

async fn remove_node(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SuccessResponse>, AppError> {
    require_leader(&state.config)?;
    // Fetch node before removal for event emission
    let nid = id.clone();
    let node_before = blocking(&state.storage, move |s| s.get_node(&nid))
        .await
        .ok()
        .flatten();

    let node_id = id.clone();
    let removed = blocking(&state.storage, move |s| s.remove_node(&node_id)).await?;

    if removed {
        if let Some(ref node) = node_before {
            state.event_bus.emit_node(NodeEvent::Removed {
                node_id: node.node_id.clone(),
                hostname: node.hostname.clone(),
                labels: node.labels.clone(),
                timestamp: chrono::Utc::now(),
            });
        }
        record_audit(
            &state.storage,
            "api",
            "remove_node",
            "node",
            &id,
            serde_json::json!({}),
        );
    }

    Ok(Json(SuccessResponse {
        success: removed,
        message: if removed {
            "Node removed".to_string()
        } else {
            "Node not found".to_string()
        },
    }))
}

// ============================================================================
// Policy handlers
// ============================================================================

async fn get_policy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PolicyJson>, AppError> {
    let node_id = id.clone();
    let policy = blocking(&state.storage, move |s| s.get_policy(&node_id))
        .await?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, format!("No policy for node {}", id)))?;

    Ok(Json(PolicyJson {
        node_id: policy.node_id,
        rules_yaml: policy.rules_yaml,
        policy_hash: policy.policy_hash,
        deployed_at: policy.deployed_at.to_rfc3339(),
        counters_enabled: policy.counters_enabled,
        rate_limit_enabled: policy.rate_limit_enabled,
        conntrack_enabled: policy.conntrack_enabled,
        axi_enabled: policy.axi_enabled,
        ports: policy.ports,
        target: policy.target,
        dynamic: policy.dynamic,
        dynamic_entries: policy.dynamic_entries,
        width: policy.width,
        ptp: policy.ptp,
        rss: policy.rss,
        rss_queues: policy.rss_queues,
        int: policy.int,
        int_switch_id: policy.int_switch_id,
    }))
}

async fn get_policy_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<PolicyVersionJson>>, AppError> {
    let node_id = id;
    let limit = q.limit;
    let versions = blocking(&state.storage, move |s| {
        s.get_policy_history(&node_id, limit)
    })
    .await?;

    let result: Vec<PolicyVersionJson> = versions
        .into_iter()
        .map(|v| PolicyVersionJson {
            version: v.version,
            node_id: v.node_id,
            rules_yaml: v.rules_yaml,
            policy_hash: v.policy_hash,
            deployed_at: v.deployed_at.to_rfc3339(),
            counters_enabled: v.counters_enabled,
            rate_limit_enabled: v.rate_limit_enabled,
            conntrack_enabled: v.conntrack_enabled,
            axi_enabled: v.axi_enabled,
            ports: v.ports,
            target: v.target,
            dynamic: v.dynamic,
            dynamic_entries: v.dynamic_entries,
            width: v.width,
            ptp: v.ptp,
            rss: v.rss,
            rss_queues: v.rss_queues,
            int: v.int,
            int_switch_id: v.int_switch_id,
        })
        .collect();

    Ok(Json(result))
}

async fn get_deployment_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<DeploymentJson>>, AppError> {
    let node_id = id;
    let limit = q.limit;
    let records = blocking(&state.storage, move |s| s.get_deployments(&node_id, limit)).await?;

    let result: Vec<DeploymentJson> = records
        .into_iter()
        .map(|r| DeploymentJson {
            id: r.id,
            node_id: r.node_id,
            policy_version: r.policy_version,
            policy_hash: r.policy_hash,
            deployed_at: r.deployed_at.to_rfc3339(),
            result: r.result.to_string(),
            message: r.message,
        })
        .collect();

    Ok(Json(result))
}

async fn rollback_policy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RollbackRequest>,
) -> Result<Json<RollbackResponse>, AppError> {
    require_leader(&state.config)?;
    // Verify node exists
    let node_id = id.clone();
    let node = blocking(&state.storage, move |s| s.get_node(&node_id))
        .await?
        .ok_or_else(|| AppError(StatusCode::NOT_FOUND, format!("Node {} not found", id)))?;

    // Get policy history
    let node_id = id.clone();
    let versions = blocking(&state.storage, move |s| s.get_policy_history(&node_id, 10)).await?;

    if versions.is_empty() {
        return Ok(Json(RollbackResponse {
            success: false,
            message: "No policy history available".to_string(),
            version: 0,
        }));
    }

    let target = if body.target_version == 0 {
        if versions.len() < 2 {
            return Ok(Json(RollbackResponse {
                success: false,
                message: "No previous version to rollback to".to_string(),
                version: 0,
            }));
        }
        versions[1].clone()
    } else {
        versions
            .iter()
            .find(|v| v.version == body.target_version)
            .cloned()
            .ok_or_else(|| {
                AppError(
                    StatusCode::NOT_FOUND,
                    format!("Policy version {} not found", body.target_version),
                )
            })?
    };

    let target_version = target.version;
    let options = pacinet_proto::CompileOptions {
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
    };

    // Acquire deploy guard
    let nid = id.clone();
    blocking(&state.storage, move |s| s.begin_deploy(&nid)).await?;

    let outcome = deploy::deploy_to_node(
        &state.storage,
        &node,
        &target.rules_yaml,
        options,
        state.config.deploy_timeout,
        &state.tls_config,
    )
    .await;

    // Release deploy guard
    state.storage.end_deploy(&id);

    Ok(Json(RollbackResponse {
        success: outcome.success,
        message: if outcome.success {
            format!("Rolled back to version {}", target_version)
        } else {
            outcome.message
        },
        version: if outcome.success { target_version } else { 0 },
    }))
}

// ============================================================================
// Counter handlers
// ============================================================================

async fn get_node_counters(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CounterJson>, AppError> {
    let node_id = id.clone();
    let counters = blocking(&state.storage, move |s| s.get_counters(&node_id))
        .await?
        .unwrap_or_default();

    Ok(Json(CounterJson {
        node_id: id,
        counters: counters
            .into_iter()
            .map(|c| RuleCounterJson {
                rule_name: c.rule_name,
                match_count: c.match_count,
                byte_count: c.byte_count,
            })
            .collect(),
        collected_at: chrono::Utc::now().to_rfc3339(),
    }))
}

async fn get_node_flow_counters(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<FlowCounterSetJson>, AppError> {
    let node_id = id.clone();
    let flow_counters = blocking(&state.storage, move |s| s.get_flow_counters(&node_id))
        .await?
        .unwrap_or_default();

    Ok(Json(FlowCounterSetJson {
        node_id: id,
        flow_counters: flow_counters
            .into_iter()
            .map(|c| FlowCounterJson {
                flow_key: c.flow_key,
                packet_count: c.packet_count,
                byte_count: c.byte_count,
                state: c.state,
            })
            .collect(),
        collected_at: chrono::Utc::now().to_rfc3339(),
    }))
}

async fn get_aggregate_counters(
    State(state): State<AppState>,
    Query(q): Query<LabelQuery>,
) -> Result<Json<Vec<NodeCounterSetJson>>, AppError> {
    let label_filter = parse_label_filter(&q.label);
    let nodes = blocking(&state.storage, move |s| s.list_nodes(&label_filter)).await?;

    let mut result = Vec::new();
    for node in &nodes {
        let nid = node.node_id.clone();
        if let Some(counters) = blocking(&state.storage, move |s| s.get_counters(&nid)).await? {
            result.push(NodeCounterSetJson {
                node_id: node.node_id.clone(),
                counters: counters
                    .into_iter()
                    .map(|c| RuleCounterJson {
                        rule_name: c.rule_name,
                        match_count: c.match_count,
                        byte_count: c.byte_count,
                    })
                    .collect(),
                collected_at: chrono::Utc::now().to_rfc3339(),
            });
        }
    }

    Ok(Json(result))
}

async fn get_aggregate_flow_counters(
    State(state): State<AppState>,
    Query(q): Query<LabelQuery>,
) -> Result<Json<Vec<FlowCounterSetJson>>, AppError> {
    let label_filter = parse_label_filter(&q.label);
    let nodes = blocking(&state.storage, move |s| s.list_nodes(&label_filter)).await?;

    let mut result = Vec::new();
    for node in &nodes {
        let nid = node.node_id.clone();
        if let Some(flow_counters) =
            blocking(&state.storage, move |s| s.get_flow_counters(&nid)).await?
        {
            result.push(FlowCounterSetJson {
                node_id: node.node_id.clone(),
                flow_counters: flow_counters
                    .into_iter()
                    .map(|c| FlowCounterJson {
                        flow_key: c.flow_key,
                        packet_count: c.packet_count,
                        byte_count: c.byte_count,
                        state: c.state,
                    })
                    .collect(),
                collected_at: chrono::Utc::now().to_rfc3339(),
            });
        }
    }

    Ok(Json(result))
}

// ============================================================================
// Fleet handler
// ============================================================================

async fn get_fleet_status(
    State(state): State<AppState>,
    Query(q): Query<LabelQuery>,
) -> Result<Json<FleetStatusJson>, AppError> {
    let label_filter = parse_label_filter(&q.label);
    let nodes = blocking(&state.storage, move |s| s.list_nodes(&label_filter)).await?;

    let node_ids: Vec<String> = nodes.iter().map(|n| n.node_id.clone()).collect();
    let policies = blocking(&state.storage, move |s| s.get_policies_for_nodes(&node_ids)).await?;

    let total_nodes = nodes.len() as u32;
    let mut nodes_by_state: HashMap<String, u32> = HashMap::new();
    let mut summaries = Vec::new();

    let now = chrono::Utc::now();
    for node in &nodes {
        *nodes_by_state.entry(node.state.to_string()).or_insert(0) += 1;
        let policy = policies.get(&node.node_id);
        let heartbeat_age = (now - node.last_heartbeat).num_milliseconds() as f64 / 1000.0;
        summaries.push(FleetNodeJson {
            node_id: node.node_id.clone(),
            hostname: node.hostname.clone(),
            state: node.state.to_string(),
            policy_hash: policy.map(|p| p.policy_hash.clone()).unwrap_or_default(),
            uptime_seconds: node.uptime_seconds,
            last_heartbeat_age_seconds: heartbeat_age,
            last_deploy_time: policy.map(|p| p.deployed_at.to_rfc3339()),
        });
    }

    Ok(Json(FleetStatusJson {
        total_nodes,
        nodes_by_state,
        nodes: summaries,
    }))
}

// ============================================================================
// Deploy handlers
// ============================================================================

async fn deploy_policy(
    State(state): State<AppState>,
    Json(body): Json<DeployRequest>,
) -> Result<Json<DeployResponse>, AppError> {
    require_leader(&state.config)?;
    let node_id = body.node_id.clone();
    let node = blocking(&state.storage, move |s| s.get_node(&node_id))
        .await?
        .ok_or_else(|| {
            AppError(
                StatusCode::NOT_FOUND,
                format!("Node {} not found", body.node_id),
            )
        })?;

    // Dry-run mode: validate YAML, compute hash, show diff — no actual deploy
    if body.dry_run {
        let mut validation_errors = Vec::new();
        if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(&body.rules_yaml) {
            validation_errors.push(format!("YAML parse error: {}", e));
        }
        let new_hash = pacinet_core::policy_hash(&body.rules_yaml);
        let nid = body.node_id.clone();
        let current_policy = blocking(&state.storage, move |s| s.get_policy(&nid)).await?;
        let current_hash = current_policy.map(|p| p.policy_hash).unwrap_or_default();

        return Ok(Json(DeployResponse {
            success: true,
            message: "Dry-run complete".to_string(),
            warnings: vec![],
            dry_run_result: Some(DryRunResultJson {
                valid: validation_errors.is_empty(),
                validation_errors,
                target_nodes: vec![DryRunNodeJson {
                    node_id: node.node_id,
                    hostname: node.hostname,
                    current_policy_hash: current_hash.clone(),
                    new_policy_hash: new_hash.clone(),
                    policy_changed: current_hash != new_hash,
                }],
            }),
        }));
    }

    let options = pacinet_proto::CompileOptions {
        counters: body.counters,
        rate_limit: body.rate_limit,
        conntrack: body.conntrack,
        axi: body.axi,
        ports: body.ports,
        target: body.target.clone(),
        dynamic: body.dynamic,
        dynamic_entries: body.dynamic_entries,
        width: body.width,
        ptp: body.ptp,
        rss: body.rss,
        rss_queues: body.rss_queues,
        int_enabled: body.int,
        int_switch_id: body.int_switch_id,
    };

    // Acquire deploy guard
    let nid = body.node_id.clone();
    blocking(&state.storage, move |s| s.begin_deploy(&nid)).await?;

    let outcome = deploy::deploy_to_node(
        &state.storage,
        &node,
        &body.rules_yaml,
        options,
        state.config.deploy_timeout,
        &state.tls_config,
    )
    .await;

    // Release deploy guard
    state.storage.end_deploy(&body.node_id);

    record_audit(
        &state.storage,
        "api",
        "deploy",
        "node",
        &body.node_id,
        serde_json::json!({"success": outcome.success}),
    );

    Ok(Json(DeployResponse {
        success: outcome.success,
        message: outcome.message,
        warnings: outcome.warnings,
        dry_run_result: None,
    }))
}

async fn batch_deploy_policy(
    State(state): State<AppState>,
    Json(body): Json<BatchDeployRequest>,
) -> Result<Json<BatchDeployResultJson>, AppError> {
    require_leader(&state.config)?;
    let label_filter = body.label_filter.clone();
    let nodes = blocking(&state.storage, move |s| s.list_nodes(&label_filter)).await?;

    if nodes.is_empty() {
        return Ok(Json(BatchDeployResultJson {
            total_nodes: 0,
            succeeded: 0,
            failed: 0,
            results: vec![],
        }));
    }

    let options = pacinet_proto::CompileOptions {
        counters: body.counters,
        rate_limit: body.rate_limit,
        conntrack: body.conntrack,
        axi: body.axi,
        ports: body.ports,
        target: body.target.clone(),
        dynamic: body.dynamic,
        dynamic_entries: body.dynamic_entries,
        width: body.width,
        ptp: body.ptp,
        rss: body.rss,
        rss_queues: body.rss_queues,
        int_enabled: body.int,
        int_switch_id: body.int_switch_id,
    };

    let action_result = deploy::deploy_to_nodes(
        &state.storage,
        nodes.clone(),
        &body.rules_yaml,
        options,
        state.config.deploy_timeout,
        &state.tls_config,
    )
    .await;

    // Build hostname map for results
    let hostname_map: HashMap<String, String> = nodes
        .iter()
        .map(|n| (n.node_id.clone(), n.hostname.clone()))
        .collect();

    let results: Vec<NodeDeployResultJson> = action_result
        .node_results
        .into_iter()
        .map(|r| NodeDeployResultJson {
            hostname: hostname_map.get(&r.node_id).cloned().unwrap_or_default(),
            node_id: r.node_id,
            success: r.success,
            message: r.message,
        })
        .collect();

    Ok(Json(BatchDeployResultJson {
        total_nodes: action_result.total,
        succeeded: action_result.succeeded,
        failed: action_result.failed,
        results,
    }))
}

// ============================================================================
// FSM definition handlers
// ============================================================================

async fn list_fsm_definitions(
    State(state): State<AppState>,
    Query(q): Query<FsmDefQuery>,
) -> Result<Json<Vec<FsmDefSummaryJson>>, AppError> {
    let kind = if let Some(ref k) = q.kind {
        Some(
            k.parse::<pacinet_core::FsmKind>()
                .map_err(|e| AppError(StatusCode::BAD_REQUEST, e))?,
        )
    } else {
        None
    };

    let defs = blocking(&state.storage, move |s| s.list_fsm_definitions(kind)).await?;

    let result: Vec<FsmDefSummaryJson> = defs
        .into_iter()
        .map(|d| FsmDefSummaryJson {
            name: d.name,
            kind: d.kind.to_string(),
            description: d.description,
            state_count: d.states.len() as u32,
            initial_state: d.initial,
        })
        .collect();

    Ok(Json(result))
}

async fn get_fsm_definition(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<FsmDefJson>, AppError> {
    let def_name = name.clone();
    let def = blocking(&state.storage, move |s| s.get_fsm_definition(&def_name))
        .await?
        .ok_or_else(|| {
            AppError(
                StatusCode::NOT_FOUND,
                format!("FSM definition '{}' not found", name),
            )
        })?;

    let yaml = serde_yaml::to_string(&def)
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(FsmDefJson {
        name: def.name,
        kind: def.kind.to_string(),
        description: def.description,
        definition_yaml: yaml,
    }))
}

async fn create_fsm_definition(
    State(state): State<AppState>,
    Json(body): Json<CreateFsmDefRequest>,
) -> Result<Json<CreateFsmDefResponse>, AppError> {
    require_leader(&state.config)?;
    let def = pacinet_core::fsm::FsmDefinition::from_yaml(&body.definition_yaml)
        .map_err(|e| AppError(StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e)))?;
    def.validate().map_err(|e| {
        AppError(
            StatusCode::BAD_REQUEST,
            format!("Invalid definition: {}", e),
        )
    })?;
    let name = def.name.clone();

    blocking(&state.storage, move |s| s.store_fsm_definition(def)).await?;

    record_audit(
        &state.storage,
        "api",
        "create_fsm_definition",
        "fsm_definition",
        &name,
        serde_json::json!({}),
    );

    Ok(Json(CreateFsmDefResponse {
        success: true,
        name: name.clone(),
        message: format!("FSM definition '{}' created", name),
    }))
}

async fn delete_fsm_definition(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<SuccessResponse>, AppError> {
    require_leader(&state.config)?;
    let def_name = name.clone();
    let deleted = blocking(&state.storage, move |s| s.delete_fsm_definition(&def_name)).await?;

    Ok(Json(SuccessResponse {
        success: deleted,
        message: if deleted {
            format!("FSM definition '{}' deleted", name)
        } else {
            format!("FSM definition '{}' not found", name)
        },
    }))
}

// ============================================================================
// FSM instance handlers
// ============================================================================

async fn list_fsm_instances(
    State(state): State<AppState>,
    Query(q): Query<FsmInstanceQuery>,
) -> Result<Json<Vec<FsmInstanceJson>>, AppError> {
    let def_name = q.definition.clone();
    let status = if let Some(ref s) = q.status {
        Some(
            s.parse::<pacinet_core::FsmInstanceStatus>()
                .map_err(|e| AppError(StatusCode::BAD_REQUEST, e))?,
        )
    } else {
        None
    };

    let instances = blocking(&state.storage, move |s| {
        s.list_fsm_instances(def_name.as_deref(), status)
    })
    .await?;

    let result: Vec<FsmInstanceJson> = instances.iter().map(instance_to_json).collect();

    Ok(Json(result))
}

async fn get_fsm_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<FsmInstanceJson>, AppError> {
    let instance_id = id.clone();
    let instance = blocking(&state.storage, move |s| s.get_fsm_instance(&instance_id))
        .await?
        .ok_or_else(|| {
            AppError(
                StatusCode::NOT_FOUND,
                format!("FSM instance '{}' not found", id),
            )
        })?;

    Ok(Json(instance_to_json(&instance)))
}

async fn start_fsm(
    State(state): State<AppState>,
    Json(body): Json<StartFsmRequest>,
) -> Result<Json<StartFsmResponse>, AppError> {
    require_leader(&state.config)?;
    let compile_opts = Some(pacinet_core::fsm::FsmCompileOptions {
        counters: body.counters,
        rate_limit: body.rate_limit,
        conntrack: body.conntrack,
        axi: body.axi,
        ports: body.ports,
        target: body.target,
        dynamic: body.dynamic,
        dynamic_entries: body.dynamic_entries,
        width: body.width,
        ptp: body.ptp,
        rss: body.rss,
        rss_queues: body.rss_queues,
        int: body.int,
        int_switch_id: body.int_switch_id,
    });

    let result = if !body.target_label_filter.is_empty() {
        let rules = if body.rules_yaml.is_empty() {
            None
        } else {
            Some(body.rules_yaml)
        };
        state
            .fsm_engine
            .start_adaptive_instance(
                &body.definition_name,
                rules,
                compile_opts,
                &body.target_label_filter,
            )
            .await
    } else {
        state
            .fsm_engine
            .start_instance(&body.definition_name, body.rules_yaml, compile_opts)
            .await
    };

    match result {
        Ok(instance) => Ok(Json(StartFsmResponse {
            success: true,
            instance_id: instance.instance_id,
            message: "FSM instance started".to_string(),
        })),
        Err(e) => Ok(Json(StartFsmResponse {
            success: false,
            instance_id: String::new(),
            message: e.to_string(),
        })),
    }
}

async fn advance_fsm(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AdvanceFsmRequest>,
) -> Result<Json<AdvanceFsmResponse>, AppError> {
    require_leader(&state.config)?;
    match state
        .fsm_engine
        .advance_instance(&id, body.target_state)
        .await
    {
        Ok(instance) => Ok(Json(AdvanceFsmResponse {
            success: true,
            state: instance.current_state,
            message: "FSM advanced".to_string(),
        })),
        Err(e) => Ok(Json(AdvanceFsmResponse {
            success: false,
            state: String::new(),
            message: e.to_string(),
        })),
    }
}

async fn cancel_fsm(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CancelFsmRequest>,
) -> Result<Json<SuccessResponse>, AppError> {
    require_leader(&state.config)?;
    match state.fsm_engine.cancel_instance(&id, &body.reason).await {
        Ok(()) => Ok(Json(SuccessResponse {
            success: true,
            message: "FSM instance cancelled".to_string(),
        })),
        Err(e) => Ok(Json(SuccessResponse {
            success: false,
            message: e.to_string(),
        })),
    }
}

// ============================================================================
// Audit helper — fire-and-forget
// ============================================================================

fn record_audit(
    storage: &Arc<dyn Storage>,
    actor: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    details: serde_json::Value,
) {
    let entry = pacinet_core::AuditEntry {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now(),
        actor: actor.to_string(),
        action: action.to_string(),
        resource_type: resource_type.to_string(),
        resource_id: resource_id.to_string(),
        details: details.to_string(),
    };
    let s = storage.clone();
    tokio::task::spawn_blocking(move || {
        let _ = s.store_audit(entry);
    });
}

// ============================================================================
// Phase 9: Annotations handler
// ============================================================================

async fn set_annotations(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetAnnotationsRequest>,
) -> Result<Json<SuccessResponse>, AppError> {
    require_leader(&state.config)?;
    let node_id = id.clone();
    let set = body.annotations.clone();
    let remove: Vec<String> = body.remove_keys.clone();
    blocking(&state.storage, move |s| {
        s.update_annotations(&node_id, set, &remove)
    })
    .await?;

    record_audit(
        &state.storage,
        "api",
        "set_annotations",
        "node",
        &id,
        serde_json::json!({"set": body.annotations, "remove": body.remove_keys}),
    );

    Ok(Json(SuccessResponse {
        success: true,
        message: "Annotations updated".to_string(),
    }))
}

// ============================================================================
// Phase 9: Audit log handler
// ============================================================================

async fn get_audit_log(
    State(state): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntryJson>>, AppError> {
    let since = q.since.as_deref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });
    let action = q.action.clone();
    let resource_type = q.resource_type.clone();
    let resource_id = q.resource_id.clone();
    let limit = q.limit;

    let entries = blocking(&state.storage, move |s| {
        s.query_audit(
            action.as_deref(),
            resource_type.as_deref(),
            resource_id.as_deref(),
            since,
            limit,
        )
    })
    .await?;

    let result: Vec<AuditEntryJson> = entries
        .into_iter()
        .map(|e| AuditEntryJson {
            id: e.id,
            timestamp: e.timestamp.to_rfc3339(),
            actor: e.actor,
            action: e.action,
            resource_type: e.resource_type,
            resource_id: e.resource_id,
            details: serde_json::from_str(&e.details).unwrap_or(serde_json::Value::Null),
        })
        .collect();

    Ok(Json(result))
}

// ============================================================================
// Phase 9: Template handlers
// ============================================================================

async fn list_templates(
    State(state): State<AppState>,
    Query(q): Query<TemplateQuery>,
) -> Result<Json<Vec<PolicyTemplateSummaryJson>>, AppError> {
    let tag = q.tag.clone();
    let templates = blocking(&state.storage, move |s| s.list_templates(tag.as_deref())).await?;

    let result: Vec<PolicyTemplateSummaryJson> = templates
        .into_iter()
        .map(|t| PolicyTemplateSummaryJson {
            name: t.name,
            description: t.description,
            tags: t.tags,
            created_at: t.created_at.to_rfc3339(),
            updated_at: t.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(result))
}

async fn get_template(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<PolicyTemplateJson>, AppError> {
    let tpl_name = name.clone();
    let template = blocking(&state.storage, move |s| s.get_template(&tpl_name))
        .await?
        .ok_or_else(|| {
            AppError(
                StatusCode::NOT_FOUND,
                format!("Template '{}' not found", name),
            )
        })?;

    Ok(Json(PolicyTemplateJson {
        name: template.name,
        description: template.description,
        rules_yaml: template.rules_yaml,
        tags: template.tags,
        created_at: template.created_at.to_rfc3339(),
        updated_at: template.updated_at.to_rfc3339(),
    }))
}

async fn create_template(
    State(state): State<AppState>,
    Json(body): Json<CreateTemplateRequest>,
) -> Result<Json<SuccessResponse>, AppError> {
    require_leader(&state.config)?;
    let now = chrono::Utc::now();
    let template = pacinet_core::PolicyTemplate {
        name: body.name.clone(),
        description: body.description,
        rules_yaml: body.rules_yaml,
        tags: body.tags,
        created_at: now,
        updated_at: now,
    };

    blocking(&state.storage, move |s| s.store_template(template)).await?;

    record_audit(
        &state.storage,
        "api",
        "create_template",
        "template",
        &body.name,
        serde_json::json!({}),
    );

    Ok(Json(SuccessResponse {
        success: true,
        message: format!("Template '{}' created", body.name),
    }))
}

async fn delete_template(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<SuccessResponse>, AppError> {
    require_leader(&state.config)?;
    let tpl_name = name.clone();
    let deleted = blocking(&state.storage, move |s| s.delete_template(&tpl_name)).await?;

    if deleted {
        record_audit(
            &state.storage,
            "api",
            "delete_template",
            "template",
            &name,
            serde_json::json!({}),
        );
    }

    Ok(Json(SuccessResponse {
        success: deleted,
        message: if deleted {
            format!("Template '{}' deleted", name)
        } else {
            format!("Template '{}' not found", name)
        },
    }))
}

// ============================================================================
// Phase 9: Webhook history handler
// ============================================================================

async fn get_webhook_history(
    State(state): State<AppState>,
    Query(q): Query<WebhookQuery>,
) -> Result<Json<Vec<WebhookDeliveryJson>>, AppError> {
    let instance_id = q.instance_id.clone();
    let limit = q.limit;
    let deliveries = blocking(&state.storage, move |s| {
        s.query_webhook_deliveries(instance_id.as_deref(), limit)
    })
    .await?;

    let result: Vec<WebhookDeliveryJson> = deliveries
        .into_iter()
        .map(|d| WebhookDeliveryJson {
            id: d.id,
            instance_id: d.instance_id,
            url: d.url,
            method: d.method,
            status_code: d.status_code,
            success: d.success,
            duration_ms: d.duration_ms,
            error: d.error,
            attempt: d.attempt,
            timestamp: d.timestamp.to_rfc3339(),
        })
        .collect();

    Ok(Json(result))
}

// ============================================================================
// SSE endpoints
// ============================================================================

async fn sse_node_events(
    State(state): State<AppState>,
    Query(q): Query<SseNodeQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let label_filter = parse_label_filter(&q.label);
    let mut rx = state.event_bus.node_tx.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Apply label filter
                    if !label_filter.is_empty() {
                        let event_labels = event.labels();
                        let matches = label_filter.iter().all(|(k, v)| {
                            event_labels.get(k).map(|ev| ev == v).unwrap_or(false)
                        });
                        if !matches {
                            continue;
                        }
                    }
                    let json = node_event_to_json(&event);
                    if let Ok(data) = serde_json::to_string(&json) {
                        yield Ok(Event::default().data(data));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "SSE node event stream lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn sse_counter_events(
    State(state): State<AppState>,
    Query(q): Query<SseCounterQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let filter_node = q.node.clone();
    let mut rx = state.event_bus.counter_tx.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(ref id) = filter_node {
                        if event.node_id != *id {
                            continue;
                        }
                    }
                    let json = counter_event_to_json(&event);
                    if let Ok(data) = serde_json::to_string(&json) {
                        yield Ok(Event::default().data(data));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "SSE counter event stream lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn sse_fsm_events(
    State(state): State<AppState>,
    Query(q): Query<SseFsmQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let filter_instance = q.instance.clone();
    let mut rx = state.event_bus.fsm_tx.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(ref id) = filter_instance {
                        if event.instance_id() != id {
                            continue;
                        }
                    }
                    let json = fsm_event_to_json(&event);
                    if let Ok(data) = serde_json::to_string(&json) {
                        yield Ok(Event::default().data(data));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "SSE FSM event stream lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ============================================================================
// Domain → JSON conversion helpers
// ============================================================================

fn instance_to_json(instance: &pacinet_core::FsmInstance) -> FsmInstanceJson {
    FsmInstanceJson {
        instance_id: instance.instance_id.clone(),
        definition_name: instance.definition_name.clone(),
        current_state: instance.current_state.clone(),
        status: instance.status.to_string(),
        created_at: instance.created_at.to_rfc3339(),
        updated_at: instance.updated_at.to_rfc3339(),
        deployed_nodes: instance.context.deployed_nodes.len() as u32,
        failed_nodes: instance.context.failed_nodes.len() as u32,
        target_nodes: instance.context.target_nodes.len() as u32,
        history: instance
            .history
            .iter()
            .map(|t| FsmTransitionJson {
                from_state: t.from_state.clone(),
                to_state: t.to_state.clone(),
                trigger: t.trigger.to_string(),
                timestamp: t.timestamp.to_rfc3339(),
                message: t.message.clone(),
            })
            .collect(),
    }
}

#[derive(Serialize)]
struct NodeEventJson {
    event_type: String,
    node_id: String,
    hostname: String,
    labels: HashMap<String, String>,
    old_state: String,
    new_state: String,
    timestamp: String,
}

fn node_event_to_json(event: &NodeEvent) -> NodeEventJson {
    match event {
        NodeEvent::Registered {
            node_id,
            hostname,
            labels,
            timestamp,
        } => NodeEventJson {
            event_type: "registered".to_string(),
            node_id: node_id.clone(),
            hostname: hostname.clone(),
            labels: labels.clone(),
            old_state: String::new(),
            new_state: "registered".to_string(),
            timestamp: timestamp.to_rfc3339(),
        },
        NodeEvent::StateChanged {
            node_id,
            hostname,
            labels,
            old_state,
            new_state,
            timestamp,
        } => NodeEventJson {
            event_type: "state_changed".to_string(),
            node_id: node_id.clone(),
            hostname: hostname.clone(),
            labels: labels.clone(),
            old_state: old_state.clone(),
            new_state: new_state.clone(),
            timestamp: timestamp.to_rfc3339(),
        },
        NodeEvent::HeartbeatStale {
            node_id,
            hostname,
            labels,
            timestamp,
        } => NodeEventJson {
            event_type: "heartbeat_stale".to_string(),
            node_id: node_id.clone(),
            hostname: hostname.clone(),
            labels: labels.clone(),
            old_state: String::new(),
            new_state: "offline".to_string(),
            timestamp: timestamp.to_rfc3339(),
        },
        NodeEvent::Removed {
            node_id,
            hostname,
            labels,
            timestamp,
        } => NodeEventJson {
            event_type: "removed".to_string(),
            node_id: node_id.clone(),
            hostname: hostname.clone(),
            labels: labels.clone(),
            old_state: String::new(),
            new_state: String::new(),
            timestamp: timestamp.to_rfc3339(),
        },
    }
}

#[derive(Serialize)]
struct CounterEventJson {
    node_id: String,
    counters: Vec<CounterRateJson>,
    collected_at: String,
}

#[derive(Serialize)]
struct CounterRateJson {
    rule_name: String,
    match_count: u64,
    byte_count: u64,
    matches_per_second: f64,
    bytes_per_second: f64,
}

fn counter_event_to_json(event: &CounterEvent) -> CounterEventJson {
    CounterEventJson {
        node_id: event.node_id.clone(),
        counters: event
            .counters
            .iter()
            .map(|c| CounterRateJson {
                rule_name: c.rule_name.clone(),
                match_count: c.match_count,
                byte_count: c.byte_count,
                matches_per_second: c.matches_per_second,
                bytes_per_second: c.bytes_per_second,
            })
            .collect(),
        collected_at: event.collected_at.to_rfc3339(),
    }
}

#[derive(Serialize)]
struct FsmEventJson {
    event_type: String,
    instance_id: String,
    definition_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    from_state: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    to_state: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    trigger: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployed_nodes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_nodes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_nodes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    final_status: Option<String>,
    timestamp: String,
}

fn fsm_event_to_json(event: &FsmEvent) -> FsmEventJson {
    match event {
        FsmEvent::Transition {
            instance_id,
            definition_name,
            from_state,
            to_state,
            trigger,
            message,
            timestamp,
        } => FsmEventJson {
            event_type: "transition".to_string(),
            instance_id: instance_id.clone(),
            definition_name: definition_name.clone(),
            from_state: from_state.clone(),
            to_state: to_state.clone(),
            trigger: trigger.clone(),
            message: message.clone(),
            deployed_nodes: None,
            failed_nodes: None,
            target_nodes: None,
            final_status: None,
            timestamp: timestamp.to_rfc3339(),
        },
        FsmEvent::DeployProgress {
            instance_id,
            definition_name,
            deployed_nodes,
            failed_nodes,
            target_nodes,
            timestamp,
        } => FsmEventJson {
            event_type: "deploy_progress".to_string(),
            instance_id: instance_id.clone(),
            definition_name: definition_name.clone(),
            from_state: String::new(),
            to_state: String::new(),
            trigger: String::new(),
            message: String::new(),
            deployed_nodes: Some(*deployed_nodes),
            failed_nodes: Some(*failed_nodes),
            target_nodes: Some(*target_nodes),
            final_status: None,
            timestamp: timestamp.to_rfc3339(),
        },
        FsmEvent::InstanceCompleted {
            instance_id,
            definition_name,
            final_status,
            timestamp,
        } => FsmEventJson {
            event_type: "instance_completed".to_string(),
            instance_id: instance_id.clone(),
            definition_name: definition_name.clone(),
            from_state: String::new(),
            to_state: String::new(),
            trigger: String::new(),
            message: String::new(),
            deployed_nodes: None,
            failed_nodes: None,
            target_nodes: None,
            final_status: Some(final_status.clone()),
            timestamp: timestamp.to_rfc3339(),
        },
    }
}

// ============================================================================
// Health check handler (no auth required)
// ============================================================================

async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let role = if state.config.is_leader() {
        "leader"
    } else {
        "standby"
    };
    Json(HealthResponse {
        status: "ok".to_string(),
        auth_required: state.api_key.is_some(),
        role: role.to_string(),
    })
}

// ============================================================================
// Event history handler
// ============================================================================

async fn get_event_history(
    State(state): State<AppState>,
    Query(q): Query<EventHistoryQuery>,
) -> Result<Json<Vec<PersistentEventJson>>, AppError> {
    let since = q.since.as_deref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });
    let until = q.until.as_deref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });
    let event_type = q.event_type.clone();
    let source = q.source.clone();
    let limit = q.limit;

    let events = blocking(&state.storage, move |s| {
        s.query_events(
            event_type.as_deref(),
            source.as_deref(),
            since,
            until,
            limit,
        )
    })
    .await?;

    let result: Vec<PersistentEventJson> = events
        .into_iter()
        .map(|e| PersistentEventJson {
            id: e.id,
            event_type: e.event_type,
            source: e.source,
            payload: serde_json::from_str(&e.payload).unwrap_or(serde_json::Value::Null),
            timestamp: e.timestamp.to_rfc3339(),
        })
        .collect();

    Ok(Json(result))
}
