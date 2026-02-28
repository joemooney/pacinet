//! PaciNet Controller — SDN controller for PacGate nodes
//!
//! Manages node registration, policy deployment, and counter aggregation.

mod registry;
mod service;

use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, Level};

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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let level = if args.debug { Level::DEBUG } else { Level::INFO };
    tracing_subscriber::fmt().with_max_level(level).init();

    let registry = Arc::new(registry::NodeRegistry::new());

    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    info!("PaciNet controller starting on {}", addr);

    let controller_service = service::ControllerService::new(registry.clone());
    let management_service = service::ManagementService::new(registry.clone());

    tonic::transport::Server::builder()
        .accept_http1(true)
        .layer(tonic_web::GrpcWebLayer::new())
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
