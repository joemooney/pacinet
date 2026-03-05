use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use futures_util::StreamExt;
use pacinet_proto::paci_net_controller_client::PaciNetControllerClient;
use pacinet_proto::{
    HeartbeatRequest, NodeState, RegisterNodeRequest, ReportCountersRequest, RuleCounter,
};
use prost_types::Timestamp;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "pacinet-sim")]
#[command(about = "PaciNet simulator with standalone web UI")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value = "8090")]
    port: u16,

    #[arg(long, default_value = "http://127.0.0.1:50054")]
    pacinet_grpc: String,

    #[arg(long, default_value = "http://127.0.0.1:8081")]
    pacinet_rest: String,

    #[arg(long, env = "PACINET_API_KEY")]
    pacinet_api_key: Option<String>,

    #[arg(long, default_value = "pacgate")]
    pacgate_bin: String,

    #[arg(long, default_value = "/home/joe/ai/pacgate")]
    pacgate_repo: String,

    #[arg(short, long)]
    debug: bool,
}

#[derive(Clone)]
struct AppState {
    grpc_endpoint: String,
    rest_base: String,
    api_key: Option<String>,
    pacgate_bin: String,
    pacgate_repo: String,
    http: Client,
    simulator_nodes: Arc<RwLock<HashMap<String, SimNodeState>>>,
}

#[derive(Clone, Serialize)]
struct SimNodeState {
    node_id: String,
    hostname: String,
    agent_address: String,
    labels: HashMap<String, String>,
    pacgate_version: String,
    counters: HashMap<String, SimRuleCounter>,
}

#[derive(Clone, Serialize)]
struct SimRuleCounter {
    match_count: u64,
    byte_count: u64,
}

#[derive(Deserialize)]
struct RegisterNodeBody {
    #[serde(default = "default_hostname")]
    hostname: String,
    #[serde(default = "default_agent_address")]
    agent_address: String,
    #[serde(default)]
    labels: HashMap<String, String>,
    #[serde(default = "default_pacgate_version")]
    pacgate_version: String,
}

#[derive(Deserialize)]
struct HeartbeatBody {
    node_id: String,
    #[serde(default = "default_state")]
    state: String,
    #[serde(default = "default_cpu")]
    cpu_usage: f64,
    #[serde(default = "default_uptime")]
    uptime_seconds: u64,
}

#[derive(Deserialize)]
struct CounterDeltaBody {
    node_id: String,
    counters: Vec<CounterDelta>,
}

#[derive(Deserialize)]
struct CounterDelta {
    rule_name: String,
    #[serde(default)]
    match_delta: u64,
    #[serde(default)]
    byte_delta: u64,
}

#[derive(Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    message: String,
    data: Option<T>,
}

#[derive(Serialize)]
struct SimConfigJson {
    grpc_endpoint: String,
    rest_base: String,
    api_key_configured: bool,
    pacgate_bin: String,
    pacgate_repo: String,
}

#[derive(Deserialize)]
struct SnapshotQuery {
    #[serde(default = "default_event_limit")]
    event_limit: u32,
}

#[derive(Serialize)]
struct PacinetSnapshot {
    health: serde_json::Value,
    fleet: serde_json::Value,
    nodes: serde_json::Value,
    event_history: serde_json::Value,
}

#[derive(Serialize)]
struct ScenarioResult {
    scenario: String,
    steps: Vec<ScenarioStep>,
    pacinet_snapshot: Option<PacinetSnapshot>,
}

#[derive(Deserialize)]
struct PacgateRegressBody {
    scenario_path: String,
    #[serde(default = "default_regress_count")]
    count: u64,
}

#[derive(Deserialize)]
struct PacgateTopologyBody {
    scenario_path: String,
}

#[derive(Serialize)]
struct ScenarioStep {
    step: String,
    ok: bool,
    detail: serde_json::Value,
}

fn default_hostname() -> String {
    "sim-node".to_string()
}

fn default_agent_address() -> String {
    "127.0.0.1:55000".to_string()
}

fn default_pacgate_version() -> String {
    "sim-1.0.0".to_string()
}

fn default_state() -> String {
    "online".to_string()
}

fn default_cpu() -> f64 {
    0.08
}

fn default_uptime() -> u64 {
    1
}

