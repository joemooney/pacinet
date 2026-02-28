use std::time::Duration;
use tokio::time::Instant;

/// Controller configuration
#[derive(Debug, Clone)]
pub struct ControllerConfig {
    pub deploy_timeout: Duration,
    pub heartbeat_expect_interval: Duration,
    pub heartbeat_miss_threshold: u32,
    pub start_time: Instant,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            deploy_timeout: Duration::from_secs(30),
            heartbeat_expect_interval: Duration::from_secs(30),
            heartbeat_miss_threshold: 3,
            start_time: Instant::now(),
        }
    }
}

impl ControllerConfig {
    /// Duration after which a node with no heartbeats is considered stale.
    pub fn stale_threshold(&self) -> chrono::Duration {
        let secs = self.heartbeat_expect_interval.as_secs() * self.heartbeat_miss_threshold as u64;
        chrono::Duration::seconds(secs as i64)
    }
}
