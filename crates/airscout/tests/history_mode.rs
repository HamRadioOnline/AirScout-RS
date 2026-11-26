//! HISTORY mode: player, snapshot tick, enter/leave, range handling.
use airscout::analysis::AnalysisJob;
use airscout::app::{AppState, RunMode, Services};
use airscout::history::{HistoryData, HistoryIndex};
use airscout::jobs::JobEvent;
use airscout::player::{advance, marker_bins, step, PlayerButton};
use airscout::services::{PathRequest, ServiceEvent};
use as_aircraft::enums::LifeMode;
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_propagation::{PathParams, PropagationPath};
use as_settings::{Settings, WatchlistItem};
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

#[derive(Default)]
struct Log {
    bg_starts: usize,
    bg_stops: usize,
    cancels: usize,
    runs: Vec<(u64, AnalysisJob)>,
    requests: usize,
}

#[derive(Clone, Default)]
struct Rec(Rc<RefCell<Log>>);

impl Services for Rec {
    fn request_path(&mut self, _r: PathRequest) {
        self.0.borrow_mut().requests += 1;
    }
    fn start_feed(
        &mut self,
        _i: u64,
        _s: &Settings,
        _c: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop_feed(&mut self) {}
    fn start_background(&mut self, _s: &Settings, _g: u64) {
        self.0.borrow_mut().bg_starts += 1;
    }
    fn stop_background(&mut self) {
        self.0.borrow_mut().bg_stops += 1;
    }
    fn run_analysis(&mut self, id: u64, job: AnalysisJob) {
        self.0.borrow_mut().runs.push((id, job));
    }
    fn cancel_analysis(&mut self) {
        self.0.borrow_mut().cancels += 1;
    }
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, 0).unwrap()
}

fn secs(n: i64) -> DateTime<Utc> {
    t0() + Duration::seconds(n)
}

fn settings() -> Settings {
    let mut s = Settings {
        my_lat: 50.5,
        my_lon: 1.0,
        planes_min_alt_m: 0,
        ..Settings::default()
    };
    let band = s.band;
    s.band_settings.get_mut(&band).unwrap().max_distance = 100.0;
    s
}

fn path() -> PropagationPath {
    PropagationPath::new(
        PathParams {
            lat1: 50.5,
            lon1: 1.0,
            h1: 100.0,
            lat2: 51.5,
            lon2: 3.0,
            h2: 100.0,
            qrg_ghz: 10.0,
            radius_km: 6371.0 * 1.33,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: f64::MIN,
        },
        0.0,
        0.0,
        true,
    )
}

/// `lat`/`lon` 51.3/1.6 on track 135 crosses the path JO50 -> JO61.
fn pos(hex: &str, t: i64, lat: f64) -> PlaneInfo {
    PlaneInfo::new(
        secs(t),
        &format!("C{hex}"),
        "",
        hex,
        lat,
        1.6,
        135.0,
        32000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    )
}

fn data(positions: Vec<PlaneInfo>) -> HistoryData {
    HistoryData {
        oldest: positions.first().map(|p| p.time),
        youngest: positions.last().map(|p| p.time + Duration::minutes(5)),
        count: positions.len() as u64,
        positions,
    }
}

fn state(rec: &Rec) -> AppState {
    AppState::new(
        settings(),
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    )
}

/// AAAAAA is off the path at 0 s and 120 s and crosses it at 60 s; BBBBBB crosses at 500 s.
fn crossing_data() -> HistoryData {
    data(vec![
        pos("AAAAAA", 0, 40.0),
        pos("AAAAAA", 60, 51.3),
        pos("AAAAAA", 120, 40.0),
        pos("BBBBBB", 500, 51.3),
    ])
}

/// Enter HISTORY, deliver `d` and the path.
fn loaded(rec: &Rec, d: HistoryData) -> AppState {
    let mut st = state(rec);
    st.enter_history().unwrap();
    let id = rec.0.borrow().runs.last().unwrap().0;
    st.on_event(ServiceEvent::Job {
        id,
        event: JobEvent::HistoryLoaded(Box::new(d)),
    });
    let (call, p) = ("DX1".to_string(), path());
    st.on_event(ServiceEvent::PathReady {
        id: st.path_gen,
        index: 0,
        call,
        path: p,
        terrain: vec![],
        my_gain_dbi: 0.0,
        dx_gain_dbi: 0.0,
    });
    st
}

fn shown_hexes(st: &AppState) -> Vec<String> {
    st.tick.shown.iter().map(|p| p.hex.clone()).collect()
}

#[test]
fn player_steps_and_back_goes_backwards() {
    assert_eq!(step(0, PlayerButton::Back), -1);
    assert_eq!(step(-1, PlayerButton::Rewind), -11);
    assert_eq!(
        step(step(-1, PlayerButton::Rewind), PlayerButton::Rewind),
        -21
    );
    assert_eq!(step(5, PlayerButton::Pause), 0);
    assert_eq!(step(0, PlayerButton::Forward), 1);
    assert_eq!(step(3595, PlayerButton::FastForward), 3600);
    assert_eq!(step(3600, PlayerButton::Forward), 3600);
    assert_eq!(step(-3600, PlayerButton::Rewind), -3600);
}

