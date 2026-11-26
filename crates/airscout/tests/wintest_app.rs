//! App handlers of the Win-Test commands (stage 4a, Task 7): the pure watchlist operations, the
//! ASSHOWPATH state changes, the play/pause rules of ASADDWATCH/ASREMOVEWATCH and the Manage
//! button predicates.
use airscout::app::{AppState, ControlTab, RunMode, Services};
use airscout::services::{PathRequest, ServiceEvent};
use airscout::wintest_service::{RemoveWatch, ShowPath, WatchEntry, WinTestCommand};
use airscout::wintest_watch::{
    add_watch, manage_enabled, options_watchlist_buttons_enabled, remove_watch, sync_watchlist,
};
use as_aircraft::enums::LifeMode;
use as_aircraft::PlaneInfoCache;
use as_geo::band::Band;
use as_geo::latlon::{bearing, GPoint};
use as_settings::{Settings, Watchlist, WatchlistItem};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
struct Log {
    feeds: Vec<u64>,
    stops: usize,
    requests: usize,
}

#[derive(Clone, Default)]
struct Rec(Rc<RefCell<Log>>);

impl Services for Rec {
    fn request_path(&mut self, _r: PathRequest) {
        self.0.borrow_mut().requests += 1;
    }
    fn start_feed(&mut self, id: u64, _: &Settings, _: Arc<PlaneInfoCache>) -> Result<(), String> {
        self.0.borrow_mut().feeds.push(id);
        Ok(())
    }
    fn stop_feed(&mut self) {
        self.0.borrow_mut().stops += 1;
    }
}

fn my() -> GPoint {
    GPoint::new(50.0208, 10.0417)
}

fn settings() -> Settings {
    Settings {
        my_lat: my().lat,
        my_lon: my().lon,
        dx_lat: 50.0208,
        dx_lon: 12.0417,
        first_run: false,
        ..Settings::default()
    }
}

fn state(s: Settings, rec: &Rec) -> AppState {
    let mut st = AppState::new(
        s,
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    );
    st.integration_gen = 1;
    st
}

fn item(call: &str, loc: &str, checked: bool) -> WatchlistItem {
    WatchlistItem {
        call: call.into(),
        loc: loc.into(),
        checked,
        ..WatchlistItem::default()
    }
}

fn entry(call: &str, loc: &str, pos: GPoint, checked: bool) -> WatchEntry {
    WatchEntry {
        call: call.into(),
        loc: loc.into(),
        pos,
        checked,
    }
}

/// About 143 km east of My.
fn near() -> GPoint {
    GPoint::new(50.0208, 12.0417)
}

/// About 1500 km east of My.
fn far() -> GPoint {
    GPoint::new(50.0208, 32.0)
}

fn wt(st: &mut AppState, cmd: WinTestCommand) {
    st.on_event(ServiceEvent::WinTest {
        generation: st.integration_gen,
        cmd,
    });
}

fn calls(l: &Watchlist) -> Vec<&str> {
    l.0.iter().map(|i| i.call.as_str()).collect()
}

#[test]
fn sync_marks_unchecked_for_removal_and_keeps_checked() {
    let mut l = Watchlist(vec![
        item("AA1AA", "JO50AA", true),
        item("BB1BB", "JO50AA", false),
        item("CC1CC", "JO50AA", false),
    ]);
    let e = [
        entry("CC1CC", "JO50AA", near(), true),
        entry("DD1DD", "JO50AA", near(), true),
    ];
    sync_watchlist(&mut l, &e, my(), 1000.0, 100);
    assert_eq!(calls(&l), ["AA1AA", "CC1CC", "DD1DD"]);
    assert!(l.0[0].checked);
    assert!(!l.0[2].checked, "a synced new item is unchecked");
    assert!(l.0.iter().all(|i| !i.remove || i.checked));
}

#[test]
fn sync_sets_out_of_range() {
    let mut l = Watchlist(vec![item("CC1CC", "JO50AA", false)]);
    let e = [
        entry("CC1CC", "JO50AA", far(), false),
        entry("DD1DD", "JO50AA", far(), false),
        entry("EE1EE", "JO50AA", near(), false),
    ];
    sync_watchlist(&mut l, &e, my(), 1000.0, 100);
    assert!(l.0[0].out_of_range);
    assert!(l.0[1].out_of_range);
    assert!(!l.0[2].out_of_range);
}

#[test]
fn sync_respects_max_count() {
    let mut l = Watchlist::default();
    let e: Vec<_> = (0..5)
        .map(|i| entry(&format!("T{i}ST"), "JO50AA", near(), false))
        .collect();
    sync_watchlist(&mut l, &e, my(), 1000.0, 2);
    assert_eq!(l.0.len(), 2);
}

