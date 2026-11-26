//! Watchlist of calls (`Settings.Watchlist`, `WatchlistItem` of the original).

/// One watchlist row.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WatchlistItem {
    pub call: String,
    pub loc: String,
    pub checked: bool,
    pub selected: bool,
    pub out_of_range: bool,
    pub remove: bool,
}

/// The watchlist; serializes as a plain array of tables.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Watchlist(pub Vec<WatchlistItem>);

impl Watchlist {
    /// Index of the first item with this call.
    pub fn index_of(&self, call: &str) -> Option<usize> {
        self.0.iter().position(|i| i.call == call)
    }

    /// Index of the item with this call and locator.
    pub fn index_of_loc(&self, call: &str, loc: &str) -> Option<usize> {
        self.0.iter().position(|i| i.call == call && i.loc == loc)
    }

    /// Sort by call, then locator (`WatchlistItem.CompareTo`).
    pub fn sort(&mut self) {
        self.0
            .sort_by(|a, b| a.call.cmp(&b.call).then_with(|| a.loc.cmp(&b.loc)));
    }

    /// Add `item` unless its call/locator exists or the list already holds `max` entries.
    pub fn insert_capped(&mut self, item: WatchlistItem, max: usize) -> bool {
        if self.0.len() >= max || self.index_of_loc(&item.call, &item.loc).is_some() {
            return false;
        }
        self.0.push(item);
        true
    }
}
