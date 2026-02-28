//! PaciNet CLI — operator interface for the PaciNet SDN controller
//!
//! Connects to the controller's management gRPC service.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pacinet_proto::paci_net_management_client::PaciNetManagementClient;
use serde_json::json;
use tracing::Level;

/// PaciNet SDN Controller CLI
#[derive(Parser, Debug)]
#[command(name = "pacinet")]
#[command(version, about = "Manage PacGate FPGA packet filter nodes")]
struct Cli {
    /// Controller gRPC address
    #[arg(short, long, global = true, default_value = "http://127.0.0.1:50054")]
    server: String,

    /// Enable debug logging
    #[arg(short, long, global = true)]
    debug: bool,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Manage nodes
    Node {
        #[command(subcommand)]
        action: NodeCommands,
    },
    /// Deploy rules to a node
    Deploy {
        /// Target node ID
        node_id: String,
        /// Path to rules YAML file
        rules_file: String,
        /// Enable counters
        #[arg(long)]
        counters: bool,
        /// Enable rate limiting
        #[arg(long)]
        rate_limit: bool,
        /// Enable connection tracking
        #[arg(long)]
        conntrack: bool,
    },
    /// Show deployed policy
    Policy {
        #[command(subcommand)]
        action: PolicyCommands,
    },
    /// Show rule counters
    Counters {
        /// Node ID (omit for aggregate)
        node_id: Option<String>,
        /// Aggregate across nodes matching labels
        #[arg(long)]
        aggregate: bool,
        /// Filter by label (key=value)
        #[arg(short, long, value_parser = parse_label)]
        label: Vec<(String, String)>,
    },
    /// Show controller status
    Status,
    /// Show version
    Version,
}