#[test]
fn sync_cap_counts_kept_items_not_stale_ones() {
    let mut l = Watchlist(vec![
        item("AA1AA", "JO50AA", false),
        item("BB1BB", "JO50AA", false),
    ]);
    let e = [
        entry("CC1CC", "JO50AA", near(), false),
        entry("DD1DD", "JO50AA", near(), false),
        entry("DD1DD", "JO50AA", near(), false),
        entry("EE1EE", "JO50AA", near(), false),
    ];
    sync_watchlist(&mut l, &e, my(), 1000.0, 2);
    assert_eq!(calls(&l), ["CC1CC", "DD1DD"]);
}

#[test]
fn add_watch_respects_max_count() {
    let mut l = Watchlist(vec![item("AA1AA", "JO50AA", true)]);
    let e: Vec<_> = (0..5)
        .map(|i| entry(&format!("T{i}ST"), "JO50AA", near(), true))
        .collect();
    add_watch(&mut l, &e, my(), 1000.0, 3);
    assert_eq!(l.0.len(), 3);
}

#[test]
fn add_watch_checks_only_in_range() {
    let mut l = Watchlist(vec![item("AA1AA", "JO50AA", true)]);
    add_watch(
        &mut l,
        &[entry("AA1AA", "JO50AA", far(), true)],
        my(),
        1000.0,
        10,
    );
    assert!(l.0[0].out_of_range);
    assert!(
        l.0[0].checked,
        "an out-of-range update leaves the mark as it was"
    );
    let mut l = Watchlist(vec![item("AA1AA", "JO50AA", false)]);
    add_watch(
        &mut l,
        &[entry("AA1AA", "JO50AA", far(), true)],
        my(),
        1000.0,
        10,
    );
    assert!(!l.0[0].checked);
    add_watch(
        &mut l,
        &[entry("AA1AA", "JO50AA", near(), true)],
        my(),
        1000.0,
        10,
    );
    assert!(l.0[0].checked && !l.0[0].out_of_range);
}

#[test]
fn add_watch_new_out_of_range_unchecked() {
    let mut l = Watchlist::default();
    add_watch(
        &mut l,
        &[entry("AA1AA", "JO50AA", far(), true)],
        my(),
        1000.0,
        10,
    );
    assert!(l.0[0].out_of_range && !l.0[0].checked);
    add_watch(
        &mut l,
        &[entry("BB1BB", "JO50AA", near(), true)],
        my(),
        1000.0,
        10,
    );
    assert!(!l.0[1].out_of_range && l.0[1].checked);
}

#[test]
fn add_watch_starts_play_when_checked_and_paused() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    wt(
        &mut st,
        WinTestCommand::AddWatch(vec![entry("AA1AA", "JO50AA", near(), true)]),
    );
    assert_eq!(st.mode, RunMode::Playing);
    assert_eq!(rec.0.borrow().feeds.len(), 1);
    // nothing checked: no play
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    wt(
        &mut st,
        WinTestCommand::AddWatch(vec![entry("AA1AA", "JO50AA", near(), false)]),
    );
    assert_eq!(st.mode, RunMode::Paused);
    assert!(rec.0.borrow().feeds.is_empty());
}

#[test]
fn add_watch_no_play_in_history() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.life_mode = LifeMode::History;
    wt(
        &mut st,
        WinTestCommand::AddWatch(vec![entry("AA1AA", "JO50AA", near(), true)]),
    );
    assert_eq!(st.mode, RunMode::Paused);
    assert_eq!(st.settings.watchlist.0.len(), 1);
}

#[test]
fn add_watch_play_refused_during_maintenance() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.maint.running = Some(1);
    wt(
        &mut st,
        WinTestCommand::AddWatch(vec![entry("AA1AA", "JO50AA", near(), true)]),
    );
    assert_eq!(st.mode, RunMode::Paused);
    assert!(rec.0.borrow().feeds.is_empty());
    assert_eq!(st.status, "database maintenance is running");
}

#[test]
fn remove_watch_all_and_calls() {
    let mut l = Watchlist(vec![
        item("TE1ST", "JO50AA", true),
        item("TE1ST", "JO60AA", false),
        item("TE2ND", "JO50AA", false),
    ]);
    remove_watch(&mut l, &RemoveWatch::Calls(vec!["TE1ST".into()]));
    assert_eq!(calls(&l), ["TE2ND"]);
    let mut l = Watchlist(vec![
        item("TE1ST", "JO50AA", true),
        item("TE2ND", "JO50AA", false),
    ]);
    remove_watch(&mut l, &RemoveWatch::All);
    assert!(l.0.is_empty());
}

