//! Tile cache reproducing `ElevationDatabase.GetElevationTile` (last-tile fast path plus an
//! insertion-ordered dictionary).

use crate::ElevationTile;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct TileCache {
    last: Option<Arc<ElevationTile>>,
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

    pub(crate) fn get(&mut self, loc: &str) -> Option<Arc<ElevationTile>> {
        if let Some(tile) = &self.last {
            if tile.index == loc {
                return Some(tile.clone());
            }
        }
        let tile = self.map.get(loc)?.clone();
        self.last = Some(tile.clone());
        Some(tile)
    }

    /// Called after a tile was loaded from the database.
    pub(crate) fn offer(&mut self, loc: &str, tile: Arc<ElevationTile>) {
        if self.capacity == 0 {
            return;
        }
        if !self.map.contains_key(loc) {
            if self.map.len() >= self.capacity {
                if let Some(old) = self.order.pop_front() {
                    self.map.remove(&old);
                }
            }
            self.order.push_back(loc.to_string());
        }
        self.last = Some(tile.clone());
        self.map.insert(loc.to_string(), tile);
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
