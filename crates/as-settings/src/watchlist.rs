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

    /// Add `item` at the front unless its call/locator exists, then keep at most `max` entries by
    /// dropping the oldest (`UpdatePaths`, MapDlg.cs:4455-4462). A `max` of 0 keeps nothing and
    /// returns `false` (a hand-edited `watchlist_max_count = 0` must not silently drop the entry
    /// while reporting success).
    pub fn insert_capped(&mut self, item: WatchlistItem, max: usize) -> bool {
        if max == 0 {
            self.0.clear();
            return false;
        }
        if self.index_of_loc(&item.call, &item.loc).is_some() {
            return false;
        }
        self.0.insert(0, item);
        if self.0.len() > max {
            self.0.truncate(max);
        }
        true
    }
}
