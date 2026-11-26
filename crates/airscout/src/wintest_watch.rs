//! Watchlist operations of the Win-Test commands ASWATCHLIST, ASADDWATCH and ASREMOVEWATCH
//! (`MapDlg.cs:7322-7360`, `:7440-7466`, `:7500-7536`), on the plain list.
//!
//! Port deviation: the merged list never grows beyond `max_count` (network input must not grow
//! the watchlist without bound), and the lookups use a hash index instead of a linear scan per
//! entry, so a datagram with thousands of entries costs `O(n + m)`.

use crate::wintest_service::{RemoveWatch, WatchEntry};
use as_geo::latlon::{distance, GPoint};
use as_settings::{Settings, Watchlist, WatchlistItem};
use std::collections::{HashMap, HashSet};

/// Index of the first item per call and locator (`Watchlist.IndexOf(call, loc)`).
fn index(list: &Watchlist) -> HashMap<(String, String), usize> {
    let mut m = HashMap::with_capacity(list.0.len());
    for (i, it) in list.0.iter().enumerate() {
        m.entry((it.call.clone(), it.loc.clone())).or_insert(i);
    }
    m
}

/// Append `item` at the end unless the list already holds `max` entries (`Watchlist.Add`).
fn push_capped(
    list: &mut Watchlist,
    idx: &mut HashMap<(String, String), usize>,
    item: WatchlistItem,
    max: usize,
) {
    if list.0.len() >= max {
        return;
    }
    idx.insert((item.call.clone(), item.loc.clone()), list.0.len());
    list.0.push(item);
}

/// ASWATCHLIST: the list becomes the entries plus the checked items. Order is kept, new items
/// are appended (no sort).
pub fn sync_watchlist(
    list: &mut Watchlist,
    e: &[WatchEntry],
    my: GPoint,
    max_len_km: f64,
    max_count: usize,
) {
    for it in &mut list.0 {
        if !it.checked {
            it.remove = true;
        }
    }
    let idx = index(list);
    let mut added: HashSet<(String, String)> = HashSet::new();
    let mut fresh = Vec::new();
    for w in e {
        let oor = distance(my.lat, my.lon, w.pos.lat, w.pos.lon) > max_len_km;
        let key = (w.call.clone(), w.loc.clone());
        match idx.get(&key) {
            Some(&i) => {
                list.0[i].remove = false;
                list.0[i].out_of_range = oor;
            }
            // the cap counts what stays, so the new items come after the removal
            None if added.insert(key) => fresh.push(WatchlistItem {
                call: w.call.clone(),
                loc: w.loc.clone(),
                out_of_range: oor,
                ..WatchlistItem::default()
            }),
            None => {}
        }
    }
    list.0.retain(|i| !i.remove);
    let room = max_count.saturating_sub(list.0.len());
    list.0.extend(fresh.into_iter().take(room));
}

/// ASADDWATCH: add or update the entries; the check mark is taken only for items in range.
pub fn add_watch(
    list: &mut Watchlist,
    e: &[WatchEntry],
    my: GPoint,
    max_len_km: f64,
    max_count: usize,
) {
    let mut idx = index(list);
    for w in e {
        let oor = distance(my.lat, my.lon, w.pos.lat, w.pos.lon) > max_len_km;
        match idx.get(&(w.call.clone(), w.loc.clone())) {
            Some(&i) => {
                let it = &mut list.0[i];
                it.remove = false;
                it.out_of_range = oor;
                if !oor {
                    it.checked = w.checked;
                }
            }
            None => push_capped(
                list,
                &mut idx,
                WatchlistItem {
                    call: w.call.clone(),
                    loc: w.loc.clone(),
                    out_of_range: oor,
                    checked: !oor && w.checked,
                    ..WatchlistItem::default()
                },
                max_count,
            ),
        }
    }
}

/// ASREMOVEWATCH: `All` empties the list (checked items too); `Calls` removes every item of
/// those calls, whatever the locator.
pub fn remove_watch(list: &mut Watchlist, r: &RemoveWatch) {
    match r {
        RemoveWatch::All => list.0.clear(),
        RemoveWatch::Calls(calls) => {
            let set: HashSet<&str> = calls.iter().map(String::as_str).collect();
            list.0.retain(|i| !set.contains(i.call.as_str()));
        }
    }
}

/// The Manage button of the Multi tab (`btn_Control_Manage_Watchlist`, `MapDlg.cs:3902`,
/// `:6049`): off while the watchlist follows KST and the Win-Test server is on.
pub fn manage_enabled(s: &Settings) -> bool {
    !(s.watchlist_sync_with_kst && s.server.activate)
}

/// The Manage and Clear buttons of the Options watchlist tab (`OptionsDlg.cs:3171-3172`): off
/// while the watchlist follows KST, whatever the server does. Takes the flag of the dialog's
/// draft settings.
pub fn options_watchlist_buttons_enabled(sync_with_kst: bool) -> bool {
    !sync_with_kst
}