fn default_event_limit() -> u32 {
    30
}

fn default_regress_count() -> u64 {
    1000
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let default_level = if args.debug { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let state = AppState {
        grpc_endpoint: args.pacinet_grpc,
        rest_base: args.pacinet_rest,
        api_key: args.pacinet_api_key,
        pacgate_bin: args.pacgate_bin,
        pacgate_repo: args.pacgate_repo,
        http: Client::new(),
        simulator_nodes: Arc::new(RwLock::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/api/sim/config", get(get_config))
        .route("/api/sim/nodes", get(list_sim_nodes))
        .route("/api/sim/register-node", post(register_node))
        .route("/api/sim/heartbeat", post(send_heartbeat))
        .route("/api/sim/counters", post(report_counters))
        .route("/api/sim/scenario/basic", post(run_basic_scenario))
        .route("/api/sim/scenario/burst", post(run_burst_scenario))
        .route("/api/sim/scenario/flap", post(run_flap_scenario))
        .route(
            "/api/sim/scenario/canary-traffic",
            post(run_canary_traffic_scenario),
        )
        .route("/api/pacinet/snapshot", get(get_pacinet_snapshot))
        .route("/api/pacinet/stream/nodes", get(stream_node_events))
        .route("/api/pacinet/stream/counters", get(stream_counter_events))
        .route("/api/pacinet/stream/fsm", get(stream_fsm_events))
        .route("/api/pacgate/scenario/regress", post(run_pacgate_regress))
        .route("/api/pacgate/scenario/topology", post(run_pacgate_topology))
        .with_state(state);

    let addr = format!("{}:{}", args.host, args.port);
    info!("pacinet-sim listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(include_str!("ui.html"))
}

async fn get_config(State(state): State<AppState>) -> Json<SimConfigJson> {
    Json(SimConfigJson {
        grpc_endpoint: state.grpc_endpoint,
        rest_base: state.rest_base,
        api_key_configured: state.api_key.is_some(),
        pacgate_bin: state.pacgate_bin,
        pacgate_repo: state.pacgate_repo,
    })
}

async fn list_sim_nodes(State(state): State<AppState>) -> Json<Vec<SimNodeState>> {
    let nodes = state
        .simulator_nodes
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    Json(nodes)
}

async fn register_node(
    State(state): State<AppState>,
    Json(body): Json<RegisterNodeBody>,
) -> impl IntoResponse {
    let mut client = match PaciNetControllerClient::connect(state.grpc_endpoint.clone()).await {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to pacinet gRPC: {}", e),
            );
        }
    };

    let request = RegisterNodeRequest {
        hostname: body.hostname.clone(),
        agent_address: body.agent_address.clone(),
        labels: body.labels.clone(),
        pacgate_version: body.pacgate_version.clone(),
        capabilities: HashMap::new(),
    };

    match client.register_node(request).await {
        Ok(resp) => {
            let reg = resp.into_inner();
            let node = SimNodeState {
                node_id: reg.node_id.clone(),
                hostname: body.hostname,
                agent_address: body.agent_address,
                labels: body.labels,
                pacgate_version: body.pacgate_version,
                counters: HashMap::new(),
            };
            state
                .simulator_nodes
                .write()
                .await
                .insert(reg.node_id.clone(), node.clone());

            json_ok(
                "Simulator node registered in pacinet",
                serde_json::json!({
                    "node_id": reg.node_id,
                    "accepted": reg.accepted,
                    "message": reg.message,
                    "node": node,
                }),
            )
        }
        Err(e) => error_response(
            StatusCode::BAD_GATEWAY,
            format!("register_node failed: {}", e.message()),
        ),
    }
}

async fn send_heartbeat(
    State(state): State<AppState>,
    Json(body): Json<HeartbeatBody>,
) -> impl IntoResponse {
    let node_state = match parse_node_state(&body.state) {
        Some(s) => s as i32,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "Invalid state. Use one of: registered, online, deploying, active, error, offline"
                    .to_string(),
            );
        }
    };

    let mut client = match PaciNetControllerClient::connect(state.grpc_endpoint.clone()).await {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to pacinet gRPC: {}", e),
            );
        }
    };

    let request = HeartbeatRequest {
        node_id: body.node_id.clone(),
        state: node_state,
        cpu_usage: body.cpu_usage,
        uptime_seconds: body.uptime_seconds,
    };

    match client.heartbeat(request).await {
        Ok(resp) => json_ok(
            "Heartbeat sent",
            serde_json::json!({
                "node_id": body.node_id,
                "acknowledged": resp.into_inner().acknowledged,
            }),
        ),
        Err(e) => error_response(
            StatusCode::BAD_GATEWAY,
            format!("heartbeat failed: {}", e.message()),
        ),
    }
}

