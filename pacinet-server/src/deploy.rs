//! Shared deploy logic used by both ManagementService RPCs and FsmEngine.

use crate::metrics as m;
use crate::storage::blocking;
use pacinet_core::fsm::{ActionResult, NodeActionResult};
use pacinet_core::model::{DeploymentRecord, DeploymentResult, Node, NodeState, Policy};
use pacinet_core::Storage;
use pacinet_proto::*;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Outcome of deploying to a single node.
pub struct DeployOutcome {
    pub success: bool,
    pub message: String,
    pub warnings: Vec<String>,
    pub result: DeploymentResult,
    pub version: u64,
}

/// Deploy to a single node (without acquiring deploy guard — caller must manage that).
pub async fn deploy_to_node(
    storage: &Arc<dyn Storage>,
    node: &Node,
    rules_yaml: &str,
    options: CompileOptions,
    deploy_timeout: std::time::Duration,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> DeployOutcome {
    let options = normalize_options(options);
    if let Err(msg) = ensure_node_capabilities(node, &options) {
        return DeployOutcome {
            success: false,
            message: msg,
            warnings: vec![],
            result: DeploymentResult::AgentFailure,
            version: 0,
        };
    }

    let deploy_start = tokio::time::Instant::now();
    let policy_hash = pacinet_core::policy_hash(rules_yaml);

    // No-op dedupe: if node is already active with identical policy + compile options,
    // skip creating a new policy version and avoid redundant agent deploy.
    let nid = node.node_id.clone();
    let current_policy = match blocking(storage, move |s| s.get_policy(&nid)).await {
        Ok(p) => p,
        Err(e) => {
            return DeployOutcome {
                success: false,
                message: format!("Failed to read current policy: {}", e),
                warnings: vec![],
                result: DeploymentResult::AgentFailure,
                version: 0,
            };
        }
    };
    if matches!(node.state, NodeState::Active)
        && current_policy
            .as_ref()
            .map(|p| policy_matches_request(p, rules_yaml, &policy_hash, &options))
            .unwrap_or(false)
    {
        let nid = node.node_id.clone();
        let current_version = match blocking(storage, move |s| s.get_policy_history(&nid, 1)).await {
            Ok(v) => v.first().map(|p| p.version).unwrap_or(0),
            Err(e) => {
                return DeployOutcome {
                    success: false,
                    message: format!("Failed to read policy history: {}", e),
                    warnings: vec![],
                    result: DeploymentResult::AgentFailure,
                    version: 0,
                };
            }
        };

        let message =
            "No change: identical policy and options already active; skipped redeploy".to_string();

        // Keep deployment history/audit visibility for no-op requests.
        let record = DeploymentRecord {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: node.node_id.clone(),
            policy_version: current_version,
            policy_hash: policy_hash.clone(),
            deployed_at: chrono::Utc::now(),
            result: DeploymentResult::Success,
            message: message.clone(),
        };
        let _ = blocking(storage, move |s| s.record_deployment(record)).await;

        m::record_deploy("success", deploy_start.elapsed().as_secs_f64());

        return DeployOutcome {
            success: true,
            message,
            warnings: vec![],
            result: DeploymentResult::Success,
            version: current_version,
        };
    }

    // Store policy
    let policy = Policy {
        node_id: node.node_id.clone(),
        rules_yaml: rules_yaml.to_string(),
        policy_hash: policy_hash.clone(),
        deployed_at: chrono::Utc::now(),
        counters_enabled: options.counters,
        rate_limit_enabled: options.rate_limit,
        conntrack_enabled: options.conntrack,
        axi_enabled: options.axi,
        ports: options.ports,
        target: options.target.clone(),
        dynamic: options.dynamic,
        dynamic_entries: options.dynamic_entries,
        width: options.width,
        ptp: options.ptp,
        rss: options.rss,
        rss_queues: options.rss_queues,
        int: options.int_enabled,
        int_switch_id: options.int_switch_id,
    };
    let node_id = node.node_id.clone();
    let policy_clone = policy.clone();
    let version = match blocking(storage, move |s| s.store_policy(policy_clone)).await {
        Ok(v) => v,
        Err(e) => {
            return DeployOutcome {
                success: false,
                message: format!("Failed to store policy: {}", e),
                warnings: vec![],
                result: DeploymentResult::AgentFailure,
                version: 0,
            };
        }
    };

    // Set node to Deploying state
    let nid = node.node_id.clone();
    let _ = blocking(storage, move |s| {
        s.update_node_state(&nid, pacinet_core::NodeState::Deploying)
    })
    .await;

    // Forward deploy request to agent via gRPC
    let scheme = if tls_config.is_some() {
        "https"
    } else {
        "http"
    };
    let agent_addr = format!("{}://{}", scheme, node.agent_address);
    debug!(node_id = %node.node_id, agent = %agent_addr, "Forwarding deploy to agent");

    let agent_result = tokio::time::timeout(
        deploy_timeout,
        forward_deploy_to_agent(&agent_addr, rules_yaml, Some(options), tls_config),
    )
    .await;

    let (response_msg, response_warnings, deploy_result) = match agent_result {
        Ok(Ok(response)) => {
            if response.success {
                let nid = node.node_id.clone();
                let _ = blocking(storage, move |s| {
                    s.update_node_state(&nid, pacinet_core::NodeState::Active)
                })
                .await;
                info!(node_id = %node.node_id, "Policy deployed successfully to agent");
                (
                    response.message,
                    response.warnings,
                    DeploymentResult::Success,
                )
            } else {
                let nid = node.node_id.clone();
                let _ = blocking(storage, move |s| {
                    s.update_node_state(&nid, pacinet_core::NodeState::Error)
                })
                .await;
                warn!(node_id = %node.node_id, msg = %response.message, "Agent deploy failed");
                (
                    response.message,
                    response.warnings,
                    DeploymentResult::AgentFailure,
                )
            }
        }
        Ok(Err(e)) => {
            let nid = node.node_id.clone();
            let _ = blocking(storage, move |s| {
                s.update_node_state(&nid, pacinet_core::NodeState::Error)
            })
            .await;
            warn!(node_id = %node.node_id, error = %e, "Failed to connect to agent");
            (
                format!("Failed to reach agent: {}", e),
                vec!["Policy stored locally but agent unreachable".to_string()],
                DeploymentResult::AgentUnreachable,
            )
        }
        Err(_) => {
            let nid = node.node_id.clone();
            let _ = blocking(storage, move |s| {
                s.update_node_state(&nid, pacinet_core::NodeState::Error)
            })
            .await;
            let timeout_secs = deploy_timeout.as_secs();
            warn!(node_id = %node.node_id, "Agent deploy timed out after {}s", timeout_secs);
            (
                format!("Agent communication timed out ({}s)", timeout_secs),
                vec!["Policy stored locally but agent timed out".to_string()],
                DeploymentResult::Timeout,
            )
        }
    };

    // Record metrics
    let duration = deploy_start.elapsed().as_secs_f64();
    m::record_deploy(&deploy_result.to_string(), duration);

    // Record deployment audit
    let record = DeploymentRecord {
        id: uuid::Uuid::new_v4().to_string(),
        node_id,
        policy_version: version,
        policy_hash,
        deployed_at: policy.deployed_at,
        result: deploy_result.clone(),
        message: response_msg.clone(),
    };
    let _ = blocking(storage, move |s| s.record_deployment(record)).await;

    DeployOutcome {
        success: deploy_result == DeploymentResult::Success,
        message: response_msg,
        warnings: response_warnings,
        result: deploy_result,
        version,
    }
}

fn policy_matches_request(
    current: &Policy,
    rules_yaml: &str,
    policy_hash: &str,
    options: &CompileOptions,
) -> bool {
    current.rules_yaml == rules_yaml
        && current.policy_hash == policy_hash
        && current.counters_enabled == options.counters
        && current.rate_limit_enabled == options.rate_limit
        && current.conntrack_enabled == options.conntrack
        && current.axi_enabled == options.axi
        && current.ports == options.ports
        && current.target == options.target
        && current.dynamic == options.dynamic
        && current.dynamic_entries == options.dynamic_entries
        && current.width == options.width
        && current.ptp == options.ptp
        && current.rss == options.rss
        && current.rss_queues == options.rss_queues
        && current.int == options.int_enabled
        && current.int_switch_id == options.int_switch_id
}

fn ensure_node_capabilities(node: &Node, options: &CompileOptions) -> Result<(), String> {
    if options.axi && !capability_true(node, "compile.axi") {
        return Err(format!(
            "Node '{}' does not advertise compile.axi capability",
            node.node_id
        ));
    }
    if options.ports > 1 && !capability_true(node, "compile.ports") {
        return Err(format!(
            "Node '{}' does not advertise compile.ports capability",
            node.node_id
        ));
    }
    if options.dynamic && !capability_true(node, "compile.dynamic") {
        return Err(format!(
            "Node '{}' does not advertise compile.dynamic capability",
            node.node_id
        ));
    }
    if options.width > 8 && !capability_true(node, "compile.width") {
        return Err(format!(
            "Node '{}' does not advertise compile.width capability",
            node.node_id
        ));
    }
    if !options.target.is_empty()
        && options.target != "standalone"
        && !capability_true(node, "compile.target")
    {
        return Err(format!(
            "Node '{}' does not advertise compile.target capability",
            node.node_id
        ));
    }
    if options.ptp && !capability_true(node, "compile.ptp") {
        return Err(format!(
            "Node '{}' does not advertise compile.ptp capability",
            node.node_id
        ));
    }
    if options.rss && !capability_true(node, "compile.rss") {
        return Err(format!(
            "Node '{}' does not advertise compile.rss capability",
            node.node_id
        ));
    }
    if options.rss_queues != 4 && !capability_true(node, "compile.rss_queues") {
        return Err(format!(
            "Node '{}' does not advertise compile.rss_queues capability",
            node.node_id
        ));
    }
    if options.int_enabled && !capability_true(node, "compile.int") {
        return Err(format!(
            "Node '{}' does not advertise compile.int capability",
            node.node_id
        ));
    }
    if options.int_switch_id != 0 && !capability_true(node, "compile.int_switch_id") {
        return Err(format!(
            "Node '{}' does not advertise compile.int_switch_id capability",
            node.node_id
        ));
    }
    Ok(())
}

fn normalize_options(mut options: CompileOptions) -> CompileOptions {
    if options.ports == 0 {
        options.ports = 1;
    }
    if options.dynamic_entries == 0 {
        options.dynamic_entries = 16;
    }
    if options.width == 0 {
        options.width = 8;
    }
    if options.rss_queues == 0 {
        options.rss_queues = 4;
    }
    if options.target.is_empty() {
        options.target = "standalone".to_string();
    }
    options
}

fn capability_true(node: &Node, key: &str) -> bool {
    node.capabilities
        .get(key)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Deploy to multiple nodes, collecting results into an ActionResult.
pub async fn deploy_to_nodes(
    storage: &Arc<dyn Storage>,
    nodes: Vec<Node>,
    rules_yaml: &str,
    options: CompileOptions,
    deploy_timeout: std::time::Duration,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> ActionResult {
    let mut node_results = Vec::new();
    let mut succeeded = 0u32;
    let mut failed = 0u32;
    let total = nodes.len() as u32;

    for node in &nodes {
        // Try acquire deploy guard
        let nid = node.node_id.clone();
        let guard_ok = blocking(storage, move |s| s.begin_deploy(&nid))
            .await
            .is_ok();
        if !guard_ok {
            failed += 1;
            node_results.push(NodeActionResult {
                node_id: node.node_id.clone(),
                success: false,
                message: "Concurrent deploy in progress".to_string(),
            });
            continue;
        }

        let outcome = deploy_to_node(
            storage,
            node,
            rules_yaml,
            options.clone(),
            deploy_timeout,
            tls_config,
        )
        .await;

        // Release deploy guard
        storage.end_deploy(&node.node_id);

        if outcome.success {
            succeeded += 1;
        } else {
            failed += 1;
        }

        node_results.push(NodeActionResult {
            node_id: node.node_id.clone(),
            success: outcome.success,
            message: outcome.message,
        });
    }

    ActionResult {
        succeeded,
        failed,
        total,
        node_results,
    }
}

/// Forward a deploy request to an agent's gRPC endpoint.
pub async fn forward_deploy_to_agent(
    agent_addr: &str,
    rules_yaml: &str,
    options: Option<CompileOptions>,
    tls_config: &Option<pacinet_core::tls::TlsConfig>,
) -> Result<DeployRulesResponse, Box<dyn std::error::Error + Send + Sync>> {
    let mut client = if let Some(tls) = tls_config {
        let client_tls = pacinet_core::tls::load_client_tls(tls)?;
        let channel = tonic::transport::Channel::from_shared(agent_addr.to_string())?
            .tls_config(client_tls)?
            .connect()
            .await?;
        paci_net_agent_client::PaciNetAgentClient::new(channel)
    } else {
        paci_net_agent_client::PaciNetAgentClient::connect(agent_addr.to_string()).await?
    };

    let response = client
        .deploy_rules(DeployRulesRequest {
            rules_yaml: rules_yaml.to_string(),
            options,
        })
        .await?;

    Ok(response.into_inner())
}
