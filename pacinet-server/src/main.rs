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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let default_level = if args.debug { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = ControllerConfig {
        deploy_timeout: std::time::Duration::from_secs(args.deploy_timeout),
        heartbeat_expect_interval: std::time::Duration::from_secs(args.heartbeat_expect_interval),
        heartbeat_miss_threshold: args.heartbeat_miss_threshold,
        start_time: tokio::time::Instant::now(),
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

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    info!("PaciNet controller starting on {}", addr);

    let controller_service = service::ControllerService::new(storage.clone());
    let management_service = service::ManagementService::new(storage.clone(), config.clone());

    // Spawn stale node reaper
    let reaper_storage = storage.clone();
    let reaper_interval = config.heartbeat_expect_interval;
    let stale_threshold = config.stale_threshold();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(reaper_interval);
        loop {
            interval.tick().await;
            let threshold = stale_threshold;
            let storage_clone = reaper_storage.clone();
            let result = tokio::task::spawn_blocking(move || {
                storage_clone.mark_stale_nodes(threshold)
            })
            .await;
            match result {
                Ok(Ok(stale_ids)) => {
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

    tonic::transport::Server::builder()
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
        .serve(addr)
        .await?;

    Ok(())
}
