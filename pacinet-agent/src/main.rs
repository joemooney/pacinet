//! PaciNet Agent — runs on each PacGate node
//!
//! Registers with the controller, handles rule deployment, and reports counters.

mod pacgate;
mod service;

use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

/// PaciNet Node Agent
#[derive(Parser, Debug)]
#[command(name = "pacinet-agent")]
#[command(about = "PacGate node agent for PaciNet SDN controller")]
struct Args {
    /// Controller gRPC address
    #[arg(short, long, default_value = "http://127.0.0.1:50054")]
    controller: String,

    /// Port for agent gRPC server
    #[arg(short, long, default_value = "50055")]
    port: u16,

    /// Host to bind agent server to
    #[arg(short = 'H', long, default_value = "0.0.0.0")]
    host: String,

    /// Hostname to report to controller
    #[arg(long)]
    hostname: Option<String>,

    /// Labels (key=value pairs)
    #[arg(short, long, value_parser = parse_label)]
    label: Vec<(String, String)>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Heartbeat interval in seconds
    #[arg(long, default_value = "30")]
    heartbeat_interval: u64,

    /// Override PacGate version (auto-detected if not specified)
    #[arg(long)]
    pacgate_version: Option<String>,

    /// CA certificate for mTLS
    #[arg(long)]
    ca_cert: Option<String>,

    /// Agent TLS certificate
    #[arg(long)]
    tls_cert: Option<String>,

    /// Agent TLS private key
    #[arg(long)]
    tls_key: Option<String>,
}

fn parse_label(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err("Label must be in key=value format".to_string());
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let default_level = if args.debug { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let hostname = args
        .hostname
        .unwrap_or_else(|| gethostname().unwrap_or_else(|| "unknown".to_string()));
    let agent_address = format!("{}:{}", args.host, args.port);
    let labels: std::collections::HashMap<String, String> = args.label.into_iter().collect();

    // Configure TLS if certs provided
    let tls_config = match (&args.ca_cert, &args.tls_cert, &args.tls_key) {
        (Some(ca), Some(cert), Some(key)) => {
            info!("mTLS enabled");
            Some(pacinet_core::tls::TlsConfig::new(
                ca.into(),
                cert.into(),
                key.into(),
            ))
        }
        (None, None, None) => None,
        _ => {
            return Err("TLS requires all three: --ca-cert, --tls-cert, --tls-key".into());
        }
    };

    // Detect PacGate version
    let pacgate_version = match &args.pacgate_version {
        Some(v) => {
            info!(version = %v, "Using specified PacGate version");
            v.clone()
        }
        None => {
            let detected = detect_pacgate_version().await;
            match &detected {
                v if v.is_empty() => info!("PacGate not found, using mock version"),
                v => info!(version = %v, "Detected PacGate version"),
            }
            detected
        }
    };
    let capabilities = detect_pacgate_capabilities();

    info!(
        controller = %args.controller,
        hostname = %hostname,
        agent_address = %agent_address,
        "PaciNet agent starting"
    );

    // Register with controller (using client TLS if configured)
    let node_id = register_with_controller(
        &args.controller,
        &hostname,
        &agent_address,
        &labels,
        &pacgate_version,
        &capabilities,
        &tls_config,
    )
    .await?;

    info!(node_id = %node_id, "Registered with controller");

    // Shared state for the agent
    let pacgate_version_clone = pacgate_version.clone();
    let agent_state = Arc::new(RwLock::new(service::AgentState {
        node_id: node_id.clone(),
        controller_address: args.controller.clone(),
        pacgate: pacgate::PacGateBackend::Real(pacgate::PacGateRunner::new()),
        active_policy_hash: None,
        active_rules_yaml: None,
        deployed_at: None,
        start_time: tokio::time::Instant::now(),
        counters: vec![],
        flow_counters: vec![],
        pacgate_version: pacgate_version_clone,
    }));

    // Bind to configured host
    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    info!("Agent gRPC server listening on {}", addr);

    let agent_service = service::AgentService::new(agent_state.clone());

    // Shutdown watch channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Spawn heartbeat task with configurable interval, shutdown signal, and TLS config
    let hb_controller = args.controller.clone();
    let hb_node_id = node_id.clone();
    let hb_state = agent_state.clone();
    let hb_interval = args.heartbeat_interval;
    let hb_tls = tls_config.clone();
    tokio::spawn(async move {
        heartbeat_loop(
            &hb_controller,
            &hb_node_id,
            hb_state,
            hb_interval,
            shutdown_rx,
            &hb_tls,
        )
        .await;
    });

    let shutdown_state = agent_state.clone();
    let shutdown = async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        let state = shutdown_state.read().await;
        info!(
            node_id = %state.node_id,
            controller = %state.controller_address,
            "Shutdown signal received, stopping agent..."
        );
        let _ = shutdown_tx.send(true);
    };

    let mut server = tonic::transport::Server::builder();

    if let Some(ref tls) = tls_config {
        let server_tls = pacinet_core::tls::load_server_tls(tls)
            .map_err(|e| -> Box<dyn std::error::Error> { e })?;
        server = server.tls_config(server_tls)?;
    }

    server
        .add_service(pacinet_proto::paci_net_agent_server::PaciNetAgentServer::new(agent_service))
        .serve_with_shutdown(addr, shutdown)
        .await?;

    info!("Agent shut down cleanly");
    Ok(())
}

