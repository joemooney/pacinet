//! Lease-based leader election for multi-controller HA.
//!
//! Uses SQLite leader_lease table for coordination. Requires `--db` (SQLite).
//! MemoryStorage always returns "leader" (single-node mode).

use pacinet_core::Storage;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub struct LeaderElection {
    controller_id: String,
    lease_duration: Duration,
    storage: Arc<dyn Storage>,
    is_leader: Arc<AtomicBool>,
}

impl LeaderElection {
    pub fn new(
        controller_id: String,
        lease_duration: Duration,
        storage: Arc<dyn Storage>,
    ) -> Self {
        Self {
            controller_id,
            lease_duration,
            storage,
            is_leader: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_leader_flag(&self) -> Arc<AtomicBool> {
        self.is_leader.clone()
    }

    /// Try to acquire the lease once.
    pub fn try_acquire(&self) -> bool {
        match self
            .storage
            .try_acquire_lease(&self.controller_id, self.lease_duration.as_secs())
        {
            Ok(acquired) => {
                let was_leader = self.is_leader.swap(acquired, Ordering::SeqCst);
                if acquired && !was_leader {
                    info!(
                        controller_id = %self.controller_id,
                        "Acquired leader lease"
                    );
                } else if !acquired && was_leader {
                    warn!(
                        controller_id = %self.controller_id,
                        "Lost leader lease"
                    );
                }
                acquired
            }
            Err(e) => {
                warn!("Leader lease acquisition failed: {}", e);
                false
            }
        }
    }

    /// Background loop: renew lease every lease_duration/2.
    pub async fn run(&self, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
        let renew_interval = self.lease_duration / 2;
        let mut interval = tokio::time::interval(renew_interval);

        // Initial acquisition
        let storage = self.storage.clone();
        let controller_id = self.controller_id.clone();
        let duration_secs = self.lease_duration.as_secs();
        let is_leader = self.is_leader.clone();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let s = storage.clone();
                    let cid = controller_id.clone();
                    let flag = is_leader.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        match s.try_acquire_lease(&cid, duration_secs) {
                            Ok(acquired) => {
                                let was_leader = flag.swap(acquired, Ordering::SeqCst);
                                if acquired && !was_leader {
                                    info!(controller_id = %cid, "Acquired leader lease");
                                } else if !acquired && was_leader {
                                    warn!(controller_id = %cid, "Lost leader lease");
                                }
                            }
                            Err(e) => {
                                warn!("Leader lease renewal failed: {}", e);
                            }
                        }
                    }).await;
                }
                _ = shutdown_rx.changed() => {
                    info!("Leader election loop stopping");
                    return;
                }
            }
        }
    }

    pub fn is_leader(&self) -> bool {
        self.is_leader.load(Ordering::SeqCst)
    }
}
