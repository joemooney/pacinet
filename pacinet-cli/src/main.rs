//! PaciNet CLI — operator interface for the PaciNet SDN controller
//!
//! Connects to the controller's management gRPC service.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pacinet_proto::paci_net_management_client::PaciNetManagementClient;
use serde_json::json;
use tokio_stream::StreamExt;
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

    /// Pretty-print multiline fields for human readability (overrides strict JSON for some commands)
    #[arg(long, global = true)]
    pretty: bool,

    /// CA certificate for mTLS
    #[arg(long, global = true)]
    ca_cert: Option<String>,

    /// Client TLS certificate
    #[arg(long, global = true)]
    tls_cert: Option<String>,

    /// Client TLS private key
    #[arg(long, global = true)]
    tls_key: Option<String>,

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
        /// Dry-run mode: validate and preview without deploying
        #[arg(long)]
        dry_run: bool,
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
    /// Show deployment history for a node
    #[command(name = "deploy-history")]
    DeployHistory {
        /// Node ID
        node_id: String,
        /// Max entries to show
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Show controller/fleet status
    Status {
        /// Filter by label (key=value)
        #[arg(short, long, value_parser = parse_label)]
        label: Vec<(String, String)>,
    },
    /// Manage FSM definitions and instances
    Fsm {
        #[command(subcommand)]
        action: FsmCommands,
    },
    /// Watch live events (streaming)
    Watch {
        #[command(subcommand)]
        action: WatchCommands,
    },
    /// Query audit log
    Audit {
        /// Filter by action
        #[arg(long)]
        action: Option<String>,
        /// Filter by resource type
        #[arg(long)]
        resource_type: Option<String>,
        /// Max entries
        #[arg(long, default_value = "50")]
        limit: u32,
    },
    /// Manage policy templates
    Template {
        #[command(subcommand)]
        action: TemplateCommands,
    },
    /// Show version
    Version,
}

