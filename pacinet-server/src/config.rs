use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

/// Controller configuration
#[derive(Debug, Clone)]
pub struct ControllerConfig {
    pub deploy_timeout: Duration,
    pub heartbeat_expect_interval: Duration,
    pub heartbeat_miss_threshold: u32,
    pub start_time: Instant,
    pub counter_snapshot_retention: Duration,
    pub counter_snapshot_max_per_node: usize,
    /// Leader flag: true when this controller is the active leader (or standalone).
    /// Defaults to true for single-node mode.
    pub is_leader: Arc<AtomicBool>,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            deploy_timeout: Duration::from_secs(30),
            heartbeat_expect_interval: Duration::from_secs(30),
            heartbeat_miss_threshold: 3,
            start_time: Instant::now(),
            counter_snapshot_retention: Duration::from_secs(3600),
            counter_snapshot_max_per_node: 120,
            is_leader: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl ControllerConfig {
    /// Duration after which a node with no heartbeats is considered stale.
    pub fn stale_threshold(&self) -> chrono::Duration {
        let secs = self.heartbeat_expect_interval.as_secs() * self.heartbeat_miss_threshold as u64;
        chrono::Duration::seconds(secs as i64)
    }

    /// Check if this controller is the active leader.
    pub fn is_leader(&self) -> bool {
        self.is_leader.load(Ordering::SeqCst)
    }
}
