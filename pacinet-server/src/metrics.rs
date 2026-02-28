use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tracing::info;

/// Start the Prometheus metrics HTTP server on the given port.
/// Returns a JoinHandle for the background task.
pub fn install_metrics(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;
    info!("Prometheus metrics endpoint on http://{}/metrics", addr);

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()?;

    Ok(())
}

/// Record a deploy attempt with result label.
pub fn record_deploy(result: &str, duration_secs: f64) {
    metrics::counter!("pacinet_deploys_total", "result" => result.to_string()).increment(1);
    metrics::histogram!("pacinet_deploy_duration_seconds").record(duration_secs);
}

/// Record a heartbeat received.
pub fn record_heartbeat() {
    metrics::counter!("pacinet_heartbeats_total").increment(1);
}

/// Record a missed heartbeat (stale detection).
pub fn record_heartbeat_missed(count: u64) {
    metrics::counter!("pacinet_heartbeats_missed_total").increment(count);
}

/// Record a batch deploy operation.
pub fn record_batch_deploy(succeeded: u32, failed: u32) {
    metrics::counter!("pacinet_batch_deploys_total").increment(1);
    metrics::counter!("pacinet_batch_deploy_nodes", "result" => "success")
        .increment(succeeded as u64);
    metrics::counter!("pacinet_batch_deploy_nodes", "result" => "failure").increment(failed as u64);
}

/// Update node gauge metrics.
pub fn update_node_gauges(total: usize, by_state: &std::collections::HashMap<String, usize>) {
    metrics::gauge!("pacinet_nodes_total").set(total as f64);
    for (state, count) in by_state {
        metrics::gauge!("pacinet_nodes_by_state", "state" => state.clone()).set(*count as f64);
    }
}

/// Record controller uptime.
pub fn record_uptime(seconds: f64) {
    metrics::gauge!("pacinet_controller_uptime_seconds").set(seconds);
}

/// Record an FSM state transition.
pub fn record_fsm_transition() {
    metrics::counter!("pacinet_fsm_transitions_total").increment(1);
}

/// Record an FSM instance status change.
pub fn record_fsm_instance_status(status: &str) {
    metrics::counter!("pacinet_fsm_instances_total", "status" => status.to_string()).increment(1);
}

/// Update FSM running instances gauge.
pub fn update_fsm_running_gauge(count: usize) {
    metrics::gauge!("pacinet_fsm_instances_running").set(count as f64);
}

/// Record a counter snapshot ingestion.
pub fn record_counter_snapshot() {
    metrics::counter!("pacinet_counter_snapshots_total").increment(1);
}

/// Update counter snapshot cache gauge.
pub fn update_counter_snapshot_gauge(count: usize) {
    metrics::gauge!("pacinet_counter_snapshots_cached").set(count as f64);
}

/// Record a webhook delivery attempt.
pub fn record_webhook_delivery(result: &str) {
    metrics::counter!("pacinet_webhook_deliveries_total", "result" => result.to_string())
        .increment(1);
}

/// Record a counter condition evaluation.
pub fn record_counter_eval(result: &str) {
    metrics::counter!("pacinet_counter_evals_total", "result" => result.to_string()).increment(1);
}