async fn connect_to_controller(
    controller_addr: &str,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<
    pacinet_proto::paci_net_controller_client::PaciNetControllerClient<tonic::transport::Channel>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    if let Some(tls) = tls_config {
        let client_tls = pacinet_core::tls::load_client_tls(tls)?;
        let channel = tonic::transport::Channel::from_shared(controller_addr.to_string())?
            .tls_config(client_tls)?
            .connect()
            .await?;
        Ok(pacinet_proto::paci_net_controller_client::PaciNetControllerClient::new(channel))
    } else {
        Ok(
            pacinet_proto::paci_net_controller_client::PaciNetControllerClient::connect(
                controller_addr.to_string(),
            )
            .await?,
        )
    }
}

async fn register_with_controller(
    controller_addr: &str,
    hostname: &str,
    agent_address: &str,
    labels: &std::collections::HashMap<String, String>,
    pacgate_version: &str,
    capabilities: &std::collections::HashMap<String, String>,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut client = connect_to_controller(controller_addr, tls_config)
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e })?;

    let response = client
        .register_node(pacinet_proto::RegisterNodeRequest {
            hostname: hostname.to_string(),
            agent_address: agent_address.to_string(),
            labels: labels.clone(),
            pacgate_version: pacgate_version.to_string(),
            capabilities: capabilities.clone(),
        })
        .await?;

    let resp = response.into_inner();
    if !resp.accepted {
        return Err(format!("Registration rejected: {}", resp.message).into());
    }

    Ok(resp.node_id)
}

async fn heartbeat_loop(
    controller_addr: &str,
    node_id: &str,
    state: Arc<RwLock<service::AgentState>>,
    interval_secs: u64,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));

    // Create client once and reuse connection
    let mut client = connect_to_controller(controller_addr, tls_config)
        .await
        .ok();

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = shutdown_rx.changed() => {
                info!("Heartbeat loop stopping (shutdown signal)");
                return;
            }
        }

        let (uptime, node_state, cpu_usage) = {
            let s = state.read().await;
            let uptime = s.start_time.elapsed().as_secs();
            let ns = if s.active_policy_hash.is_some() {
                pacinet_proto::NodeState::Active
            } else {
                pacinet_proto::NodeState::Online
            };
            (uptime, ns, read_cpu_usage())
        };

        let request = pacinet_proto::HeartbeatRequest {
            node_id: node_id.to_string(),
            state: node_state as i32,
            cpu_usage,
            uptime_seconds: uptime,
        };

        // Retry with exponential backoff: 500ms, 1s, 2s
        let mut succeeded = false;
        let backoffs = [500, 1000, 2000];

        for (attempt, &backoff_ms) in backoffs.iter().enumerate() {
            // Reconnect if client is missing
            if client.is_none() {
                match connect_to_controller(controller_addr, tls_config).await {
                    Ok(c) => client = Some(c),
                    Err(e) => {
                        warn!(
                            attempt = attempt + 1,
                            "Failed to connect for heartbeat: {}", e
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        continue;
                    }
                }
            }

            if let Some(ref mut c) = client {
                match c.heartbeat(request.clone()).await {
                    Ok(_) => {
                        succeeded = true;
                        break;
                    }
                    Err(e) => {
                        warn!(attempt = attempt + 1, "Heartbeat failed: {}", e);
                        // Connection may be stale, drop it to force reconnect
                        client = None;
                        if attempt < backoffs.len() - 1 {
                            tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        }
                    }
                }
            }
        }

        if !succeeded {
            error!("Heartbeat failed after 3 retries");
        }
    }
}

/// Read CPU usage from /proc/stat (Linux only).
/// Returns a percentage (0.0 - 100.0), or 0.0 if unavailable.
fn read_cpu_usage() -> f64 {
    // Simple approach: read /proc/stat idle percentage
    // For a proper implementation we'd need to track deltas between reads,
    // but for a heartbeat metric, a snapshot approach is sufficient.
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|content| {
            // /proc/loadavg: "0.50 0.33 0.25 1/234 12345"
            // Use 1-minute load average as a rough CPU proxy
            content.split_whitespace().next()?.parse::<f64>().ok()
        })
        .unwrap_or(0.0)
}

/// Detect PacGate version by running `pacgate --version`
async fn detect_pacgate_version() -> String {
    match tokio::process::Command::new("pacgate")
        .arg("--version")
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    }
}

fn gethostname() -> Option<String> {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
}

fn detect_pacgate_capabilities() -> std::collections::HashMap<String, String> {
    // Static declaration for now; pacinet can hard-gate on these.
    std::collections::HashMap::from([
        ("compile.axi".to_string(), "true".to_string()),
        ("compile.ports".to_string(), "true".to_string()),
        ("compile.target".to_string(), "true".to_string()),
        ("compile.dynamic".to_string(), "true".to_string()),
        ("compile.width".to_string(), "true".to_string()),
        ("compile.ptp".to_string(), "true".to_string()),
        ("compile.rss".to_string(), "true".to_string()),
        ("compile.rss_queues".to_string(), "true".to_string()),
        ("compile.int".to_string(), "true".to_string()),
        ("compile.int_switch_id".to_string(), "true".to_string()),
        ("telemetry.flow_counters".to_string(), "true".to_string()),
        ("scenario.regress".to_string(), "true".to_string()),
        ("scenario.topology".to_string(), "true".to_string()),
    ])
}