#[test]
fn remove_watch_calls_many_is_fast() {
    // the call list of a datagram is uncapped: matching must not be O(n * m)
    let mut l = Watchlist(
        (0..1000)
            .map(|i| item(&format!("T{i}"), "JO50AA", false))
            .collect(),
    );
    let many: Vec<String> = (0..30_000).map(|i| format!("X{i}")).collect();
    let t = std::time::Instant::now();
    remove_watch(&mut l, &RemoveWatch::Calls(many));
    assert_eq!(l.0.len(), 1000);
    assert!(t.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn remove_watch_pauses_when_none_checked() {
    let rec = Rec::default();
    let mut s = settings();
    s.watchlist = Watchlist(vec![
        item("AA1AA", "JO50AA", true),
        item("BB1BB", "JO50AA", false),
    ]);
    let mut st = state(s, &rec);
    st.play().unwrap();
    wt(
        &mut st,
        WinTestCommand::RemoveWatch(RemoveWatch::Calls(vec!["BB1BB".into()])),
    );
    assert_eq!(st.mode, RunMode::Playing, "a checked item is left");
    wt(
        &mut st,
        WinTestCommand::RemoveWatch(RemoveWatch::Calls(vec!["AA1AA".into()])),
    );
    assert_eq!(st.mode, RunMode::Paused);
}

fn show(band: Option<Band>) -> WinTestCommand {
    WinTestCommand::ShowPath(ShowPath {
        band,
        my_call: "TE1ST".into(),
        my: my(),
        dx_call: "TE2ND".into(),
        dx: near(),
    })
}

#[test]
fn show_path_sets_stations_heights_band_and_az() {
    let rec = Rec::default();
    let mut st = state(Settings::default(), &rec);
    st.control_tab = ControlTab::Opt;
    wt(&mut st, show(Some(Band::B144M)));
    let s = &st.settings;
    assert_eq!((s.my_call.as_str(), s.dx_call.as_str()), ("TE1ST", "TE2ND"));
    assert_eq!((s.my_lat, s.my_lon), (my().lat, my().lon));
    assert_eq!((s.dx_lat, s.dx_lon), (near().lat, near().lon));
    assert_eq!(s.my_height_m, 10.0);
    assert_eq!(s.dx_height_m, 10.0);
    assert_eq!(s.band, Band::B144M);
    assert_eq!(
        s.track.set_az,
        bearing(my().lat, my().lon, near().lat, near().lon)
    );
    assert_eq!(s.track.set_el, 0.0);
    assert_eq!(st.control_tab, ControlTab::Single);
    assert!(st.alarm_focus);
    assert_eq!(st.mode, RunMode::Playing);
    assert_eq!(st.my_loc_text, "JO50AA");
}

#[test]
fn show_path_pauses_first_when_playing() {
    let rec = Rec::default();
    let mut s = settings();
    s.watchlist = Watchlist(vec![item("AA1AA", "JO50AA", true)]);
    let mut st = state(s, &rec);
    st.control_tab = ControlTab::Multi;
    st.play().unwrap();
    let before = st.feed_gen;
    let stops = rec.0.borrow().stops;
    wt(&mut st, show(None));
    assert!(rec.0.borrow().stops > stops);
    assert_eq!(st.mode, RunMode::Playing);
    assert_eq!(st.control_tab, ControlTab::Single);
    assert!(st.feed_gen > before);
}

#[test]
fn show_path_in_history_does_not_play() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.life_mode = LifeMode::History;
    wt(&mut st, show(Some(Band::B432M)));
    assert_eq!(st.mode, RunMode::Paused);
    assert_eq!(st.settings.dx_call, "TE2ND");
    assert_eq!(st.settings.band, Band::B432M);
    assert_eq!(st.life_mode, LifeMode::History);
}

#[test]
fn show_path_unknown_band_keeps_band() {
    let rec = Rec::default();
    let mut s = settings();
    s.band = Band::B10G;
    let mut st = state(s, &rec);
    wt(&mut st, show(None));
    assert_eq!(st.settings.band, Band::B10G);
}

#[test]
fn stale_wintest_generation_dropped() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.integration_gen = 2;
    st.on_event(ServiceEvent::WinTest {
        generation: 1,
        cmd: WinTestCommand::AddWatch(vec![entry("AA1AA", "JO50AA", near(), true)]),
    });
    st.on_event(ServiceEvent::WinTest {
        generation: 1,
        cmd: show(None),
    });
    assert!(st.settings.watchlist.0.is_empty());
    assert_ne!(st.settings.dx_call, "TE2ND");
    assert_eq!(st.mode, RunMode::Paused);
}

#[test]
fn manage_disabled_only_when_synced_and_server_on() {
    for (sync, server, want) in [
        (false, false, true),
        (false, true, true),
        (true, false, true),
        (true, true, false),
    ] {
        let mut s = Settings {
            watchlist_sync_with_kst: sync,
            ..Settings::default()
        };
        s.server.activate = server;
        assert_eq!(manage_enabled(&s), want, "sync={sync} server={server}");
    }
}

#[test]
fn options_buttons_follow_the_sync_flag_alone() {
    assert!(options_watchlist_buttons_enabled(false));
    assert!(!options_watchlist_buttons_enabled(true));
}
