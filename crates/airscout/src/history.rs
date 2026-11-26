//! The loaded position history (`AllPositions` of `MapDlg`) and its per-hex index.

use as_aircraft::PlaneInfo;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

/// Result of the history data getter (`bw_Analysis_DataGetter`, MapDlg.cs:9340-9418).
#[derive(Debug, Clone, Default)]
pub struct HistoryData {
    /// Oldest stored report; `None` on an empty database.
    pub oldest: Option<DateTime<Utc>>,
    /// Youngest stored report plus `Planes_Position_TTL` minutes; `None` on an empty database.
    pub youngest: Option<DateTime<Utc>>,
    /// Number of rows in the database when the load started.
    pub count: u64,
    /// Every position, sorted by time.
    pub positions: Vec<PlaneInfo>,
}

/// Per-hex indices into a time-sorted position list, built once after a load.
#[derive(Debug, Clone, Default)]
pub struct HistoryIndex {
    /// Hex to the indices of its positions; ascending, so also sorted by time.
    by_hex: BTreeMap<String, Vec<u32>>,
}

impl HistoryIndex {
    /// Index `positions`, which must be sorted by time.
    pub fn new(positions: &[PlaneInfo]) -> Self {
        let mut by_hex: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (i, p) in positions.iter().enumerate() {
            match by_hex.get_mut(&p.hex) {
                Some(v) => v.push(i as u32),
                None => {
                    by_hex.insert(p.hex.clone(), vec![i as u32]);
                }
            }
        }
        Self { by_hex }
    }

    /// For each hex the last position with `min <= time <= at`, ordered by hex. Same result as
    /// the linear refill of MapDlg.cs:5384-5391, but O(hexes * log n): one binary search per hex.
    pub fn latest_per_hex(
        &self,
        positions: &[PlaneInfo],
        min: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Vec<PlaneInfo> {
        let mut out = Vec::new();
        for idx in self.by_hex.values() {
            let n = idx.partition_point(|&i| positions[i as usize].time <= at);
            if let Some(&i) = n.checked_sub(1).and_then(|k| idx.get(k)) {
                let p = &positions[i as usize];
                if p.time >= min {
                    out.push(p.clone());
                }
            }
        }
        out
    }

    /// The distinct report times, ascending (the markers of the time slider).
    pub fn markers(&self, positions: &[PlaneInfo]) -> Vec<DateTime<Utc>> {
        let mut out: Vec<DateTime<Utc>> = positions.iter().map(|p| p.time).collect();
        out.dedup();
        out
    }
}
