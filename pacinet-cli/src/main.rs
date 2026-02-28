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
    /// Deploy rules to a node or nodes matching labels
    Deploy {
        /// Path to rules YAML file
        rules_file: String,
        /// Target node ID (for single-node deploy)
        #[arg(long)]
        node: Option<String>,
        /// Filter by label for batch deploy (key=value)
        #[arg(short, long, value_parser = parse_label)]
        label: Vec<(String, String)>,
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
    /// Show controller/fleet status
    Status {
        /// Filter by label (key=value)
        #[arg(short, long, value_parser = parse_label)]
        label: Vec<(String, String)>,
    },
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
    /// Show unified diff between policies of two nodes
    Diff {
        /// First node ID
        node_a: String,
        /// Second node ID
        node_b: String,
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
    let state = state_name(node.state);
    json!({
        "node_id": node.node_id,
        "hostname": node.hostname,
        "agent_address": node.agent_address,
        "labels": node.labels,
        "state": state,
        "pacgate_version": node.pacgate_version,
        "policy_hash": node.policy_hash,
        "uptime_seconds": node.uptime_seconds,
        "heartbeat_age_seconds": node.last_heartbeat_age_seconds,
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

fn format_heartbeat_age(age_seconds: f64) -> String {
    if age_seconds < 60.0 {
        format!("{:.0}s", age_seconds)
    } else if age_seconds < 3600.0 {
        format!("{:.0}m", age_seconds / 60.0)
    } else {
        format!("{:.1}h", age_seconds / 3600.0)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let level = if cli.debug { Level::DEBUG } else { Level::WARN };
    tracing_subscriber::fmt().with_max_level(level).init();

    match cli.command {
        Commands::Node { action } => handle_node(action, &cli.server, cli.json).await?,
        Commands::Deploy {
            rules_file,
            node,
            label,
            counters,
            rate_limit,
            conntrack,
        } => {
            handle_deploy(
                &cli.server,
                &rules_file,
                node.as_deref(),
                label,
                counters,
                rate_limit,
                conntrack,
            )
            .await?
        }
        Commands::Policy { action } => handle_policy(action, &cli.server, cli.json).await?,
        Commands::Counters {
            node_id,
            aggregate,
            label,
        } => handle_counters(&cli.server, node_id, aggregate, label, cli.json).await?,
        Commands::Status { label } => handle_status(&cli.server, label, cli.json).await?,
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
                    "{:<38} {:<15} {:<10} {:<16} {:>5}",
                    "NODE ID", "HOSTNAME", "STATE", "POLICY HASH", "HB"
                );
                for node in &response.nodes {
                    let hash_short = if node.policy_hash.is_empty() {
                        "-".to_string()
                    } else if node.policy_hash.len() > 12 {
                        node.policy_hash[..12].to_string()
                    } else {
                        node.policy_hash.clone()
                    };
                    println!(
                        "{:<38} {:<15} {:<10} {:<16} {:>5}",
                        node.node_id,
                        node.hostname,
                        state_name(node.state),
                        hash_short,
                        format_heartbeat_age(node.last_heartbeat_age_seconds),
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
                    if !node.policy_hash.is_empty() {
                        println!("Policy Hash:  {}", node.policy_hash);
                    }
                    if node.uptime_seconds > 0 {
                        println!("Uptime:       {}s", node.uptime_seconds);
                    }
                    println!(
                        "Heartbeat:    {} ago",
                        format_heartbeat_age(node.last_heartbeat_age_seconds)
                    );
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
    rules_file: &str,
    node_id: Option<&str>,
    label: Vec<(String, String)>,
    counters: bool,
    rate_limit: bool,
    conntrack: bool,
) -> Result<()> {
    let rules_yaml = std::fs::read_to_string(rules_file)
        .context(format!("Failed to read rules file: {}", rules_file))?;

    let mut client = connect(server).await?;

    let options = Some(pacinet_proto::CompileOptions {
        counters,
        rate_limit,
        conntrack,
    });

    if let Some(nid) = node_id {
        // Single-node deploy
        let response = client
            .deploy_policy(pacinet_proto::DeployPolicyRequest {
                node_id: nid.to_string(),
                rules_yaml,
                options,
            })
            .await?
            .into_inner();

        if response.success {
            println!("Policy deployed to {}", nid);
        } else {
            eprintln!("Deployment failed: {}", response.message);
        }
        for warning in &response.warnings {
            eprintln!("  warning: {}", warning);
        }
    } else {
        // Batch deploy by label
        let label_filter: std::collections::HashMap<String, String> =
            label.into_iter().collect();

        let response = client
            .batch_deploy_policy(pacinet_proto::BatchDeployPolicyRequest {
                label_filter,
                rules_yaml,
                options,
            })
            .await?
            .into_inner();

        if response.total_nodes == 0 {
            println!("No nodes matched the label filter");
            return Ok(());
        }

        // Show per-node table
        println!(
            "{:<38} {:<15} {:<10} MESSAGE",
            "NODE ID", "HOSTNAME", "RESULT"
        );
        for result in &response.results {
            let status = if result.success { "OK" } else { "FAILED" };
            println!(
                "{:<38} {:<15} {:<10} {}",
                result.node_id, result.hostname, status, result.message
            );
            for warning in &result.warnings {
                println!("  warning: {}", warning);
            }
        }
        println!(
            "\n{}/{} succeeded, {} failed",
            response.succeeded, response.total_nodes, response.failed
        );
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
        PolicyCommands::Diff { node_a, node_b } => {
            let resp_a = client
                .get_policy(pacinet_proto::GetPolicyRequest {
                    node_id: node_a.clone(),
                })
                .await?
                .into_inner();

            let resp_b = client
                .get_policy(pacinet_proto::GetPolicyRequest {
                    node_id: node_b.clone(),
                })
                .await?
                .into_inner();

            if resp_a.rules_yaml == resp_b.rules_yaml {
                println!("Policies are identical (hash: {})", resp_a.policy_hash);
            } else {
                use similar::TextDiff;
                let diff = TextDiff::from_lines(&resp_a.rules_yaml, &resp_b.rules_yaml);
                println!("--- {} ({})", node_a, resp_a.policy_hash);
                println!("+++ {} ({})", node_b, resp_b.policy_hash);
                for change in diff.iter_all_changes() {
                    let sign = match change.tag() {
                        similar::ChangeTag::Delete => "-",
                        similar::ChangeTag::Insert => "+",
                        similar::ChangeTag::Equal => " ",
                    };
                    print!("{}{}", sign, change);
                }
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

async fn handle_status(
    server: &str,
    label: Vec<(String, String)>,
    as_json: bool,
) -> Result<()> {
    match connect(server).await {
        Ok(mut client) => {
            let label_filter: std::collections::HashMap<String, String> =
                label.into_iter().collect();

            let response = client
                .get_fleet_status(pacinet_proto::GetFleetStatusRequest { label_filter })
                .await?
                .into_inner();

            if as_json {
                let nodes: Vec<_> = response
                    .nodes
                    .iter()
                    .map(|n| {
                        json!({
                            "node_id": n.node_id,
                            "hostname": n.hostname,
                            "state": state_name(n.state),
                            "policy_hash": n.policy_hash,
                            "uptime_seconds": n.uptime_seconds,
                            "heartbeat_age_seconds": n.last_heartbeat_age_seconds,
                        })
                    })
                    .collect();
                let val = json!({
                    "controller": server,
                    "total_nodes": response.total_nodes,
                    "nodes_by_state": response.nodes_by_state,
                    "nodes": nodes,
                });
                println!("{}", serde_json::to_string_pretty(&val)?);
            } else {
                println!("Controller:  {} (connected)", server);
                println!("Total nodes: {}", response.total_nodes);

                if !response.nodes_by_state.is_empty() {
                    let mut states: Vec<_> = response.nodes_by_state.iter().collect();
                    states.sort_by_key(|(k, _)| (*k).clone());
                    let summary: Vec<String> = states
                        .iter()
                        .map(|(state, count)| format!("{}={}", state, count))
                        .collect();
                    println!("By state:    {}", summary.join(", "));
                }

                if !response.nodes.is_empty() {
                    println!();
                    println!(
                        "{:<38} {:<15} {:<10} {:<16} {:>5}",
                        "NODE ID", "HOSTNAME", "STATE", "POLICY HASH", "HB"
                    );
                    for node in &response.nodes {
                        let hash_short = if node.policy_hash.is_empty() {
                            "-".to_string()
                        } else if node.policy_hash.len() > 12 {
                            node.policy_hash[..12].to_string()
                        } else {
                            node.policy_hash.clone()
                        };
                        println!(
                            "{:<38} {:<15} {:<10} {:<16} {:>5}",
                            node.node_id,
                            node.hostname,
                            state_name(node.state),
                            hash_short,
                            format_heartbeat_age(node.last_heartbeat_age_seconds),
                        );
                    }
                }
            }
        }
        Err(e) => {
            println!("Controller:  {} (unreachable)", server);
            println!("Error:       {}", e);
        }
    }

    Ok(())
}