#[test]
fn advance_clamps_to_range() {
    assert_eq!(advance(secs(10), 5, secs(0), secs(100)), secs(15));
    assert_eq!(advance(secs(98), 5, secs(0), secs(100)), secs(100));
    assert_eq!(advance(secs(2), -5, secs(0), secs(100)), secs(0));
}

#[test]
fn enter_history_stops_services_and_loads() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.enter_history().unwrap();
    assert_eq!(st.life_mode, LifeMode::History);
    assert_eq!(st.mode, RunMode::Paused);
    let l = rec.0.borrow();
    assert_eq!(l.bg_stops, 1);
    assert_eq!(l.runs.len(), 1);
    assert!(matches!(
        l.runs[0].1,
        AnalysisJob::LoadHistory { ttl_min: 5 }
    ));
    drop(l);
    assert!(st.history.as_ref().unwrap().loading);
}

#[test]
fn enter_history_refused_while_playing() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.play().unwrap();
    assert!(st.enter_history().is_err());
    assert_eq!(st.life_mode, LifeMode::Life);
    assert!(rec.0.borrow().runs.is_empty());
}

#[test]
fn leave_history_restarts_background() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.enter_history().unwrap();
    st.leave_history();
    let l = rec.0.borrow();
    assert_eq!(l.cancels, 1);
    assert_eq!(l.bg_starts, 1);
    drop(l);
    assert!(st.history.is_none());
    assert_eq!(st.life_mode, LifeMode::Life);
    assert_eq!(st.mode, RunMode::Paused);
}

#[test]
fn empty_history_shows_message() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.enter_history().unwrap();
    let id = rec.0.borrow().runs[0].0;
    st.on_event(ServiceEvent::Job {
        id,
        event: JobEvent::HistoryLoaded(Box::default()),
    });
    assert_eq!(st.status, "No positions in the database");
    assert_eq!(rec.0.borrow().requests, 0);
    let h = st.history.as_ref().unwrap();
    assert!(!h.loading, "Load stays enabled");
    assert!(h.markers.is_empty());
}

#[test]
fn loaded_sets_range_and_offline_time() {
    let rec = Rec::default();
    let st = loaded(&rec, crossing_data());
    let h = st.history.as_ref().unwrap();
    assert_eq!(h.min, secs(0));
    assert_eq!(h.max, secs(500) + Duration::minutes(5));
    assert_eq!(st.settings.time.offline, secs(0));
    assert!(!st.settings.time.online);
    assert_eq!(h.markers.len(), 4);
    assert_eq!(rec.0.borrow().requests, 1, "paths are requested, no feed");
}

#[test]
fn history_tick_uses_latest_per_hex_up_to_offline_time() {
    let rec = Rec::default();
    let mut st = loaded(&rec, crossing_data());
    st.settings.time.offline = secs(80);
    st.history.as_mut().unwrap().interval_s = 10;
    assert!(
        st.tick_due(Instant::now()),
        "ticks while the interval is not 0"
    );
    st.on_tick(Utc::now(), &PlaneInfoCache::new());
    // 80 s + 10 s: AAAAAA's 60 s report is the latest, BBBBBB is in the future
    assert_eq!(st.settings.time.offline, secs(90));
    assert_eq!(shown_hexes(&st), ["AAAAAA"]);
    // the snapshot holds the 60 s report (the tick extrapolates its copy to the offline time)
    let h = st.history.as_ref().unwrap();
    assert_eq!(h.cache.latest("AAAAAA").unwrap().time, secs(60));
    assert!(h.cache.latest("BBBBBB").is_none());
    // after 120 s the plane is off the path again
    st.settings.time.offline = secs(115);
    st.on_tick(Utc::now(), &PlaneInfoCache::new());
    assert!(st.tick.shown.is_empty());
}

#[test]
fn paused_player_does_not_tick() {
    let rec = Rec::default();
    let mut st = loaded(&rec, crossing_data());
    assert!(!st.tick_due(Instant::now()));
}

#[test]
fn scroll_refills_snapshot() {
    let rec = Rec::default();
    let mut st = loaded(&rec, crossing_data());
    st.scroll_to(30);
    assert!(st.tick.shown.is_empty(), "AAAAAA is off the path at 0 s");
    st.scroll_to(90);
    assert_eq!(shown_hexes(&st), ["AAAAAA"]);
    assert_eq!(st.settings.time.offline, secs(90));
    st.scroll_to(600);
    assert_eq!(shown_hexes(&st), ["BBBBBB"]);
    // clamped to the range
    st.scroll_to(1_000_000);
    assert_eq!(st.settings.time.offline, st.history.as_ref().unwrap().max);
    st.scroll_to(-5);
    assert_eq!(st.settings.time.offline, secs(0));
}