async fn report_counters(
    State(state): State<AppState>,
    Json(body): Json<CounterDeltaBody>,
) -> impl IntoResponse {
    if body.counters.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "At least one counter is required".into(),
        );
    }

    let report = {
        let mut nodes = state.simulator_nodes.write().await;
        let node = match nodes.get_mut(&body.node_id) {
            Some(node) => node,
            None => {
                return error_response(
                    StatusCode::NOT_FOUND,
                    format!("Unknown simulator node: {}", body.node_id),
                );
            }
        };

        for delta in &body.counters {
            let entry = node
                .counters
                .entry(delta.rule_name.clone())
                .or_insert(SimRuleCounter {
                    match_count: 0,
                    byte_count: 0,
                });
            entry.match_count = entry.match_count.saturating_add(delta.match_delta);
            entry.byte_count = entry.byte_count.saturating_add(delta.byte_delta);
        }

        node.counters
            .iter()
            .map(|(rule_name, counter)| RuleCounter {
                rule_name: rule_name.clone(),
                match_count: counter.match_count,
                byte_count: counter.byte_count,
            })
            .collect::<Vec<_>>()
    };

    let now = chrono::Utc::now();
    let collected_at = Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    };

    let mut client = match PaciNetControllerClient::connect(state.grpc_endpoint.clone()).await {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to pacinet gRPC: {}", e),
            );
        }
    };

    let request = ReportCountersRequest {
        node_id: body.node_id.clone(),
        counters: report,
        collected_at: Some(collected_at),
        flow_counters: vec![],
    };

    match client.report_counters(request).await {
        Ok(resp) => {
            let snapshot = state
                .simulator_nodes
                .read()
                .await
                .get(&body.node_id)
                .map(|n| n.counters.clone())
                .unwrap_or_default();
            json_ok(
                "Counters reported",
                serde_json::json!({
                    "node_id": body.node_id,
                    "acknowledged": resp.into_inner().acknowledged,
                    "current_counters": snapshot,
                }),
            )
        }
        Err(e) => error_response(
            StatusCode::BAD_GATEWAY,
            format!("report_counters failed: {}", e.message()),
        ),
    }
}