#[derive(Subcommand, Debug)]
enum NodeCommands {
    /// List registered nodes
    List {
        /// Filter by label (key=value)
        #[arg(short, long, value_parser = parse_label)]
        label: Vec<(String, String)>,
    },
    /// Show node details
    Show {
        /// Node ID
        node_id: String,
    },
    /// Remove a node
    Remove {
        /// Node ID
        node_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum PolicyCommands {
    /// Show policy for a node
    Show {
        /// Node ID
        node_id: String,
    },
}

fn parse_label(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err("Label must be in key=value format".to_string());
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn node_to_json(node: &pacinet_proto::NodeInfo) -> serde_json::Value {
    let state = pacinet_proto::NodeState::try_from(node.state)
        .map(|s| format!("{:?}", s))
        .unwrap_or_else(|_| "unknown".to_string());
    json!({
        "node_id": node.node_id,
        "hostname": node.hostname,
        "agent_address": node.agent_address,
        "labels": node.labels,
        "state": state,
        "pacgate_version": node.pacgate_version,
    })
}

fn counter_to_json(c: &pacinet_proto::RuleCounter) -> serde_json::Value {
    json!({
        "rule_name": c.rule_name,
        "match_count": c.match_count,
        "byte_count": c.byte_count,
    })
}

fn state_name(state: i32) -> String {
    pacinet_proto::NodeState::try_from(state)
        .map(|s| format!("{:?}", s))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let level = if cli.debug { Level::DEBUG } else { Level::WARN };
    tracing_subscriber::fmt().with_max_level(level).init();

    match cli.command {
        Commands::Node { action } => handle_node(action, &cli.server, cli.json).await?,
        Commands::Deploy {
            node_id,
            rules_file,
            counters,
            rate_limit,
            conntrack,
        } => {
            handle_deploy(&cli.server, &node_id, &rules_file, counters, rate_limit, conntrack)
                .await?
        }
        Commands::Policy { action } => handle_policy(action, &cli.server, cli.json).await?,
        Commands::Counters {
            node_id,
            aggregate,
            label,
        } => handle_counters(&cli.server, node_id, aggregate, label, cli.json).await?,
        Commands::Status => handle_status(&cli.server).await?,
        Commands::Version => {
            println!("pacinet {}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}

async fn connect(server: &str) -> Result<PaciNetManagementClient<tonic::transport::Channel>> {
    PaciNetManagementClient::connect(server.to_string())
        .await
        .context(format!("Failed to connect to controller at {}", server))
}

async fn handle_node(action: NodeCommands, server: &str, as_json: bool) -> Result<()> {
    let mut client = connect(server).await?;

    match action {
        NodeCommands::List { label } => {
            let label_filter: std::collections::HashMap<String, String> =
                label.into_iter().collect();
            let response = client
                .list_nodes(pacinet_proto::ListNodesRequest { label_filter })
                .await?
                .into_inner();

            if as_json {
                let nodes: Vec<_> = response.nodes.iter().map(node_to_json).collect();
                println!("{}", serde_json::to_string_pretty(&nodes)?);
            } else if response.nodes.is_empty() {
                println!("No nodes registered");
            } else {
                println!(
                    "{:<38} {:<20} {:<25} {}",
                    "NODE ID", "HOSTNAME", "ADDRESS", "STATE"
                );
                for node in &response.nodes {
                    println!(
                        "{:<38} {:<20} {:<25} {}",
                        node.node_id, node.hostname, node.agent_address, state_name(node.state)
                    );
                }
            }
        }
        NodeCommands::Show { node_id } => {
            let response = client
                .get_node(pacinet_proto::GetNodeRequest {
                    node_id: node_id.clone(),
                })
                .await?
                .into_inner();

            if let Some(node) = response.node {
                if as_json {
                    println!("{}", serde_json::to_string_pretty(&node_to_json(&node))?);
                } else {
                    println!("Node ID:      {}", node.node_id);
                    println!("Hostname:     {}", node.hostname);
                    println!("Address:      {}", node.agent_address);
                    println!("PacGate:      {}", node.pacgate_version);
                    println!("State:        {}", state_name(node.state));
                    if !node.labels.is_empty() {
                        println!("Labels:");
                        for (k, v) in &node.labels {
                            println!("  {}={}", k, v);
                        }
                    }
                }
            } else {
                eprintln!("Node {} not found", node_id);
            }
        }
        NodeCommands::Remove { node_id } => {
            let response = client
                .remove_node(pacinet_proto::RemoveNodeRequest {
                    node_id: node_id.clone(),
                })
                .await?
                .into_inner();

            if response.success {
                println!("Node {} removed", node_id);
            } else {
                eprintln!("{}", response.message);
            }
        }
    }

    Ok(())
}

async fn handle_deploy(
    server: &str,
    node_id: &str,
    rules_file: &str,
    counters: bool,
    rate_limit: bool,
    conntrack: bool,
) -> Result<()> {
    let rules_yaml = std::fs::read_to_string(rules_file)
        .context(format!("Failed to read rules file: {}", rules_file))?;

    let mut client = connect(server).await?;

    let response = client
        .deploy_policy(pacinet_proto::DeployPolicyRequest {
            node_id: node_id.to_string(),
            rules_yaml,
            options: Some(pacinet_proto::CompileOptions {
                counters,
                rate_limit,
                conntrack,
            }),
        })
        .await?
        .into_inner();

    if response.success {
        println!("Policy deployed to {}", node_id);
    } else {
        eprintln!("Deployment failed: {}", response.message);
    }
    for warning in &response.warnings {
        eprintln!("  warning: {}", warning);
    }

    Ok(())
}

async fn handle_policy(action: PolicyCommands, server: &str, as_json: bool) -> Result<()> {
    let mut client = connect(server).await?;

    match action {
        PolicyCommands::Show { node_id } => {
            let response = client
                .get_policy(pacinet_proto::GetPolicyRequest {
                    node_id: node_id.clone(),
                })
                .await?
                .into_inner();

            if as_json {
                let val = json!({
                    "node_id": response.node_id,
                    "rules_yaml": response.rules_yaml,
                    "policy_hash": response.policy_hash,
                });
                println!("{}", serde_json::to_string_pretty(&val)?);
            } else {
                println!("Node:   {}", response.node_id);
                println!("Hash:   {}", response.policy_hash);
                println!("---");
                println!("{}", response.rules_yaml);
            }
        }
    }

    Ok(())
}

async fn handle_counters(
    server: &str,
    node_id: Option<String>,
    aggregate: bool,
    label: Vec<(String, String)>,
    as_json: bool,
) -> Result<()> {
    let mut client = connect(server).await?;

    if aggregate || node_id.is_none() {
        let label_filter: std::collections::HashMap<String, String> =
            label.into_iter().collect();
        let response = client
            .get_aggregate_counters(pacinet_proto::GetAggregateCountersRequest { label_filter })
            .await?
            .into_inner();

        if as_json {
            let val: Vec<_> = response
                .node_counters
                .iter()
                .map(|nc| {
                    json!({
                        "node_id": nc.node_id,
                        "counters": nc.counters.iter().map(counter_to_json).collect::<Vec<_>>(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&val)?);
        } else {
            for nc in &response.node_counters {
                println!("Node: {}", nc.node_id);
                print_counters(&nc.counters);
                println!();
            }
        }
    } else if let Some(node_id) = node_id {
        let response = client
            .get_node_counters(pacinet_proto::GetNodeCountersRequest {
                node_id: node_id.clone(),
            })
            .await?
            .into_inner();

        if as_json {
            let val: Vec<_> = response.counters.iter().map(counter_to_json).collect();
            println!("{}", serde_json::to_string_pretty(&val)?);
        } else {
            println!("Node: {}", response.node_id);
            print_counters(&response.counters);
        }
    }

    Ok(())
}

fn print_counters(counters: &[pacinet_proto::RuleCounter]) {
    if counters.is_empty() {
        println!("  (no counters)");
        return;
    }
    println!("  {:<30} {:>12} {:>12}", "RULE", "MATCHES", "BYTES");
    for c in counters {
        println!(
            "  {:<30} {:>12} {:>12}",
            c.rule_name, c.match_count, c.byte_count
        );
    }
}

async fn handle_status(server: &str) -> Result<()> {
    match connect(server).await {
        Ok(mut client) => {
            let response = client
                .list_nodes(pacinet_proto::ListNodesRequest {
                    label_filter: std::collections::HashMap::new(),
                })
                .await?
                .into_inner();

            println!("Controller:  {} (connected)", server);
            println!("Nodes:       {}", response.nodes.len());
        }
        Err(e) => {
            println!("Controller:  {} (unreachable)", server);
            println!("Error:       {}", e);
        }
    }

    Ok(())
}
