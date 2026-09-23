use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};

use tracing::info;

/// Statistics of the received data events: how often each event was received and which
/// values have been seen.
#[derive(Default)]
pub struct DataStats {
    events: BTreeMap<String, EventStats>,
}

#[derive(Default)]
struct EventStats {
    count: u64,
    values: HashSet<String>,
}

impl DataStats {
    pub fn record(&mut self, key: &str, value: &str) {
        let stats = self.events.entry(key.to_string()).or_default();
        stats.count += 1;
        if !stats.values.contains(value) {
            stats.values.insert(value.to_string());
        }
    }

    /// One line per event, sorted by event name, e.g. `LampView1 (0, 1, 255): 23`.
    pub fn lines(&self) -> Vec<String> {
        self.events
            .iter()
            .map(|(key, stats)| {
                let mut values: Vec<&str> = stats.values.iter().map(String::as_str).collect();
                values.sort_by(|a, b| compare_values(a, b));
                format!("{} ({}): {}", key, values.join(", "), stats.count)
            })
            .collect()
    }

    /// Writes the statistics to the log.
    pub fn log(&self) {
        if self.events.is_empty() {
            info!("Data event statistics: no data events received.");
            return;
        }
        let total: u64 = self.events.values().map(|s| s.count).sum();
        info!(
            "Data event statistics ({} events, {} different signals):",
            total,
            self.events.len()
        );
        for line in self.lines() {
            info!("  {line}");
        }
    }
}

/// Numbers are sorted numerically and before any non-numeric values.
fn compare_values(a: &str, b: &str) -> Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.total_cmp(&y),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_values_and_counts_events() {
        let mut stats = DataStats::default();
        for value in ["255", "0", "1", "0", "255", "10"] {
            stats.record("LampView1", value);
        }
        stats.record("LampStart", "on");
        stats.record("LampStart", "2");

        assert_eq!(
            stats.lines(),
            ["LampStart (2, on): 2", "LampView1 (0, 1, 10, 255): 6"]
        );
    }
}
