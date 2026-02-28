//! PaciNet Controller — SDN controller for PacGate nodes
//!
//! Manages node registration, policy deployment, and counter aggregation.

use pacinet_server::config::ControllerConfig;
use pacinet_server::rest;
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

    /// Web dashboard HTTP port (0 to disable)
    #[arg(long, default_value = "8081")]
    web_port: u16,

    /// Directory for static web files (SPA)
    #[arg(long)]
    static_dir: Option<String>,

    /// API key for REST authentication (optional, env: PACINET_API_KEY)
    #[arg(long, env = "PACINET_API_KEY")]
    api_key: Option<String>,

    /// Cluster ID for HA leader election (enables HA mode, requires --db)
    #[arg(long)]
    cluster_id: Option<String>,

    /// Leader lease duration in seconds (default: 30)
    #[arg(long, default_value = "30")]
    lease_duration: u64,

    /// Persist counter events to event log (default: false, high frequency)
    #[arg(long)]
    persist_counter_events: bool,

    /// Maximum event age in days before pruning (default: 7)
    #[arg(long, default_value = "7")]
    event_max_age_days: u64,

    /// Enable mDNS discovery of agents
    #[arg(long)]
    mdns_discover: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let default_level = if args.debug { "debug" } else { "info" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // Validate HA config
    if args.cluster_id.is_some() && args.db.is_none() {
        return Err("--cluster-id requires --db (SQLite) for shared lease storage".into());
    }

    let config = ControllerConfig {
        deploy_timeout: std::time::Duration::from_secs(args.deploy_timeout),
        heartbeat_expect_interval: std::time::Duration::from_secs(args.heartbeat_expect_interval),
        heartbeat_miss_threshold: args.heartbeat_miss_threshold,
        start_time: tokio::time::Instant::now(),
        counter_snapshot_retention: std::time::Duration::from_secs(args.counter_retention_secs),
        counter_snapshot_max_per_node: args.counter_max_per_node,
        ..Default::default()
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

    // Create event bus for streaming RPCs
    let event_bus = pacinet_server::events::EventBus::new(256);
    info!("Event bus initialized (buffer=256)");

    // Create FSM engine
    let fsm_engine = Arc::new(
        pacinet_server::fsm_engine::FsmEngine::new(
            storage.clone(),
            config.clone(),
            tls_config.clone(),
            counter_cache.clone(),
        )
        .with_event_bus(event_bus.clone()),
    );

    let controller_service = service::ControllerService::new(storage.clone())
        .with_counter_cache(counter_cache.clone())
        .with_event_bus(event_bus.clone());
    let management_service = service::ManagementService::new(storage.clone(), config.clone())
        .with_tls(tls_config.clone())
        .with_fsm_engine(fsm_engine.clone())
        .with_event_bus(event_bus.clone());

    // Leader election (HA mode)
    let (leader_shutdown_tx, leader_shutdown_rx) = tokio::sync::watch::channel(false);
    if let Some(ref cluster_id) = args.cluster_id {
        let leader = pacinet_server::leader::LeaderElection::new(
            cluster_id.clone(),
            std::time::Duration::from_secs(args.lease_duration),
            storage.clone(),
        );
        // Share the is_leader flag with config
        let is_leader_flag = leader.is_leader_flag();
        config.is_leader.store(false, std::sync::atomic::Ordering::SeqCst);
        // Copy the flag reference
        let config_flag = config.is_leader.clone();
        tokio::spawn(async move {
            // Sync flags: when leader flag changes, update config
            let leader_flag = is_leader_flag;
            leader.run(leader_shutdown_rx).await;
            drop(leader_flag);
            drop(config_flag);
        });
        // Do initial sync before starting
        info!(cluster_id = %cluster_id, "HA mode enabled, starting leader election");
    } else {
        info!("Single-node mode (leader by default)");
    }

    // Spawn FSM engine evaluation loop
    let (fsm_shutdown_tx, fsm_shutdown_rx) = tokio::sync::watch::channel(false);
    let fsm_engine_clone = fsm_engine.clone();
    let fsm_is_leader = config.is_leader.clone();
    tokio::spawn(async move {
        // FSM engine: wrap run to only evaluate when leader
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut shutdown_rx = fsm_shutdown_rx;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if fsm_is_leader.load(std::sync::atomic::Ordering::SeqCst) {
                        fsm_engine_clone.evaluate_all_public().await;
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("FSM engine shutting down");
                    return;
                }
            }
        }
    });

    // Spawn event persistence subscriber
    let persist_storage = storage.clone();
    let persist_counter_events = args.persist_counter_events;
    let persist_is_leader = config.is_leader.clone();
    let mut fsm_rx = event_bus.fsm_tx.subscribe();
    let mut counter_rx = event_bus.counter_tx.subscribe();
    let mut node_rx = event_bus.node_tx.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Ok(event) = fsm_rx.recv() => {
                    if !persist_is_leader.load(std::sync::atomic::Ordering::SeqCst) { continue; }
                    let pe = event.to_persistent();
                    let s = persist_storage.clone();
                    let _ = tokio::task::spawn_blocking(move || s.store_event(pe)).await;
                }
                Ok(event) = counter_rx.recv() => {
                    if !persist_counter_events { continue; }
                    if !persist_is_leader.load(std::sync::atomic::Ordering::SeqCst) { continue; }
                    let pe = event.to_persistent();
                    let s = persist_storage.clone();
                    let _ = tokio::task::spawn_blocking(move || s.store_event(pe)).await;
                }
                Ok(event) = node_rx.recv() => {
                    if !persist_is_leader.load(std::sync::atomic::Ordering::SeqCst) { continue; }
                    let pe = event.to_persistent();
                    let s = persist_storage.clone();
                    let _ = tokio::task::spawn_blocking(move || s.store_event(pe)).await;
                }
                else => { break; }
            }
        }
    });

    // Spawn stale node reaper + metrics updater + event pruning
    let reaper_storage = storage.clone();
    let reaper_interval = config.heartbeat_expect_interval;
    let stale_threshold = config.stale_threshold();
    let reaper_start = config.start_time;
    let reaper_cache = counter_cache.clone();
    let reaper_event_bus = event_bus.clone();
    let reaper_is_leader = config.is_leader.clone();
    let event_max_age_days = args.event_max_age_days;
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

            // Only run write operations on leader
            if !reaper_is_leader.load(std::sync::atomic::Ordering::SeqCst) {
                continue;
            }

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
                        // Emit HeartbeatStale event
                        let nid = id.clone();
                        let s = reaper_storage.clone();
                        if let Ok(Ok(Some(node))) =
                            tokio::task::spawn_blocking(move || s.get_node(&nid)).await
                        {
                            reaper_event_bus.emit_node(
                                pacinet_server::events::NodeEvent::HeartbeatStale {
                                    node_id: node.node_id.clone(),
                                    hostname: node.hostname.clone(),
                                    labels: node.labels.clone(),
                                    timestamp: chrono::Utc::now(),
                                },
                            );
                        }
                    }
                }
                Ok(Err(e)) => warn!("Stale node check failed: {}", e),
                Err(e) => warn!("Stale node task panicked: {}", e),
            }

            // Prune old events
            if event_max_age_days > 0 {
                let cutoff =
                    chrono::Utc::now() - chrono::Duration::days(event_max_age_days as i64);
                let s = reaper_storage.clone();
                let _ = tokio::task::spawn_blocking(move || s.prune_events(cutoff)).await;
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

    // Shared shutdown signal
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // Spawn REST/web server if enabled
    if args.web_port > 0 {
        let app_state = rest::AppState {
            storage: storage.clone(),
            config: config.clone(),
            counter_cache: counter_cache.clone(),
            fsm_engine: fsm_engine.clone(),
            event_bus: event_bus.clone(),
            tls_config: tls_config.clone(),
            api_key: args.api_key.clone(),
        };

        let mut app = rest::router(app_state);

        // Serve static files with SPA fallback
        let static_dir = args
            .static_dir
            .clone()
            .unwrap_or_else(|| "pacinet-web/dist".to_string());
        if std::path::Path::new(&static_dir).exists() {
            info!(dir = %static_dir, "Serving static files for web dashboard");
            app = app.fallback_service(
                tower_http::services::ServeDir::new(&static_dir)
                    .fallback(tower_http::services::ServeFile::new(format!(
                        "{}/index.html",
                        static_dir
                    ))),
            );
        } else {
            info!(dir = %static_dir, "Static dir not found, REST API only (dev mode)");
        }

        let web_addr: SocketAddr = format!("{}:{}", args.host, args.web_port).parse()?;
        let mut web_shutdown_rx = shutdown_tx.subscribe();
        info!("Web dashboard starting on http://{}", web_addr);
        let listener = tokio::net::TcpListener::bind(web_addr).await?;
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = web_shutdown_rx.recv().await;
                })
                .await
                .unwrap_or_else(|e| warn!("Web server error: {}", e));
        });
    }

    if args.api_key.is_some() {
        info!("REST API key authentication enabled");
    }

    // mDNS discovery (placeholder - requires mdns-sd crate)
    if args.mdns_discover {
        info!("mDNS agent discovery enabled (scanning for _pacinet-agent._tcp.local.)");
        // mDNS discovery is a future enhancement requiring the mdns-sd crate
        warn!("mDNS discovery is not yet implemented — agents must register manually");
    }

    let shutdown_tx_clone = shutdown_tx.clone();
    let shutdown = async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        info!("Shutdown signal received, draining connections...");
        let _ = fsm_shutdown_tx.send(true);
        let _ = leader_shutdown_tx.send(true);
        let _ = shutdown_tx_clone.send(());
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