async fn run_basic_scenario(State(state): State<AppState>) -> impl IntoResponse {
    let suffix = chrono::Utc::now().timestamp_millis();
    let hostname = format!("sim-basic-{}", suffix);
    let agent_address = format!("127.0.0.1:{}", 56000 + (suffix % 1000));

    let mut steps = Vec::new();

    let mut grpc = match PaciNetControllerClient::connect(state.grpc_endpoint.clone()).await {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to pacinet gRPC: {}", e),
            );
        }
    };

    let register = grpc
        .register_node(RegisterNodeRequest {
            hostname: hostname.clone(),
            agent_address: agent_address.clone(),
            labels: HashMap::from([
                ("sim".to_string(), "true".to_string()),
                ("scenario".to_string(), "basic".to_string()),
            ]),
            pacgate_version: "sim-1.0.0".to_string(),
            capabilities: HashMap::new(),
        })
        .await;

    let node_id = match register {
        Ok(resp) => {
            let payload = resp.into_inner();
            let id = payload.node_id.clone();
            state.simulator_nodes.write().await.insert(
                id.clone(),
                SimNodeState {
                    node_id: id.clone(),
                    hostname,
                    agent_address,
                    labels: HashMap::from([
                        ("sim".to_string(), "true".to_string()),
                        ("scenario".to_string(), "basic".to_string()),
                    ]),
                    pacgate_version: "sim-1.0.0".to_string(),
                    counters: HashMap::new(),
                },
            );
            steps.push(ScenarioStep {
                step: "register_node".to_string(),
                ok: true,
                detail: serde_json::json!({
                    "node_id": payload.node_id,
                    "accepted": payload.accepted,
                    "message": payload.message,
                }),
            });
            id
        }
        Err(e) => {
            steps.push(ScenarioStep {
                step: "register_node".to_string(),
                ok: false,
                detail: serde_json::json!({"error": e.message()}),
            });
            return (
                StatusCode::BAD_GATEWAY,
                Json(ApiResponse {
                    success: false,
                    message: "Scenario failed at register_node".to_string(),
                    data: Some(ScenarioResult {
                        scenario: "basic".to_string(),
                        steps,
                        pacinet_snapshot: None,
                    }),
                }),
            )
                .into_response();
        }
    };

    let heartbeat = grpc
        .heartbeat(HeartbeatRequest {
            node_id: node_id.clone(),
            state: NodeState::Online as i32,
            cpu_usage: 0.12,
            uptime_seconds: 5,
        })
        .await;

    match heartbeat {
        Ok(resp) => {
            let hb = resp.into_inner();
            steps.push(ScenarioStep {
                step: "heartbeat".to_string(),
                ok: true,
                detail: serde_json::json!({
                    "acknowledged": hb.acknowledged,
                }),
            })
        }
        Err(e) => steps.push(ScenarioStep {
            step: "heartbeat".to_string(),
            ok: false,
            detail: serde_json::json!({"error": e.message()}),
        }),
    }

    let c1 = ReportCountersRequest {
        node_id: node_id.clone(),
        counters: vec![RuleCounter {
            rule_name: "allow-web".to_string(),
            match_count: 100,
            byte_count: 12000,
        }],
        collected_at: Some(current_timestamp()),
        flow_counters: vec![],
    };
    let counter_1 = grpc.report_counters(c1).await;
    match counter_1 {
        Ok(resp) => {
            let c = resp.into_inner();
            steps.push(ScenarioStep {
                step: "report_counters_1".to_string(),
                ok: true,
                detail: serde_json::json!({
                    "acknowledged": c.acknowledged,
                }),
            })
        }
        Err(e) => steps.push(ScenarioStep {
            step: "report_counters_1".to_string(),
            ok: false,
            detail: serde_json::json!({"error": e.message()}),
        }),
    }

    tokio::time::sleep(std::time::Duration::from_millis(350)).await;

    let c2 = ReportCountersRequest {
        node_id: node_id.clone(),
        counters: vec![RuleCounter {
            rule_name: "allow-web".to_string(),
            match_count: 450,
            byte_count: 51000,
        }],
        collected_at: Some(current_timestamp()),
        flow_counters: vec![],
    };
    let counter_2 = grpc.report_counters(c2).await;
    match counter_2 {
        Ok(resp) => {
            let c = resp.into_inner();
            steps.push(ScenarioStep {
                step: "report_counters_2".to_string(),
                ok: true,
                detail: serde_json::json!({
                    "acknowledged": c.acknowledged,
                }),
            })
        }
        Err(e) => steps.push(ScenarioStep {
            step: "report_counters_2".to_string(),
            ok: false,
            detail: serde_json::json!({"error": e.message()}),
        }),
    }

    let snapshot = fetch_snapshot(&state, 30).await.ok();

    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Basic scenario executed".to_string(),
            data: Some(ScenarioResult {
                scenario: "basic".to_string(),
                steps,
                pacinet_snapshot: snapshot,
            }),
        }),
    )
        .into_response()
}

