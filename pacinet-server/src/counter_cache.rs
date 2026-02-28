//! In-memory ring buffer of counter snapshots per node.
//!
//! Used by the FSM engine for rate calculation. Not persisted — intentionally
//! separate from the Storage trait to avoid SQLite writes on every heartbeat.

use chrono::{Duration, Utc};
use pacinet_core::CounterSnapshot;
use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;

pub struct CounterSnapshotCache {
    data: RwLock<HashMap<String, VecDeque<CounterSnapshot>>>,
    retention: Duration,
    max_per_node: usize,
}

impl CounterSnapshotCache {
    pub fn new(retention: Duration, max_per_node: usize) -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
            retention,
            max_per_node,
        }
    }

    /// Record a new counter snapshot for a node.
    pub fn record(&self, snapshot: CounterSnapshot) {
        let mut data = self.data.write().unwrap();
        let deque = data.entry(snapshot.node_id.clone()).or_default();
        deque.push_back(snapshot);
        // Enforce max capacity
        while deque.len() > self.max_per_node {
            deque.pop_front();
        }
    }

    /// Get the two most recent snapshots for a node (older, newer).
    pub fn latest_pair(&self, node_id: &str) -> Option<(CounterSnapshot, CounterSnapshot)> {
        let data = self.data.read().unwrap();
        let deque = data.get(node_id)?;
        if deque.len() < 2 {
            return None;
        }
        let len = deque.len();
        Some((deque[len - 2].clone(), deque[len - 1].clone()))
    }

    /// Get the most recent snapshot for a node.
    pub fn latest(&self, node_id: &str) -> Option<CounterSnapshot> {
        let data = self.data.read().unwrap();
        let deque = data.get(node_id)?;
        deque.back().cloned()
    }

    /// Get all snapshots within a time window for a node.
    pub fn snapshots_in_window(&self, node_id: &str, window: Duration) -> Vec<CounterSnapshot> {
        let data = self.data.read().unwrap();
        let deque = match data.get(node_id) {
            Some(d) => d,
            None => return vec![],
        };
        let cutoff = Utc::now() - window;
        deque
            .iter()
            .filter(|s| s.collected_at >= cutoff)
            .cloned()
            .collect()
    }

    /// Remove all snapshots for a node.
    pub fn remove_node(&self, node_id: &str) {
        let mut data = self.data.write().unwrap();
        data.remove(node_id);
    }

    /// Evict snapshots older than the retention period.
    pub fn evict_expired(&self) {
        let mut data = self.data.write().unwrap();
        let cutoff = Utc::now() - self.retention;
        for deque in data.values_mut() {
            while let Some(front) = deque.front() {
                if front.collected_at < cutoff {
                    deque.pop_front();
                } else {
                    break;
                }
            }
        }
        // Remove empty entries
        data.retain(|_, deque| !deque.is_empty());
    }

    /// Get all node IDs that have cached snapshots.
    pub fn node_ids(&self) -> Vec<String> {
        let data = self.data.read().unwrap();
        data.keys().cloned().collect()
    }

    /// Get total snapshot count (for metrics).
    pub fn total_snapshots(&self) -> usize {
        let data = self.data.read().unwrap();
        data.values().map(|d| d.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use pacinet_core::RuleCounter;

    fn make_snapshot(node_id: &str, at: DateTime<Utc>, match_count: u64) -> CounterSnapshot {
        CounterSnapshot {
            node_id: node_id.to_string(),
            collected_at: at,
            counters: vec![RuleCounter {
                rule_name: "drop_all".to_string(),
                match_count,
                byte_count: match_count * 100,
            }],
        }
    }

    #[test]
    fn test_record_and_latest() {
        let cache = CounterSnapshotCache::new(Duration::hours(1), 100);
        let now = Utc::now();

        cache.record(make_snapshot("node-1", now - Duration::seconds(10), 100));
        cache.record(make_snapshot("node-1", now, 200));

        let latest = cache.latest("node-1").unwrap();
        assert_eq!(latest.counters[0].match_count, 200);

        assert!(cache.latest("node-unknown").is_none());
    }

    #[test]
    fn test_latest_pair() {
        let cache = CounterSnapshotCache::new(Duration::hours(1), 100);
        let now = Utc::now();

        // Single snapshot — no pair
        cache.record(make_snapshot("node-1", now - Duration::seconds(10), 100));
        assert!(cache.latest_pair("node-1").is_none());

        // Two snapshots — pair available
        cache.record(make_snapshot("node-1", now, 200));
        let (older, newer) = cache.latest_pair("node-1").unwrap();
        assert_eq!(older.counters[0].match_count, 100);
        assert_eq!(newer.counters[0].match_count, 200);
    }

    #[test]
    fn test_max_per_node_eviction() {
        let cache = CounterSnapshotCache::new(Duration::hours(1), 3);
        let now = Utc::now();

        for i in 0..5 {
            cache.record(make_snapshot(
                "node-1",
                now + Duration::seconds(i),
                (i + 1) as u64 * 100,
            ));
        }

        // Should only have 3 most recent
        let (older, newer) = cache.latest_pair("node-1").unwrap();
        assert_eq!(older.counters[0].match_count, 400);
        assert_eq!(newer.counters[0].match_count, 500);
    }

    #[test]
    fn test_evict_expired() {
        let cache = CounterSnapshotCache::new(Duration::seconds(60), 100);
        let now = Utc::now();

        // Old snapshot (2 minutes ago)
        cache.record(make_snapshot("node-1", now - Duration::seconds(120), 100));
        // Recent snapshot
        cache.record(make_snapshot("node-1", now, 200));

        cache.evict_expired();

        let latest = cache.latest("node-1").unwrap();
        assert_eq!(latest.counters[0].match_count, 200);
        // Only 1 snapshot should remain
        assert!(cache.latest_pair("node-1").is_none());
    }

    #[test]
    fn test_remove_node() {
        let cache = CounterSnapshotCache::new(Duration::hours(1), 100);
        cache.record(make_snapshot("node-1", Utc::now(), 100));
        assert!(cache.latest("node-1").is_some());

        cache.remove_node("node-1");
        assert!(cache.latest("node-1").is_none());
    }

    #[test]
    fn test_snapshots_in_window() {
        let cache = CounterSnapshotCache::new(Duration::hours(1), 100);
        let now = Utc::now();

        cache.record(make_snapshot("node-1", now - Duration::seconds(120), 100));
        cache.record(make_snapshot("node-1", now - Duration::seconds(30), 200));
        cache.record(make_snapshot("node-1", now, 300));

        let snaps = cache.snapshots_in_window("node-1", Duration::seconds(60));
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].counters[0].match_count, 200);
        assert_eq!(snaps[1].counters[0].match_count, 300);
    }
}