#[test]
fn inverted_range_rejected() {
    let rec = Rec::default();
    let mut st = loaded(&rec, crossing_data());
    assert!(st.set_history_range(secs(100), secs(50)).is_err());
    assert!(st.set_history_range(secs(100), secs(100)).is_err());
    let h = st.history.as_ref().unwrap();
    assert_eq!((h.min, h.max), (secs(0), secs(500) + Duration::minutes(5)));
    // clamped to the loaded range
    st.set_history_range(secs(-100), secs(10_000)).unwrap();
    let h = st.history.as_ref().unwrap();
    assert_eq!((h.min, h.max), (secs(0), secs(500) + Duration::minutes(5)));
    st.set_history_range(secs(60), secs(200)).unwrap();
    assert_eq!(
        st.settings.time.offline,
        secs(60),
        "offline time is pulled in"
    );
}

#[test]
fn player_button_sets_interval() {
    let rec = Rec::default();
    let mut st = loaded(&rec, crossing_data());
    st.player(PlayerButton::Back);
    assert_eq!(st.history.as_ref().unwrap().interval_s, -1);
    st.player(PlayerButton::Pause);
    assert_eq!(st.history.as_ref().unwrap().interval_s, 0);
}

#[test]
fn single_instant_markers_no_panic() {
    assert_eq!(marker_bins(&[t0()], t0(), t0(), 10), {
        let mut v = vec![false; 10];
        v[0] = true;
        v
    });
    assert!(marker_bins(&[t0()], t0(), t0(), 0).is_empty());
    assert_eq!(marker_bins(&[], t0(), t0(), 3), vec![false; 3]);
    let b = marker_bins(
        &[secs(0), secs(50), secs(100), secs(500)],
        secs(0),
        secs(100),
        11,
    );
    assert!(b[0] && b[5] && b[10]);
    assert_eq!(
        b.iter().filter(|x| **x).count(),
        3,
        "out-of-range marker is dropped"
    );
    // a single-instant history loads and ticks
    let rec = Rec::default();
    let mut st = loaded(&rec, data(vec![pos("AAAAAA", 0, 51.3)]));
    st.history.as_mut().unwrap().interval_s = 1;
    st.on_tick(Utc::now(), &PlaneInfoCache::new());
}

#[test]
fn snapshot_200k_positions_is_fast() {
    let mut positions = Vec::with_capacity(200_000);
    for i in 0..200_000i64 {
        positions.push(pos(
            &format!("{:06X}", i % 2000),
            i,
            50.0 + (i % 7) as f64 * 0.1,
        ));
    }
    let index = HistoryIndex::new(&positions);
    let at = secs(150_000);
    // best of 5: robust against a loaded CI machine
    let best = (0..5)
        .map(|_| {
            let t = Instant::now();
            let r = index.latest_per_hex(&positions, secs(0), at);
            assert_eq!(r.len(), 2000);
            t.elapsed()
        })
        .min()
        .unwrap();
    let bound = if cfg!(debug_assertions) { 50 } else { 30 };
    assert!(best.as_millis() < bound, "snapshot took {best:?}");
}

fn watch(call: &str, loc: &str) -> WatchlistItem {
    WatchlistItem {
        call: call.into(),
        loc: loc.into(),
        ..WatchlistItem::default()
    }
}

/// MapDlg.cs:6094, 5101: a check mark change re-sends the paths while the player plays forward
/// (`PlayMode == FORWARD`); backwards or paused it is ignored.
#[test]
fn history_multi_watchlist_change_resends_paths_only_when_forward() {
    let rec = Rec::default();
    let mut st = loaded(&rec, crossing_data());
    assert!(st.set_control_tab(airscout::app::ControlTab::Multi));
    st.settings.watchlist.0 = vec![watch("DL1ABC", "JO50AA"), watch("DL2ABC", "JO61AA")];
    let before = rec.0.borrow().requests;

    // paused: no re-send
    st.set_watch_checked(0, true);
    st.on_tick(Utc::now(), &PlaneInfoCache::new());
    assert_eq!(rec.0.borrow().requests, before);

    // backwards: no re-send
    st.player(PlayerButton::Back);
    st.set_watch_checked(1, true);
    st.on_tick(Utc::now(), &PlaneInfoCache::new());
    assert_eq!(rec.0.borrow().requests, before);

    // forward: re-send on the next tick
    st.player(PlayerButton::Pause);
    st.player(PlayerButton::Forward);
    st.set_watch_checked(1, false);
    st.on_tick(Utc::now(), &PlaneInfoCache::new());
    assert_eq!(rec.0.borrow().requests, before + 1);
}

#[test]
fn second_leave_history_is_a_noop() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.leave_history();
    assert_eq!(
        rec.0.borrow().bg_starts,
        0,
        "not in HISTORY: nothing to leave"
    );
    assert_eq!(rec.0.borrow().cancels, 0);
    st.enter_history().unwrap();
    st.leave_history();
    st.leave_history();
    assert_eq!(rec.0.borrow().bg_starts, 1);
    assert_eq!(rec.0.borrow().cancels, 1);
}
