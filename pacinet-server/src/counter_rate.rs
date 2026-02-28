//! Counter rate calculation from snapshot pairs.

use pacinet_core::CounterSnapshot;

/// Calculated rate for a specific rule counter.
#[derive(Debug, Clone)]
pub struct CounterRate {
    pub rule_name: String,
    pub matches_per_second: f64,
    pub bytes_per_second: f64,
    pub window_seconds: f64,
}

/// How to aggregate rates across multiple nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateMode {
    /// Fire if ANY node exceeds the threshold (default).
    Any,
    /// Fire only if ALL nodes exceed the threshold.
    All,
    /// Sum rates across all nodes, then compare to threshold.
    Sum,
}

/// Parse an aggregate mode string. Defaults to Any for unrecognized values.
pub fn parse_aggregate_mode(s: &str) -> AggregateMode {
    match s.to_lowercase().as_str() {
        "all" => AggregateMode::All,
        "sum" => AggregateMode::Sum,
        _ => AggregateMode::Any,
    }
}

/// Calculate the rate for a specific rule between two snapshots.
/// Returns None if the rule is not found in both snapshots.
/// If newer < older (counter reset), rate = 0 (conservative).
pub fn calculate_rate(
    older: &CounterSnapshot,
    newer: &CounterSnapshot,
    rule_name: &str,
) -> Option<CounterRate> {
    let older_counter = older.counters.iter().find(|c| c.rule_name == rule_name)?;
    let newer_counter = newer.counters.iter().find(|c| c.rule_name == rule_name)?;

    let elapsed = (newer.collected_at - older.collected_at).num_milliseconds() as f64 / 1000.0;
    if elapsed <= 0.0 {
        return None;
    }

    // Counter reset handling: if newer < older, rate = 0
    let match_delta = if newer_counter.match_count >= older_counter.match_count {
        (newer_counter.match_count - older_counter.match_count) as f64
    } else {
        0.0
    };

    let byte_delta = if newer_counter.byte_count >= older_counter.byte_count {
        (newer_counter.byte_count - older_counter.byte_count) as f64
    } else {
        0.0
    };

    Some(CounterRate {
        rule_name: rule_name.to_string(),
        matches_per_second: match_delta / elapsed,
        bytes_per_second: byte_delta / elapsed,
        window_seconds: elapsed,
    })
}

/// Get a specific counter value from a snapshot. Returns (match_count, byte_count).
pub fn get_counter_total(snapshot: &CounterSnapshot, rule_name: &str) -> Option<(u64, u64)> {
    snapshot
        .counters
        .iter()
        .find(|c| c.rule_name == rule_name)
        .map(|c| (c.match_count, c.byte_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use pacinet_core::RuleCounter;

    fn make_snapshot(
        node: &str,
        secs_ago: i64,
        rule: &str,
        matches: u64,
        bytes: u64,
    ) -> CounterSnapshot {
        CounterSnapshot {
            node_id: node.to_string(),
            collected_at: Utc::now() - Duration::seconds(secs_ago),
            counters: vec![RuleCounter {
                rule_name: rule.to_string(),
                match_count: matches,
                byte_count: bytes,
            }],
        }
    }

    #[test]
    fn test_basic_rate_calculation() {
        let older = make_snapshot("n1", 10, "drop_all", 1000, 100000);
        let newer = make_snapshot("n1", 0, "drop_all", 2000, 200000);

        let rate = calculate_rate(&older, &newer, "drop_all").unwrap();
        assert_eq!(rate.rule_name, "drop_all");
        // ~100 matches/sec over 10s window
        assert!((rate.matches_per_second - 100.0).abs() < 1.0);
        assert!((rate.bytes_per_second - 10000.0).abs() < 100.0);
        assert!((rate.window_seconds - 10.0).abs() < 0.5);
    }

    #[test]
    fn test_counter_reset_returns_zero() {
        let older = make_snapshot("n1", 10, "drop_all", 5000, 500000);
        let newer = make_snapshot("n1", 0, "drop_all", 100, 10000); // counter reset

        let rate = calculate_rate(&older, &newer, "drop_all").unwrap();
        assert_eq!(rate.matches_per_second, 0.0);
        assert_eq!(rate.bytes_per_second, 0.0);
    }

    #[test]
    fn test_missing_rule_returns_none() {
        let older = make_snapshot("n1", 10, "drop_all", 1000, 100000);
        let newer = make_snapshot("n1", 0, "drop_all", 2000, 200000);

        assert!(calculate_rate(&older, &newer, "nonexistent").is_none());
    }

    #[test]
    fn test_zero_elapsed_returns_none() {
        let now = Utc::now();
        let s1 = CounterSnapshot {
            node_id: "n1".to_string(),
            collected_at: now,
            counters: vec![RuleCounter {
                rule_name: "r".to_string(),
                match_count: 100,
                byte_count: 1000,
            }],
        };
        let s2 = s1.clone();
        assert!(calculate_rate(&s1, &s2, "r").is_none());
    }

    #[test]
    fn test_get_counter_total() {
        let snap = make_snapshot("n1", 0, "drop_all", 42, 4200);
        let (m, b) = get_counter_total(&snap, "drop_all").unwrap();
        assert_eq!(m, 42);
        assert_eq!(b, 4200);
        assert!(get_counter_total(&snap, "nonexistent").is_none());
    }

    #[test]
    fn test_aggregate_mode_parsing() {
        assert_eq!(parse_aggregate_mode("any"), AggregateMode::Any);
        assert_eq!(parse_aggregate_mode("all"), AggregateMode::All);
        assert_eq!(parse_aggregate_mode("sum"), AggregateMode::Sum);
        assert_eq!(parse_aggregate_mode("ANY"), AggregateMode::Any);
        assert_eq!(parse_aggregate_mode("unknown"), AggregateMode::Any);
    }
}