async fn run_burst_scenario(State(state): State<AppState>) -> impl IntoResponse {
    let mut steps = Vec::new();
    let mut grpc = match PaciNetControllerClient::connect(state.grpc_endpoint.clone()).await {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to pacinet gRPC: {}", e),
            );
        }
    };

    let base = chrono::Utc::now().timestamp_millis();
    let mut created = Vec::new();

    for i in 0..3 {
        let hostname = format!("sim-burst-{}-{}", base, i + 1);
        let agent_address = format!("127.0.0.1:{}", 57000 + i);
        let labels = HashMap::from([
            ("sim".to_string(), "true".to_string()),
            ("scenario".to_string(), "burst".to_string()),
            ("group".to_string(), "load".to_string()),
        ]);
        let register = grpc
            .register_node(RegisterNodeRequest {
                hostname: hostname.clone(),
                agent_address: agent_address.clone(),
                labels: labels.clone(),
                pacgate_version: "sim-1.0.0".to_string(),
                capabilities: HashMap::new(),
            })
            .await;

        match register {
            Ok(resp) => {
                let payload = resp.into_inner();
                let node_id = payload.node_id.clone();
                state.simulator_nodes.write().await.insert(
                    node_id.clone(),
                    SimNodeState {
                        node_id: node_id.clone(),
                        hostname,
                        agent_address,
                        labels,
                        pacgate_version: "sim-1.0.0".to_string(),
                        counters: HashMap::new(),
                    },
                );
                created.push(node_id.clone());
                steps.push(ScenarioStep {
                    step: format!("register_node_{}", i + 1),
                    ok: true,
                    detail: serde_json::json!({ "node_id": node_id }),
                });
            }
            Err(e) => {
                steps.push(ScenarioStep {
                    step: format!("register_node_{}", i + 1),
                    ok: false,
                    detail: serde_json::json!({ "error": e.message() }),
                });
            }
        }
    }

    for node_id in &created {
        let hb = grpc
            .heartbeat(HeartbeatRequest {
                node_id: node_id.clone(),
                state: NodeState::Online as i32,
                cpu_usage: 0.18,
                uptime_seconds: 20,
            })
            .await;
        steps.push(ScenarioStep {
            step: format!("heartbeat_{}", node_id),
            ok: hb.is_ok(),
            detail: hb
                .map(|r| serde_json::json!({ "acknowledged": r.into_inner().acknowledged }))
                .unwrap_or_else(|e| serde_json::json!({ "error": e.message() })),
        });
    }

    for round in 1..=3 {
        for (idx, node_id) in created.iter().enumerate() {
            let factor = (idx as u64 + 1) * round as u64;
            let req = ReportCountersRequest {
                node_id: node_id.clone(),
                counters: vec![
                    RuleCounter {
                        rule_name: "allow-web".to_string(),
                        match_count: factor * 400,
                        byte_count: factor * 48_000,
                    },
                    RuleCounter {
                        rule_name: "allow-api".to_string(),
                        match_count: factor * 210,
                        byte_count: factor * 31_000,
                    },
                ],
                collected_at: Some(current_timestamp()),
                flow_counters: vec![],
            };
            let result = grpc.report_counters(req).await;
            steps.push(ScenarioStep {
                step: format!("counters_round{}_{}", round, node_id),
                ok: result.is_ok(),
                detail: result
                    .map(|r| serde_json::json!({ "acknowledged": r.into_inner().acknowledged }))
                    .unwrap_or_else(|e| serde_json::json!({ "error": e.message() })),
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    let snapshot = fetch_snapshot(&state, 40).await.ok();
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Burst scenario executed".to_string(),
            data: Some(ScenarioResult {
                scenario: "burst".to_string(),
                steps,
                pacinet_snapshot: snapshot,
            }),
        }),
    )
        .into_response()
}

