//! PaciNet Controller — SDN controller for PacGate nodes
//!
//! Manages node registration, policy deployment, and counter aggregation.

use pacinet_server::config::ControllerConfig;
use pacinet_server::service;
use pacinet_server::storage;

use clap::Parser;
use pacinet_core::Storage;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

/// PaciNet SDN Controller
#[derive(Parser, Debug)]
#[command(name = "pacinet-server")]
#[command(about = "SDN controller for PacGate FPGA packet filter nodes")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "50054")]
    port: u16,

    /// Host to bind to
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// SQLite database path (default: in-memory)
    #[arg(long)]
    db: Option<String>,

    /// Deploy timeout in seconds
    #[arg(long, default_value = "30")]
    deploy_timeout: u64,

    /// Expected heartbeat interval in seconds
    #[arg(long, default_value = "30")]
    heartbeat_expect_interval: u64,

    /// Number of missed heartbeats before marking node offline
    #[arg(long, default_value = "3")]
    heartbeat_miss_threshold: u32,

    /// Prometheus metrics HTTP port (0 to disable)
    #[arg(long, default_value = "9090")]
    metrics_port: u16,

    /// Counter snapshot retention in seconds (default: 3600 = 1h)
    #[arg(long, default_value = "3600")]
    counter_retention_secs: u64,

    /// Max counter snapshots per node (default: 120)
    #[arg(long, default_value = "120")]
    counter_max_per_node: usize,

    /// CA certificate for mTLS
    #[arg(long)]
    ca_cert: Option<String>,

    /// Server TLS certificate
    #[arg(long)]
    tls_cert: Option<String>,

    /// Server TLS private key
    #[arg(long)]
    tls_key: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let default_level = if args.debug { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = ControllerConfig {
        deploy_timeout: std::time::Duration::from_secs(args.deploy_timeout),
        heartbeat_expect_interval: std::time::Duration::from_secs(args.heartbeat_expect_interval),
        heartbeat_miss_threshold: args.heartbeat_miss_threshold,
        start_time: tokio::time::Instant::now(),
        counter_snapshot_retention: std::time::Duration::from_secs(args.counter_retention_secs),
        counter_snapshot_max_per_node: args.counter_max_per_node,
    };

    // Create storage backend
    let storage: Arc<dyn Storage> = match &args.db {
        Some(path) => {
            info!(path = %path, "Using SQLite storage");
            Arc::new(storage::SqliteStorage::open(path)?)
        }
        None => {
            info!("Using in-memory storage");
            Arc::new(storage::MemoryStorage::new())
        }
    };

    // Start Prometheus metrics endpoint
    if args.metrics_port > 0 {
        pacinet_server::metrics::install_metrics(args.metrics_port)?;
    }

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
        (None, None, None) => {
            info!("Running without TLS (plaintext)");
            None
        }
        _ => {
            return Err("TLS requires all three: --ca-cert, --tls-cert, --tls-key".into());
        }
    };

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    info!("PaciNet controller starting on {}", addr);

    // Create counter snapshot cache
    let counter_cache = Arc::new(pacinet_server::counter_cache::CounterSnapshotCache::new(
        chrono::Duration::seconds(config.counter_snapshot_retention.as_secs() as i64),
        config.counter_snapshot_max_per_node,
    ));
    info!(
        retention_secs = config.counter_snapshot_retention.as_secs(),
        max_per_node = config.counter_snapshot_max_per_node,
        "Counter snapshot cache initialized"
    );

    // Create FSM engine
    let fsm_engine = Arc::new(pacinet_server::fsm_engine::FsmEngine::new(
        storage.clone(),
        config.clone(),
        tls_config.clone(),
        counter_cache.clone(),
    ));

    let controller_service = service::ControllerService::new(storage.clone())
        .with_counter_cache(counter_cache.clone());
    let management_service = service::ManagementService::new(storage.clone(), config.clone())
        .with_tls(tls_config.clone())
        .with_fsm_engine(fsm_engine.clone());

    // Spawn FSM engine evaluation loop
    let (fsm_shutdown_tx, fsm_shutdown_rx) = tokio::sync::watch::channel(false);
    let fsm_engine_clone = fsm_engine.clone();
    tokio::spawn(async move {
        fsm_engine_clone.run(fsm_shutdown_rx).await;
    });

    // Spawn stale node reaper + metrics updater
    let reaper_storage = storage.clone();
    let reaper_interval = config.heartbeat_expect_interval;
    let stale_threshold = config.stale_threshold();
    let reaper_start = config.start_time;
    let reaper_cache = counter_cache.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(reaper_interval);
        loop {
            interval.tick().await;

            // Update uptime metric
            pacinet_server::metrics::record_uptime(reaper_start.elapsed().as_secs_f64());

            // Evict expired counter snapshots + update gauge
            reaper_cache.evict_expired();
            pacinet_server::metrics::update_counter_snapshot_gauge(
                reaper_cache.total_snapshots(),
            );

            // Update node gauge metrics
            let storage_summary = reaper_storage.clone();
            if let Ok(Ok((total, by_state, _))) =
                tokio::task::spawn_blocking(move || storage_summary.status_summary()).await
            {
                pacinet_server::metrics::update_node_gauges(total, &by_state);
            }

            let threshold = stale_threshold;
            let storage_clone = reaper_storage.clone();
            let result =
                tokio::task::spawn_blocking(move || storage_clone.mark_stale_nodes(threshold))
                    .await;
            match result {
                Ok(Ok(stale_ids)) => {
                    if !stale_ids.is_empty() {
                        pacinet_server::metrics::record_heartbeat_missed(stale_ids.len() as u64);
                    }
                    for id in &stale_ids {
                        warn!(node_id = %id, "Node marked offline (missed heartbeats)");
                    }
                }
                Ok(Err(e)) => warn!("Stale node check failed: {}", e),
                Err(e) => warn!("Stale node task panicked: {}", e),
            }
        }
    });

    // Health check service
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pacinet_proto::paci_net_controller_server::PaciNetControllerServer<
            service::ControllerService,
        >>()
        .await;
    health_reporter
        .set_serving::<pacinet_proto::paci_net_management_server::PaciNetManagementServer<
            service::ManagementService,
        >>()
        .await;

    let shutdown = async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        info!("Shutdown signal received, draining connections...");
        let _ = fsm_shutdown_tx.send(true);
    };

    let mut server = tonic::transport::Server::builder();

    if let Some(ref tls) = tls_config {
        let server_tls = pacinet_core::tls::load_server_tls(tls)
            .map_err(|e| -> Box<dyn std::error::Error> { e })?;
        server = server.tls_config(server_tls)?;
    }

    server
        .accept_http1(true)
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(health_service)
        .add_service(
            pacinet_proto::paci_net_controller_server::PaciNetControllerServer::new(
                controller_service,
            ),
        )
        .add_service(
            pacinet_proto::paci_net_management_server::PaciNetManagementServer::new(
                management_service,
            ),
        )
        .serve_with_shutdown(addr, shutdown)
        .await?;

    info!("Controller shut down cleanly");
    Ok(())
}