#[derive(Subcommand, Debug)]
enum TemplateCommands {
    /// Create a template from a YAML file
    Create {
        /// Path to rules YAML file
        file: String,
        /// Template name
        #[arg(long)]
        name: String,
        /// Description
        #[arg(long, default_value = "")]
        description: String,
        /// Tags (comma-separated)
        #[arg(long, default_value = "")]
        tags: String,
    },
    /// List templates
    List {
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,
    },
    /// Show a template
    Show {
        /// Template name
        name: String,
    },
    /// Delete a template
    Delete {
        /// Template name
        name: String,
    },
    /// Deploy from a template
    Deploy {
        /// Template name
        name: String,
        /// Target node ID
        #[arg(long)]
        node: String,
        /// Enable counters
        #[arg(long)]
        counters: bool,
        /// Dry-run mode
        #[arg(long)]
        dry_run: bool,
    },
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
    /// Set annotations on a node (key=value pairs)
    Annotate {
        /// Node ID
        node_id: String,
        /// Annotations (key=value)
        #[arg(value_parser = parse_label)]
        annotations: Vec<(String, String)>,
        /// Keys to remove
        #[arg(long)]
        remove: Vec<String>,
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
    /// Show policy version history for a node
    History {
        /// Node ID
        node_id: String,
        /// Max entries to show
        #[arg(long, default_value = "10")]
        limit: u32,
    },
    /// Rollback to a previous policy version
    Rollback {
        /// Node ID
        node_id: String,
        /// Target version (default: previous version)
        #[arg(long, default_value = "0")]
        version: u64,
    },
}

#[derive(Subcommand, Debug)]
enum FsmCommands {
    /// Create an FSM definition from a YAML file
    Create {
        /// Path to FSM definition YAML file
        file: String,
    },
    /// List FSM definitions
    List {
        /// Filter by kind (deployment, adaptive_policy)
        #[arg(long)]
        kind: Option<String>,
    },
    /// Show an FSM definition
    Show {
        /// Definition name
        name: String,
    },
    /// Delete an FSM definition
    Delete {
        /// Definition name
        name: String,
    },
    /// Start an FSM instance
    Start {
        /// Definition name
        name: String,
        /// Path to rules YAML file (required for deployment FSMs, optional for adaptive)
        #[arg(long)]
        rules: Option<String>,
        /// Enable counters
        #[arg(long)]
        counters: bool,
        /// Enable rate limiting
        #[arg(long)]
        rate_limit: bool,
        /// Enable connection tracking
        #[arg(long)]
        conntrack: bool,
        /// Target node label filter for adaptive policy FSMs (key=value)
        #[arg(short, long, value_parser = parse_label)]
        label: Vec<(String, String)>,
    },
    /// Show FSM instance status
    #[command(name = "status")]
    InstanceStatus {
        /// Instance ID
        instance_id: String,
    },
    /// List FSM instances
    Instances {
        /// Filter by definition name
        #[arg(long)]
        definition: Option<String>,
        /// Filter by status (running, completed, failed, cancelled)
        #[arg(long)]
        status: Option<String>,
    },
    /// Manually advance an FSM instance
    Advance {
        /// Instance ID
        instance_id: String,
        /// Target state (if not specified, advances to next valid state)
        #[arg(long)]
        state: Option<String>,
    },
    /// Cancel a running FSM instance
    Cancel {
        /// Instance ID
        instance_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum WatchCommands {
    /// Watch FSM transition events
    Fsm {
        /// Filter by instance ID
        #[arg(long)]
        instance: Option<String>,
    },
    /// Watch counter updates with rates
    Counters {
        /// Filter by node ID
        #[arg(long)]
        node: Option<String>,
    },
    /// Watch node lifecycle events
    Nodes {
        /// Filter by label (key=value)
        #[arg(short, long, value_parser = parse_label)]
        label: Vec<(String, String)>,
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
        .map(|s| match s {
            pacinet_proto::NodeState::Unspecified => "unknown",
            pacinet_proto::NodeState::Registered => "registered",
            pacinet_proto::NodeState::Online => "online",
            pacinet_proto::NodeState::Deploying => "deploying",
            pacinet_proto::NodeState::Active => "active",
            pacinet_proto::NodeState::Error => "error",
            pacinet_proto::NodeState::Offline => "offline",
        })
        .unwrap_or("unknown")
        .to_string()
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

    // Ensure crypto provider is available before any TLS operations
    pacinet_core::tls::ensure_crypto_provider();

    // Configure TLS if certs provided
    let tls_config = match (&cli.ca_cert, &cli.tls_cert, &cli.tls_key) {
        (Some(ca), Some(cert), Some(key)) => Some(pacinet_core::tls::TlsConfig::new(
            ca.into(),
            cert.into(),
            key.into(),
        )),
        (None, None, None) => None,
        _ => {
            anyhow::bail!("TLS requires all three: --ca-cert, --tls-cert, --tls-key");
        }
    };

    match cli.command {
        Commands::Node { action } => {
            handle_node(action, &cli.server, cli.json, &tls_config).await?
        }
        Commands::Deploy {
            rules_file,
            node,
            label,
            counters,
            rate_limit,
            conntrack,
            dry_run,
        } => {
            handle_deploy(
                &cli.server,
                &rules_file,
                node.as_deref(),
                label,
                counters,
                rate_limit,
                conntrack,
                dry_run,
                &tls_config,
            )
            .await?
        }
        Commands::Policy { action } => {
            handle_policy(action, &cli.server, cli.json, cli.pretty, &tls_config).await?
        }
        Commands::DeployHistory { node_id, limit } => {
            handle_deploy_history(&cli.server, &node_id, limit, cli.json, &tls_config).await?
        }
        Commands::Counters {
            node_id,
            aggregate,
            label,
        } => {
            handle_counters(
                &cli.server,
                node_id,
                aggregate,
                label,
                cli.json,
                &tls_config,
            )
            .await?
        }
        Commands::Status { label } => {
            handle_status(&cli.server, label, cli.json, &tls_config).await?
        }
        Commands::Fsm { action } => handle_fsm(action, &cli.server, cli.json, &tls_config).await?,
        Commands::Watch { action } => {
            handle_watch(action, &cli.server, cli.json, &tls_config).await?
        }
        Commands::Audit {
            action,
            resource_type,
            limit,
        } => {
            handle_audit(
                &cli.server,
                action,
                resource_type,
                limit,
                cli.json,
                &tls_config,
            )
            .await?
        }
        Commands::Template { action } => {
            handle_template(action, &cli.server, cli.json, cli.pretty, &tls_config).await?
        }
        Commands::Version => {
            println!("pacinet {}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}

async fn connect(
    server: &str,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<PaciNetManagementClient<tonic::transport::Channel>> {
    if let Some(tls) = tls_config {
        let client_tls = pacinet_core::tls::load_client_tls(tls)
            .map_err(|e| anyhow::anyhow!("TLS load error: {}", e))?;
        let endpoint = tonic::transport::Channel::from_shared(server.to_string())
            .map_err(|e| anyhow::anyhow!("Invalid server URI: {}", e))?
            .tls_config(client_tls)
            .map_err(|e| anyhow::anyhow!("TLS config error: {}", e))?;
        let channel = endpoint
            .connect()
            .await
            .context(format!("TLS connect to {} failed", server))?;
        Ok(PaciNetManagementClient::new(channel))
    } else {
        PaciNetManagementClient::connect(server.to_string())
            .await
            .context(format!("Failed to connect to controller at {}", server))
    }
}

async fn handle_node(
    action: NodeCommands,
    server: &str,
    as_json: bool,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    let mut client = connect(server, tls_config).await?;

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
                    if !node.annotations.is_empty() {
                        println!("Annotations:");
                        for (k, v) in &node.annotations {
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
        NodeCommands::Annotate {
            node_id,
            annotations,
            remove,
        } => {
            let ann_map: std::collections::HashMap<String, String> =
                annotations.into_iter().collect();
            let response = client
                .set_node_annotations(pacinet_proto::SetNodeAnnotationsRequest {
                    node_id: node_id.clone(),
                    annotations: ann_map,
                    remove_keys: remove,
                })
                .await?
                .into_inner();

            if response.success {
                println!("Annotations updated for node {}", node_id);
            } else {
                eprintln!("{}", response.message);
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_deploy(
    server: &str,
    rules_file: &str,
    node_id: Option<&str>,
    label: Vec<(String, String)>,
    counters: bool,
    rate_limit: bool,
    conntrack: bool,
    dry_run: bool,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    let rules_yaml = std::fs::read_to_string(rules_file)
        .context(format!("Failed to read rules file: {}", rules_file))?;

    let mut client = connect(server, tls_config).await?;

    let options = Some(pacinet_proto::CompileOptions {
        counters,
        rate_limit,
        conntrack,
        axi: false,
        ports: 1,
        target: "standalone".to_string(),
        dynamic: false,
        dynamic_entries: 16,
        width: 8,
        ptp: false,
        rss: false,
        rss_queues: 4,
        int_enabled: false,
        int_switch_id: 0,
    });

    if let Some(nid) = node_id {
        // Single-node deploy
        let response = client
            .deploy_policy(pacinet_proto::DeployPolicyRequest {
                node_id: nid.to_string(),
                rules_yaml,
                options,
                dry_run,
            })
            .await?
            .into_inner();

        if dry_run {
            if let Some(dr) = response.dry_run_result {
                println!("Dry-run result:");
                println!("  Valid: {}", dr.valid);
                if !dr.validation_errors.is_empty() {
                    println!("  Errors:");
                    for e in &dr.validation_errors {
                        println!("    - {}", e);
                    }
                }
                if !dr.target_nodes.is_empty() {
                    println!(
                        "\n  {:<38} {:<15} {:<16} {:<16} CHANGED",
                        "NODE ID", "HOSTNAME", "CURRENT HASH", "NEW HASH"
                    );
                    for n in &dr.target_nodes {
                        let cur = if n.current_policy_hash.is_empty() {
                            "-".to_string()
                        } else if n.current_policy_hash.len() > 12 {
                            n.current_policy_hash[..12].to_string()
                        } else {
                            n.current_policy_hash.clone()
                        };
                        let new_h = if n.new_policy_hash.len() > 12 {
                            n.new_policy_hash[..12].to_string()
                        } else {
                            n.new_policy_hash.clone()
                        };
                        let changed = if n.policy_changed { "yes" } else { "no" };
                        println!(
                            "  {:<38} {:<15} {:<16} {:<16} {}",
                            n.node_id, n.hostname, cur, new_h, changed
                        );
                    }
                }
            } else {
                println!("Dry-run completed (no details returned)");
            }
        } else {
            if response.success {
                println!("Policy deployed to {}", nid);
            } else {
                eprintln!("Deployment failed: {}", response.message);
            }
            for warning in &response.warnings {
                eprintln!("  warning: {}", warning);
            }
        }
    } else {
        // Batch deploy by label
        let label_filter: std::collections::HashMap<String, String> = label.into_iter().collect();

        let response = client
            .batch_deploy_policy(pacinet_proto::BatchDeployPolicyRequest {
                label_filter,
                rules_yaml,
                options,
                dry_run,
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

async fn handle_policy(
    action: PolicyCommands,
    server: &str,
    as_json: bool,
    pretty: bool,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    let mut client = connect(server, tls_config).await?;

    match action {
        PolicyCommands::Show { node_id } => {
            let response = client
                .get_policy(pacinet_proto::GetPolicyRequest {
                    node_id: node_id.clone(),
                })
                .await?
                .into_inner();

            if as_json && !pretty {
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
        PolicyCommands::History { node_id, limit } => {
            let response = client
                .get_policy_history(pacinet_proto::GetPolicyHistoryRequest {
                    node_id: node_id.clone(),
                    limit,
                })
                .await?
                .into_inner();

            if as_json {
                let versions: Vec<_> = response
                    .versions
                    .iter()
                    .map(|v| {
                        json!({
                            "version": v.version,
                            "policy_hash": v.policy_hash,
                            "deployed_at": v.deployed_at.as_ref().map(|t| t.seconds),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&versions)?);
            } else if response.versions.is_empty() {
                println!("No policy history for node {}", node_id);
            } else {
                println!("{:<8} {:<16} DEPLOYED AT", "VERSION", "HASH");
                for v in &response.versions {
                    let hash_short = if v.policy_hash.len() > 12 {
                        &v.policy_hash[..12]
                    } else {
                        &v.policy_hash
                    };
                    let deployed = v
                        .deployed_at
                        .as_ref()
                        .map(|t| {
                            chrono::DateTime::from_timestamp(t.seconds, 0)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                .unwrap_or_else(|| "?".to_string())
                        })
                        .unwrap_or_else(|| "-".to_string());
                    println!("{:<8} {:<16} {}", v.version, hash_short, deployed);
                }
            }
        }
        PolicyCommands::Rollback { node_id, version } => {
            let response = client
                .rollback_policy(pacinet_proto::RollbackPolicyRequest {
                    node_id: node_id.clone(),
                    target_version: version,
                })
                .await?
                .into_inner();

            if response.success {
                println!(
                    "Rolled back node {} to version {}",
                    node_id, response.rolled_back_to_version
                );
            } else {
                eprintln!("Rollback failed: {}", response.message);
            }
        }
    }

    Ok(())
}

async fn handle_deploy_history(
    server: &str,
    node_id: &str,
    limit: u32,
    as_json: bool,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    let mut client = connect(server, tls_config).await?;

    let response = client
        .get_deployment_history(pacinet_proto::GetDeploymentHistoryRequest {
            node_id: node_id.to_string(),
            limit,
        })
        .await?
        .into_inner();

    if as_json {
        let deployments: Vec<_> = response
            .deployments
            .iter()
            .map(|d| {
                json!({
                    "id": d.id,
                    "policy_version": d.policy_version,
                    "policy_hash": d.policy_hash,
                    "result": d.result,
                    "message": d.message,
                    "deployed_at": d.deployed_at.as_ref().map(|t| t.seconds),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&deployments)?);
    } else if response.deployments.is_empty() {
        println!("No deployment history for node {}", node_id);
    } else {
        println!(
            "{:<20} {:<8} {:<16} {:<12} MESSAGE",
            "TIME", "VERSION", "HASH", "RESULT"
        );
        for d in &response.deployments {
            let hash_short = if d.policy_hash.len() > 12 {
                &d.policy_hash[..12]
            } else {
                &d.policy_hash
            };
            let deployed = d
                .deployed_at
                .as_ref()
                .map(|t| {
                    chrono::DateTime::from_timestamp(t.seconds, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| "?".to_string())
                })
                .unwrap_or_else(|| "-".to_string());
            println!(
                "{:<20} {:<8} {:<16} {:<12} {}",
                deployed, d.policy_version, hash_short, d.result, d.message
            );
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
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    let mut client = connect(server, tls_config).await?;

    if aggregate || node_id.is_none() {
        let label_filter: std::collections::HashMap<String, String> = label.into_iter().collect();
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
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    match connect(server, tls_config).await {
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

async fn handle_fsm(
    action: FsmCommands,
    server: &str,
    as_json: bool,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    let mut client = connect(server, tls_config).await?;

    match action {
        FsmCommands::Create { file } => {
            let yaml = std::fs::read_to_string(&file)
                .context(format!("Failed to read FSM definition: {}", file))?;

            let response = client
                .create_fsm_definition(pacinet_proto::CreateFsmDefinitionRequest {
                    definition_yaml: yaml,
                })
                .await?
                .into_inner();

            if response.success {
                println!("FSM definition '{}' created", response.name);
            } else {
                eprintln!("Failed: {}", response.message);
            }
        }
        FsmCommands::List { kind } => {
            let response = client
                .list_fsm_definitions(pacinet_proto::ListFsmDefinitionsRequest {
                    kind: kind.unwrap_or_default(),
                })
                .await?
                .into_inner();

            if as_json {
                let defs: Vec<_> = response
                    .definitions
                    .iter()
                    .map(|d| {
                        serde_json::json!({
                            "name": d.name,
                            "kind": d.kind,
                            "description": d.description,
                            "states": d.state_count,
                            "initial": d.initial_state,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&defs)?);
            } else if response.definitions.is_empty() {
                println!("No FSM definitions");
            } else {
                println!(
                    "{:<25} {:<15} {:<6} {:<12} DESCRIPTION",
                    "NAME", "KIND", "STATES", "INITIAL"
                );
                for d in &response.definitions {
                    println!(
                        "{:<25} {:<15} {:<6} {:<12} {}",
                        d.name, d.kind, d.state_count, d.initial_state, d.description
                    );
                }
            }
        }
        FsmCommands::Show { name } => {
            let response = client
                .get_fsm_definition(pacinet_proto::GetFsmDefinitionRequest { name: name.clone() })
                .await?
                .into_inner();

            if as_json {
                let val = serde_json::json!({
                    "name": response.name,
                    "kind": response.kind,
                    "description": response.description,
                });
                println!("{}", serde_json::to_string_pretty(&val)?);
            } else {
                println!("Name:        {}", response.name);
                println!("Kind:        {}", response.kind);
                println!("Description: {}", response.description);
                println!("---");
                println!("{}", response.definition_yaml);
            }
        }
        FsmCommands::Delete { name } => {
            let response = client
                .delete_fsm_definition(pacinet_proto::DeleteFsmDefinitionRequest {
                    name: name.clone(),
                })
                .await?
                .into_inner();

            if response.success {
                println!("FSM definition '{}' deleted", name);
            } else {
                eprintln!("{}", response.message);
            }
        }
        FsmCommands::Start {
            name,
            rules,
            counters,
            rate_limit,
            conntrack,
            label,
        } => {
            let rules_yaml = match rules {
                Some(ref path) => std::fs::read_to_string(path)
                    .context(format!("Failed to read rules file: {}", path))?,
                None => String::new(),
            };

            let target_label_filter: std::collections::HashMap<String, String> =
                label.into_iter().collect();

            let response = client
                .start_fsm(pacinet_proto::StartFsmRequest {
                    definition_name: name.clone(),
                    rules_yaml,
                    options: Some(pacinet_proto::CompileOptions {
                        counters,
                        rate_limit,
                        conntrack,
                        axi: false,
                        ports: 1,
                        target: "standalone".to_string(),
                        dynamic: false,
                        dynamic_entries: 16,
                        width: 8,
                        ptp: false,
                        rss: false,
                        rss_queues: 4,
                        int_enabled: false,
                        int_switch_id: 0,
                    }),
                    target_label_filter,
                })
                .await?
                .into_inner();

            if response.success {
                println!("FSM instance started: {}", response.instance_id);
            } else {
                eprintln!("Failed: {}", response.message);
            }
        }
        FsmCommands::InstanceStatus { instance_id } => {
            let response = client
                .get_fsm_instance(pacinet_proto::GetFsmInstanceRequest {
                    instance_id: instance_id.clone(),
                })
                .await?
                .into_inner();

            if let Some(instance) = response.instance {
                if as_json {
                    let val = instance_to_json(&instance);
                    println!("{}", serde_json::to_string_pretty(&val)?);
                } else {
                    println!("Instance:    {}", instance.instance_id);
                    println!("Definition:  {}", instance.definition_name);
                    println!("State:       {}", instance.current_state);
                    println!("Status:      {}", instance.status);
                    println!(
                        "Nodes:       {} deployed, {} failed, {} target",
                        instance.deployed_nodes, instance.failed_nodes, instance.target_nodes
                    );
                    if !instance.history.is_empty() {
                        println!("\nTransition History:");
                        for t in &instance.history {
                            let time = t
                                .timestamp
                                .as_ref()
                                .and_then(|ts| {
                                    chrono::DateTime::from_timestamp(ts.seconds, 0)
                                        .map(|dt| dt.format("%H:%M:%S").to_string())
                                })
                                .unwrap_or_else(|| "?".to_string());
                            if t.from_state.is_empty() {
                                println!(
                                    "  {} -> {} [{}] {}",
                                    time, t.to_state, t.trigger, t.message
                                );
                            } else {
                                println!(
                                    "  {} {} -> {} [{}] {}",
                                    time, t.from_state, t.to_state, t.trigger, t.message
                                );
                            }
                        }
                    }
                }
            } else {
                eprintln!("Instance {} not found", instance_id);
            }
        }
        FsmCommands::Instances { definition, status } => {
            let response = client
                .list_fsm_instances(pacinet_proto::ListFsmInstancesRequest {
                    definition_name: definition.unwrap_or_default(),
                    status: status.unwrap_or_default(),
                })
                .await?
                .into_inner();

            if as_json {
                let instances: Vec<_> = response.instances.iter().map(instance_to_json).collect();
                println!("{}", serde_json::to_string_pretty(&instances)?);
            } else if response.instances.is_empty() {
                println!("No FSM instances");
            } else {
                println!(
                    "{:<38} {:<20} {:<12} {:<10} {:>3}/{:>3}",
                    "INSTANCE ID", "DEFINITION", "STATE", "STATUS", "OK", "FAIL"
                );
                for i in &response.instances {
                    println!(
                        "{:<38} {:<20} {:<12} {:<10} {:>3}/{:>3}",
                        i.instance_id,
                        i.definition_name,
                        i.current_state,
                        i.status,
                        i.deployed_nodes,
                        i.failed_nodes,
                    );
                }
            }
        }
        FsmCommands::Advance { instance_id, state } => {
            let response = client
                .advance_fsm(pacinet_proto::AdvanceFsmRequest {
                    instance_id: instance_id.clone(),
                    target_state: state.unwrap_or_default(),
                })
                .await?
                .into_inner();

            if response.success {
                println!("FSM advanced to state: {}", response.current_state);
            } else {
                eprintln!("Failed: {}", response.message);
            }
        }
        FsmCommands::Cancel { instance_id } => {
            let response = client
                .cancel_fsm(pacinet_proto::CancelFsmRequest {
                    instance_id: instance_id.clone(),
                    reason: "Cancelled by operator".to_string(),
                })
                .await?
                .into_inner();

            if response.success {
                println!("FSM instance {} cancelled", instance_id);
            } else {
                eprintln!("Failed: {}", response.message);
            }
        }
    }

    Ok(())
}

async fn handle_watch(
    action: WatchCommands,
    server: &str,
    as_json: bool,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    let mut client = connect(server, tls_config).await?;

    match action {
        WatchCommands::Fsm { instance } => {
            let mut stream = client
                .watch_fsm_events(pacinet_proto::WatchFsmEventsRequest {
                    instance_id: instance.unwrap_or_default(),
                })
                .await?
                .into_inner();

            while let Some(event) = stream.next().await {
                let event = event?;
                if as_json {
                    let val = json!({
                        "event_type": event.event_type,
                        "instance_id": event.instance_id,
                        "definition_name": event.definition_name,
                        "from_state": event.from_state,
                        "to_state": event.to_state,
                        "trigger": event.trigger,
                        "message": event.message,
                        "deployed_nodes": event.deployed_nodes,
                        "failed_nodes": event.failed_nodes,
                        "target_nodes": event.target_nodes,
                        "final_status": event.final_status,
                    });
                    println!("{}", serde_json::to_string(&val)?);
                } else {
                    let time = event
                        .timestamp
                        .as_ref()
                        .and_then(|t| {
                            chrono::DateTime::from_timestamp(t.seconds, 0)
                                .map(|dt| dt.format("%H:%M:%S").to_string())
                        })
                        .unwrap_or_else(|| "?".to_string());
                    let id_short = if event.instance_id.len() > 8 {
                        &event.instance_id[..8]
                    } else {
                        &event.instance_id
                    };

                    let event_type = pacinet_proto::FsmEventType::try_from(event.event_type)
                        .unwrap_or(pacinet_proto::FsmEventType::FsmEventUnspecified);
                    match event_type {
                        pacinet_proto::FsmEventType::FsmEventTransition => {
                            println!(
                                "{} [{}] {} -> {} ({}) {}",
                                time,
                                id_short,
                                event.from_state,
                                event.to_state,
                                event.trigger,
                                event.message,
                            );
                        }
                        pacinet_proto::FsmEventType::FsmEventDeployProgress => {
                            println!(
                                "{} [{}] deploy progress: {}/{} succeeded, {} failed",
                                time,
                                id_short,
                                event.deployed_nodes,
                                event.target_nodes,
                                event.failed_nodes,
                            );
                        }
                        pacinet_proto::FsmEventType::FsmEventInstanceCompleted => {
                            println!(
                                "{} [{}] instance completed: {}",
                                time, id_short, event.final_status,
                            );
                        }
                        _ => {
                            println!("{} [{}] unknown event", time, id_short);
                        }
                    }
                }
            }
        }
        WatchCommands::Counters { node } => {
            let mut stream = client
                .watch_counters(pacinet_proto::WatchCountersRequest {
                    node_id: node.unwrap_or_default(),
                })
                .await?
                .into_inner();

            while let Some(event) = stream.next().await {
                let event = event?;
                if as_json {
                    let counters: Vec<_> = event
                        .counters
                        .iter()
                        .map(|c| {
                            json!({
                                "rule_name": c.rule_name,
                                "match_count": c.match_count,
                                "byte_count": c.byte_count,
                                "matches_per_second": c.matches_per_second,
                                "bytes_per_second": c.bytes_per_second,
                            })
                        })
                        .collect();
                    let val = json!({
                        "node_id": event.node_id,
                        "counters": counters,
                    });
                    println!("{}", serde_json::to_string(&val)?);
                } else {
                    let time = event
                        .collected_at
                        .as_ref()
                        .and_then(|t| {
                            chrono::DateTime::from_timestamp(t.seconds, 0)
                                .map(|dt| dt.format("%H:%M:%S").to_string())
                        })
                        .unwrap_or_else(|| "?".to_string());
                    println!("{} node={}", time, event.node_id);
                    for c in &event.counters {
                        println!(
                            "  {:<30} {:>8} matches ({:.1}/s)  {:>10} bytes ({:.1}/s)",
                            c.rule_name,
                            c.match_count,
                            c.matches_per_second,
                            c.byte_count,
                            c.bytes_per_second,
                        );
                    }
                }
            }
        }
        WatchCommands::Nodes { label } => {
            let label_filter: std::collections::HashMap<String, String> =
                label.into_iter().collect();
            let mut stream = client
                .watch_node_events(pacinet_proto::WatchNodeEventsRequest { label_filter })
                .await?
                .into_inner();

            while let Some(event) = stream.next().await {
                let event = event?;
                if as_json {
                    let val = json!({
                        "event_type": event.event_type,
                        "node_id": event.node_id,
                        "hostname": event.hostname,
                        "labels": event.labels,
                        "old_state": event.old_state,
                        "new_state": event.new_state,
                    });
                    println!("{}", serde_json::to_string(&val)?);
                } else {
                    let time = event
                        .timestamp
                        .as_ref()
                        .and_then(|t| {
                            chrono::DateTime::from_timestamp(t.seconds, 0)
                                .map(|dt| dt.format("%H:%M:%S").to_string())
                        })
                        .unwrap_or_else(|| "?".to_string());
                    let id_short = if event.node_id.len() > 8 {
                        &event.node_id[..8]
                    } else {
                        &event.node_id
                    };

                    let event_type = pacinet_proto::NodeEventType::try_from(event.event_type)
                        .unwrap_or(pacinet_proto::NodeEventType::NodeEventUnspecified);
                    match event_type {
                        pacinet_proto::NodeEventType::NodeEventRegistered => {
                            println!("{} + {} ({}) registered", time, id_short, event.hostname,);
                        }
                        pacinet_proto::NodeEventType::NodeEventStateChanged => {
                            let old = state_name(event.old_state);
                            let new = state_name(event.new_state);
                            println!(
                                "{} ~ {} ({}) {} -> {}",
                                time, id_short, event.hostname, old, new,
                            );
                        }
                        pacinet_proto::NodeEventType::NodeEventHeartbeatStale => {
                            println!(
                                "{} ! {} ({}) heartbeat stale",
                                time, id_short, event.hostname,
                            );
                        }
                        pacinet_proto::NodeEventType::NodeEventRemoved => {
                            println!("{} - {} ({}) removed", time, id_short, event.hostname,);
                        }
                        _ => {
                            println!("{} ? {} ({}) unknown event", time, id_short, event.hostname,);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_audit(
    server: &str,
    action: Option<String>,
    resource_type: Option<String>,
    limit: u32,
    as_json: bool,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    let mut client = connect(server, tls_config).await?;

    let response = client
        .query_audit_log(pacinet_proto::QueryAuditLogRequest {
            action: action.unwrap_or_default(),
            resource_type: resource_type.unwrap_or_default(),
            resource_id: String::new(),
            since: None,
            limit,
        })
        .await?
        .into_inner();

    if as_json {
        let entries: Vec<_> = response
            .entries
            .iter()
            .map(|e| {
                json!({
                    "id": e.id,
                    "timestamp": e.timestamp.as_ref().map(|t| t.seconds),
                    "actor": e.actor,
                    "action": e.action,
                    "resource_type": e.resource_type,
                    "resource_id": e.resource_id,
                    "details": e.details,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if response.entries.is_empty() {
        println!("No audit entries found");
    } else {
        println!(
            "{:<20} {:<10} {:<16} {:<12} {:<38} DETAILS",
            "TIMESTAMP", "ACTOR", "ACTION", "TYPE", "RESOURCE ID"
        );
        for e in &response.entries {
            let time = e
                .timestamp
                .as_ref()
                .and_then(|t| {
                    chrono::DateTime::from_timestamp(t.seconds, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                })
                .unwrap_or_else(|| "-".to_string());
            let details_short = if e.details.len() > 40 {
                format!("{}...", &e.details[..40])
            } else {
                e.details.clone()
            };
            println!(
                "{:<20} {:<10} {:<16} {:<12} {:<38} {}",
                time, e.actor, e.action, e.resource_type, e.resource_id, details_short
            );
        }
    }

    Ok(())
}

async fn handle_template(
    action: TemplateCommands,
    server: &str,
    as_json: bool,
    pretty: bool,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<()> {
    let mut client = connect(server, tls_config).await?;

    match action {
        TemplateCommands::Create {
            file,
            name,
            description,
            tags,
        } => {
            let rules_yaml = std::fs::read_to_string(&file)
                .context(format!("Failed to read rules file: {}", file))?;

            let tag_list: Vec<String> = if tags.is_empty() {
                vec![]
            } else {
                tags.split(',').map(|s| s.trim().to_string()).collect()
            };

            let response = client
                .create_policy_template(pacinet_proto::CreatePolicyTemplateRequest {
                    name: name.clone(),
                    description,
                    rules_yaml,
                    tags: tag_list,
                })
                .await?
                .into_inner();

            if response.success {
                println!("Template '{}' created", response.name);
            } else {
                eprintln!("Failed: {}", response.message);
            }
        }
        TemplateCommands::List { tag } => {
            let response = client
                .list_policy_templates(pacinet_proto::ListPolicyTemplatesRequest {
                    tag: tag.unwrap_or_default(),
                })
                .await?
                .into_inner();

            if as_json {
                let templates: Vec<_> = response
                    .templates
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "description": t.description,
                            "tags": t.tags,
                            "created_at": t.created_at.as_ref().map(|ts| ts.seconds),
                            "updated_at": t.updated_at.as_ref().map(|ts| ts.seconds),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&templates)?);
            } else if response.templates.is_empty() {
                println!("No templates found");
            } else {
                println!("{:<25} {:<30} TAGS", "NAME", "DESCRIPTION");
                for t in &response.templates {
                    let tags_str = t.tags.join(", ");
                    let desc = if t.description.len() > 28 {
                        format!("{}...", &t.description[..28])
                    } else {
                        t.description.clone()
                    };
                    println!("{:<25} {:<30} {}", t.name, desc, tags_str);
                }
            }
        }
        TemplateCommands::Show { name } => {
            let response = client
                .get_policy_template(pacinet_proto::GetPolicyTemplateRequest { name: name.clone() })
                .await?
                .into_inner();

            if as_json && !pretty {
                let val = json!({
                    "name": response.name,
                    "description": response.description,
                    "tags": response.tags,
                    "rules_yaml": response.rules_yaml,
                    "created_at": response.created_at.as_ref().map(|t| t.seconds),
                    "updated_at": response.updated_at.as_ref().map(|t| t.seconds),
                });
                println!("{}", serde_json::to_string_pretty(&val)?);
            } else {
                println!("Name:        {}", response.name);
                println!("Description: {}", response.description);
                if !response.tags.is_empty() {
                    println!("Tags:        {}", response.tags.join(", "));
                }
                println!("---");
                println!("{}", response.rules_yaml);
            }
        }
        TemplateCommands::Delete { name } => {
            let response = client
                .delete_policy_template(pacinet_proto::DeletePolicyTemplateRequest {
                    name: name.clone(),
                })
                .await?
                .into_inner();

            if response.success {
                println!("Template '{}' deleted", name);
            } else {
                eprintln!("{}", response.message);
            }
        }
        TemplateCommands::Deploy {
            name,
            node,
            counters,
            dry_run,
        } => {
            // Fetch template first
            let template = client
                .get_policy_template(pacinet_proto::GetPolicyTemplateRequest { name: name.clone() })
                .await?
                .into_inner();

            if template.rules_yaml.is_empty() {
                eprintln!("Template '{}' not found or has no rules", name);
                return Ok(());
            }

            // Deploy using the template's rules
            let response = client
                .deploy_policy(pacinet_proto::DeployPolicyRequest {
                    node_id: node.clone(),
                    rules_yaml: template.rules_yaml,
                    options: Some(pacinet_proto::CompileOptions {
                        counters,
                        rate_limit: false,
                        conntrack: false,
                        axi: false,
                        ports: 1,
                        target: "standalone".to_string(),
                        dynamic: false,
                        dynamic_entries: 16,
                        width: 8,
                        ptp: false,
                        rss: false,
                        rss_queues: 4,
                        int_enabled: false,
                        int_switch_id: 0,
                    }),
                    dry_run,
                })
                .await?
                .into_inner();

            if dry_run {
                if let Some(dr) = response.dry_run_result {
                    println!("Dry-run from template '{}':", name);
                    println!("  Valid: {}", dr.valid);
                    for e in &dr.validation_errors {
                        println!("    - {}", e);
                    }
                    for n in &dr.target_nodes {
                        let changed = if n.policy_changed {
                            "changed"
                        } else {
                            "unchanged"
                        };
                        println!("  {} ({}): {}", n.node_id, n.hostname, changed);
                    }
                } else {
                    println!("Dry-run completed");
                }
            } else if response.success {
                println!("Template '{}' deployed to {}", name, node);
            } else {
                eprintln!("Deploy failed: {}", response.message);
            }
        }
    }

    Ok(())
}

fn instance_to_json(instance: &pacinet_proto::FsmInstanceInfo) -> serde_json::Value {
    serde_json::json!({
        "instance_id": instance.instance_id,
        "definition_name": instance.definition_name,
        "current_state": instance.current_state,
        "status": instance.status,
        "deployed_nodes": instance.deployed_nodes,
        "failed_nodes": instance.failed_nodes,
        "target_nodes": instance.target_nodes,
    })
}