async fn run_flap_scenario(State(state): State<AppState>) -> impl IntoResponse {
    let mut steps = Vec::new();
    let mut grpc = match PaciNetControllerClient::connect(state.grpc_endpoint.clone()).await {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to pacinet gRPC: {}", e),
            );
        }
    };

    let suffix = chrono::Utc::now().timestamp_millis();
    let hostname = format!("sim-flap-{}", suffix);
    let agent_address = format!("127.0.0.1:{}", 57500 + (suffix % 400));
    let labels = HashMap::from([
        ("sim".to_string(), "true".to_string()),
        ("scenario".to_string(), "flap".to_string()),
        ("role".to_string(), "edge".to_string()),
    ]);

    let register = grpc
        .register_node(RegisterNodeRequest {
            hostname: hostname.clone(),
            agent_address: agent_address.clone(),
            labels: labels.clone(),
            pacgate_version: "sim-1.0.0".to_string(),
            capabilities: HashMap::new(),
        })
        .await;

    let node_id = match register {
        Ok(resp) => {
            let reg = resp.into_inner();
            let node_id = reg.node_id.clone();
            state.simulator_nodes.write().await.insert(
                node_id.clone(),
                SimNodeState {
                    node_id: node_id.clone(),
                    hostname,
                    agent_address,
                    labels,
                    pacgate_version: "sim-1.0.0".to_string(),
                    counters: HashMap::new(),
                },
            );
            steps.push(ScenarioStep {
                step: "register_node".to_string(),
                ok: true,
                detail: serde_json::json!({ "node_id": node_id }),
            });
            node_id
        }
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(ApiResponse::<serde_json::Value> {
                    success: false,
                    message: format!("register_node failed: {}", e.message()),
                    data: None,
                }),
            )
                .into_response();
        }
    };

    let states = [
        NodeState::Online,
        NodeState::Offline,
        NodeState::Online,
        NodeState::Error,
        NodeState::Active,
    ];

    for (idx, state_value) in states.iter().enumerate() {
        let hb = grpc
            .heartbeat(HeartbeatRequest {
                node_id: node_id.clone(),
                state: *state_value as i32,
                cpu_usage: if matches!(state_value, NodeState::Error) {
                    0.98
                } else {
                    0.22
                },
                uptime_seconds: 15 + idx as u64 * 5,
            })
            .await;
        steps.push(ScenarioStep {
            step: format!("heartbeat_state_{}", idx + 1),
            ok: hb.is_ok(),
            detail: hb
                .map(|r| serde_json::json!({ "acknowledged": r.into_inner().acknowledged }))
                .unwrap_or_else(|e| serde_json::json!({ "error": e.message() })),
        });
        tokio::time::sleep(std::time::Duration::from_millis(220)).await;
    }

    let snapshot = fetch_snapshot(&state, 40).await.ok();
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Flap scenario executed".to_string(),
            data: Some(ScenarioResult {
                scenario: "flap".to_string(),
                steps,
                pacinet_snapshot: snapshot,
            }),
        }),
    )
        .into_response()
}

async fn run_canary_traffic_scenario(State(state): State<AppState>) -> impl IntoResponse {
    let mut steps = Vec::new();
    let mut grpc = match PaciNetControllerClient::connect(state.grpc_endpoint.clone()).await {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                format!("Failed to connect to pacinet gRPC: {}", e),
            );
        }
    };

    let base = chrono::Utc::now().timestamp_millis();
    let mut nodes = Vec::new();
    let plan = [("canary", 1_u64), ("stable-a", 4_u64), ("stable-b", 5_u64)];

    for (idx, (role, weight)) in plan.iter().enumerate() {
        let labels = HashMap::from([
            ("sim".to_string(), "true".to_string()),
            ("scenario".to_string(), "canary_traffic".to_string()),
            (
                "tier".to_string(),
                if *role == "canary" {
                    "canary".to_string()
                } else {
                    "stable".to_string()
                },
            ),
            ("role".to_string(), role.to_string()),
        ]);
        let req = RegisterNodeRequest {
            hostname: format!("sim-canary-{}-{}", role, base),
            agent_address: format!("127.0.0.1:{}", 57800 + idx),
            labels: labels.clone(),
            pacgate_version: "sim-1.0.0".to_string(),
            capabilities: HashMap::new(),
        };
        match grpc.register_node(req).await {
            Ok(resp) => {
                let reg = resp.into_inner();
                state.simulator_nodes.write().await.insert(
                    reg.node_id.clone(),
                    SimNodeState {
                        node_id: reg.node_id.clone(),
                        hostname: format!("sim-canary-{}-{}", role, base),
                        agent_address: format!("127.0.0.1:{}", 57800 + idx),
                        labels,
                        pacgate_version: "sim-1.0.0".to_string(),
                        counters: HashMap::new(),
                    },
                );
                nodes.push((reg.node_id.clone(), *role, *weight));
                steps.push(ScenarioStep {
                    step: format!("register_{}", role),
                    ok: true,
                    detail: serde_json::json!({ "node_id": reg.node_id }),
                });
            }
            Err(e) => steps.push(ScenarioStep {
                step: format!("register_{}", role),
                ok: false,
                detail: serde_json::json!({ "error": e.message() }),
            }),
        }
    }

    for (node_id, role, weight) in &nodes {
        let hb = grpc
            .heartbeat(HeartbeatRequest {
                node_id: node_id.clone(),
                state: NodeState::Active as i32,
                cpu_usage: 0.2,
                uptime_seconds: 40,
            })
            .await;
        steps.push(ScenarioStep {
            step: format!("heartbeat_{}", role),
            ok: hb.is_ok(),
            detail: hb
                .map(|r| serde_json::json!({ "acknowledged": r.into_inner().acknowledged }))
                .unwrap_or_else(|e| serde_json::json!({ "error": e.message() })),
        });

        let counters = ReportCountersRequest {
            node_id: node_id.clone(),
            counters: vec![
                RuleCounter {
                    rule_name: "requests".to_string(),
                    match_count: 900 * weight,
                    byte_count: 120_000 * weight,
                },
                RuleCounter {
                    rule_name: "errors".to_string(),
                    match_count: if *role == "canary" { 15 } else { 4 * weight },
                    byte_count: if *role == "canary" {
                        1_900
                    } else {
                        500 * weight
                    },
                },
            ],
            collected_at: Some(current_timestamp()),
            flow_counters: vec![],
        };
        let report = grpc.report_counters(counters).await;
        steps.push(ScenarioStep {
            step: format!("counters_{}", role),
            ok: report.is_ok(),
            detail: report
                .map(|r| serde_json::json!({ "acknowledged": r.into_inner().acknowledged }))
                .unwrap_or_else(|e| serde_json::json!({ "error": e.message() })),
        });
    }

    let snapshot = fetch_snapshot(&state, 50).await.ok();
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "Canary traffic scenario executed".to_string(),
            data: Some(ScenarioResult {
                scenario: "canary_traffic".to_string(),
                steps,
                pacinet_snapshot: snapshot,
            }),
        }),
    )
        .into_response()
}

