//! Tile cache reproducing `ElevationDatabase.GetElevationTile` (last-tile fast path plus an
//! insertion-ordered dictionary).

use crate::ElevationTile;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Reproduce `logic-findings.md` #5: when the cache is over capacity, evict the oldest entry
/// and do NOT insert the new tile (C# `if (cache.Count > size) cache.RemoveAt(0); else cache.Add`).
pub(crate) const LEGACY_CACHE_BEHAVIOUR: bool = true;

#[derive(Default)]
pub(crate) struct TileCache {
    pub(crate) last: Option<Arc<ElevationTile>>,
    order: VecDeque<String>,
    map: HashMap<String, Arc<ElevationTile>>,
    capacity: usize,
}

impl TileCache {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            ..Default::default()
        }
    }

    pub(crate) fn get(&self, loc: &str) -> Option<Arc<ElevationTile>> {
        self.map.get(loc).cloned()
    }

    /// Called after a tile was loaded from the database.
    pub(crate) fn offer(&mut self, loc: &str, tile: Arc<ElevationTile>) {
        if self.map.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
            if LEGACY_CACHE_BEHAVIOUR {
                return;
            }
        }
        if self.map.insert(loc.to_string(), tile).is_none() {
            self.order.push_back(loc.to_string());
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.last = None;
    }
}