async fn stream_node_events(State(state): State<AppState>) -> impl IntoResponse {
    proxy_pacinet_sse(state, "/api/events/nodes").await
}

async fn stream_counter_events(State(state): State<AppState>) -> impl IntoResponse {
    proxy_pacinet_sse(state, "/api/events/counters").await
}

async fn stream_fsm_events(State(state): State<AppState>) -> impl IntoResponse {
    proxy_pacinet_sse(state, "/api/events/fsm").await
}

async fn run_pacgate_regress(
    State(state): State<AppState>,
    Json(body): Json<PacgateRegressBody>,
) -> impl IntoResponse {
    run_pacgate_command(
        &state,
        &[
            "regress",
            "--scenario",
            &body.scenario_path,
            "--count",
            &body.count.to_string(),
            "--json",
        ],
    )
    .await
}

async fn run_pacgate_topology(
    State(state): State<AppState>,
    Json(body): Json<PacgateTopologyBody>,
) -> impl IntoResponse {
    run_pacgate_command(
        &state,
        &["topology", "--scenario", &body.scenario_path, "--json"],
    )
    .await
}

async fn run_pacgate_command(state: &AppState, args: &[&str]) -> axum::response::Response {
    let mut cmd = Command::new(&state.pacgate_bin);
    cmd.current_dir(&state.pacgate_repo);
    for arg in args {
        cmd.arg(arg);
    }

    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                format!(
                    "Failed to execute pacgate command (bin='{}'): {}",
                    state.pacgate_bin, e
                ),
            );
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let payload = if stdout.trim().is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(v) => v,
            Err(_) => serde_json::json!({ "raw_stdout": stdout }),
        }
    };

    if output.status.success() {
        json_ok("PacGate scenario command executed", payload)
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                message: "PacGate scenario command failed".to_string(),
                data: Some(serde_json::json!({
                    "status": output.status.code(),
                    "stdout": payload,
                    "stderr": stderr,
                })),
            }),
        )
            .into_response()
    }
}

async fn proxy_pacinet_sse(state: AppState, path: &str) -> axum::response::Response {
    let url = format!("{}{}", state.rest_base.trim_end_matches('/'), path);
    let mut req = state
        .http
        .get(url)
        .header(header::ACCEPT, "text/event-stream");

    if let Some(ref key) = state.api_key {
        req = req.header(header::AUTHORIZATION, format!("Bearer {}", key));
    }

    let upstream = match req.send().await {
        Ok(resp) => resp,
        Err(e) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                format!("Failed to open upstream SSE {}: {}", path, e),
            );
        }
    };

    if !upstream.status().is_success() {
        let status = upstream.status();
        let body = upstream
            .text()
            .await
            .unwrap_or_else(|_| "<unable to read body>".to_string());
        return error_response(
            StatusCode::BAD_GATEWAY,
            format!("Upstream SSE {} failed with {}: {}", path, status, body),
        );
    }

    let mut bytes = upstream.bytes_stream();
    let stream = async_stream::stream! {
        let mut buffer = String::new();
        let mut data_lines: Vec<String> = Vec::new();
        let mut event_name: Option<String> = None;

        while let Some(chunk) = bytes.next().await {
            match chunk {
                Ok(chunk) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some(newline) = buffer.find('\n') {
                        let line = buffer[..newline].trim_end_matches('\r').to_string();
                        buffer = buffer[newline + 1..].to_string();

                        if line.is_empty() {
                            if !data_lines.is_empty() || event_name.is_some() {
                                let data = data_lines.join("\n");
                                let mut ev = Event::default().data(data);
                                if let Some(name) = event_name.take() {
                                    ev = ev.event(name);
                                }
                                data_lines.clear();
                                yield Ok::<Event, Infallible>(ev);
                            }
                            continue;
                        }

                        if let Some(rest) = line.strip_prefix("event:") {
                            event_name = Some(rest.trim().to_string());
                        } else if let Some(rest) = line.strip_prefix("data:") {
                            data_lines.push(rest.trim_start().to_string());
                        }
                    }
                }
                Err(e) => {
                    yield Ok::<Event, Infallible>(Event::default().event("proxy-error").data(
                        serde_json::json!({ "error": e.to_string() }).to_string(),
                    ));
                    break;
                }
            }
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
        .into_response()
}

async fn get_pacinet_snapshot(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<SnapshotQuery>,
) -> impl IntoResponse {
    match fetch_snapshot(&state, q.event_limit).await {
        Ok(snapshot) => json_ok("Fetched pacinet snapshot", snapshot),
        Err(e) => error_response(StatusCode::BAD_GATEWAY, e),
    }
}

async fn fetch_snapshot(state: &AppState, event_limit: u32) -> Result<PacinetSnapshot, String> {
    let health = get_pacinet_json(state, "/api/health").await?;
    let fleet = get_pacinet_json(state, "/api/fleet").await?;
    let nodes = get_pacinet_json(state, "/api/nodes").await?;
    let history_path = format!("/api/events/history?limit={}", event_limit);
    let event_history = get_pacinet_json(state, &history_path).await?;

    Ok(PacinetSnapshot {
        health,
        fleet,
        nodes,
        event_history,
    })
}

async fn get_pacinet_json(state: &AppState, path: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", state.rest_base.trim_end_matches('/'), path);
    let mut req = state.http.get(&url);

    if let Some(ref key) = state.api_key {
        req = req.header(header::AUTHORIZATION, format!("Bearer {}", key));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Request failed for {}: {}", path, e))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<unable to read body>".to_string());
        return Err(format!("{} returned {}: {}", path, status, body));
    }

    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Invalid JSON from {}: {}", path, e))
}

fn parse_node_state(value: &str) -> Option<NodeState> {
    match value.to_ascii_lowercase().as_str() {
        "registered" => Some(NodeState::Registered),
        "online" => Some(NodeState::Online),
        "deploying" => Some(NodeState::Deploying),
        "active" => Some(NodeState::Active),
        "error" => Some(NodeState::Error),
        "offline" => Some(NodeState::Offline),
        _ => None,
    }
}

fn current_timestamp() -> Timestamp {
    let now = chrono::Utc::now();
    Timestamp {
        seconds: now.timestamp(),
        nanos: now.timestamp_subsec_nanos() as i32,
    }
}

fn json_ok<T: Serialize>(message: &str, data: T) -> axum::response::Response {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: message.to_string(),
            data: Some(data),
        }),
    )
        .into_response()
}

fn error_response(status: StatusCode, message: String) -> axum::response::Response {
    (
        status,
        Json(ApiResponse::<serde_json::Value> {
            success: false,
            message,
            data: None,
        }),
    )
        .into_response()
}
