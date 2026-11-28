//! App state machine tests: play/pause, service events, ticks, settings handling.
use airscout::app::{load_settings, AppState, ControlTab, RunMode, Services};
use airscout::background::{Led, LedState};
use airscout::services::{PathRequest, ServiceEvent};
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_elevation::ElevationModel;
use as_propagation::{PathParams, PropagationPath};
use as_settings::{ElevationEnabled, Settings, WatchlistItem};
use chrono::{DateTime, TimeZone, Utc};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
struct Log {
    requests: Vec<PathRequest>,
    feed_starts: usize,
    feed_stops: usize,
    feed_fails: bool,
    feed_ids: Vec<u64>,
    bg_gens: Vec<u64>,
    nfs: Vec<f64>,
    my: Vec<(String, f64, f64)>,
}

#[derive(Clone, Default)]
struct Rec(Rc<RefCell<Log>>);

impl Services for Rec {
    fn request_path(&mut self, req: PathRequest) {
        self.0.borrow_mut().requests.push(req);
    }
    fn start_feed(
        &mut self,
        _id: u64,
        _s: &Settings,
        _c: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        let mut l = self.0.borrow_mut();
        if l.feed_fails {
            return Err("no feed".into());
        }
        l.feed_starts += 1;
        l.feed_ids.push(_id);
        Ok(())
    }
    fn stop_feed(&mut self) {
        self.0.borrow_mut().feed_stops += 1;
    }
    fn start_background(&mut self, _s: &Settings, generation: u64) {
        self.0.borrow_mut().bg_gens.push(generation);
    }
    fn restart_background(&mut self, _s: &Settings, generation: u64) {
        self.0.borrow_mut().bg_gens.push(generation);
    }
    fn set_near_field_suppression(&mut self, m: f64) {
        self.0.borrow_mut().nfs.push(m);
    }
    fn set_my(&mut self, call: &str, lat: f64, lon: f64) {
        self.0.borrow_mut().my.push((call.into(), lat, lon));
    }
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap()
}

fn state_with(s: Settings, rec: &Rec) -> AppState {
    AppState::new(
        s,
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    )
}

fn state(rec: &Rec) -> AppState {
    state_with(Settings::default(), rec)
}

fn ready(id: u64, index: usize, path: PropagationPath, terrain: Vec<f64>) -> ServiceEvent {
    ServiceEvent::PathReady {
        id,
        index,
        call: format!("DX{}", index + 1),
        path,
        terrain,
        my_gain_dbi: 7.0,
        dx_gain_dbi: 9.0,
    }
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

#[test]
fn invalid_locator_blocks_play() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.set_my_loc_text("XX99");
    assert!(st.play().is_err());
    assert_eq!(st.mode, RunMode::Paused);
    assert!(rec.0.borrow().requests.is_empty());
    assert_eq!(rec.0.borrow().feed_starts, 0);

    st.set_my_loc_text("JO50AA");
    st.set_dx_loc_text("1,2,3");
    assert!(st.play().is_err());
    assert_eq!(st.mode, RunMode::Paused);
}

#[test]
fn lat_lon_text_is_accepted() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.set_my_loc_text("50.9, 10.7");
    assert!((st.settings.my_lat - 50.9).abs() < 1e-9);
    assert!((st.settings.my_lon - 10.7).abs() < 1e-9);
    assert!(st.play().is_ok());
}

#[test]
fn play_sends_path_request_and_pause_stops_feed() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.play().unwrap();
    assert_eq!(st.mode, RunMode::Playing);
    {
        let l = rec.0.borrow();
        assert_eq!(l.requests.len(), 1);
        assert_eq!(l.requests[0].my_call, st.settings.my_call);
        assert_eq!(l.requests[0].targets.len(), 1);
        assert_eq!(l.requests[0].targets[0].call, st.settings.dx_call);
        assert_eq!(l.requests[0].my.lat, st.settings.my_lat);
        assert_eq!(l.requests[0].targets[0].pos.lon, st.settings.dx_lon);
        assert_eq!(l.feed_starts, 1);
        assert_eq!(l.feed_stops, 0);
    }
    st.pause();
    assert_eq!(st.mode, RunMode::Paused);
    assert_eq!(rec.0.borrow().feed_stops, 1);
}

#[test]
fn feed_start_failure_keeps_paused() {
    let rec = Rec::default();
    rec.0.borrow_mut().feed_fails = true;
    let mut st = state(&rec);
    assert_eq!(st.play(), Err("no feed".into()));
    assert_eq!(st.mode, RunMode::Paused);
    assert!(st.status.contains("no feed"));
}

#[test]
fn effective_model_from_flags() {
    let flags = |globe, srtm3, srtm1, aster3, aster1| ElevationEnabled {
        globe,
        srtm3,
        srtm1,
        aster3,
        aster1,
    };
    // the default enables GLOBE only
    assert_eq!(
        Settings::default().elevation_enabled.effective_model(),
        ElevationModel::Globe
    );
    assert_eq!(
        flags(true, true, false, false, false).effective_model(),
        ElevationModel::Srtm3
    );
    assert_eq!(
        flags(true, true, true, true, true).effective_model(),
        ElevationModel::Aster1
    );
    assert_eq!(
        flags(false, false, false, false, false).effective_model(),
        ElevationModel::None
    );
    // the request carries the flags; the service derives the model from them
    let rec = Rec::default();
    let mut st = state_with(
        Settings {
            elevation_enabled: flags(false, false, true, false, false),
            ..Settings::default()
        },
        &rec,
    );
    st.play().unwrap();
    let req = rec.0.borrow().requests[0].settings.elevation_enabled;
    assert_eq!(req.effective_model(), ElevationModel::Srtm1);
}

#[test]
fn path_event_updates_state() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.on_event(ready(st.path_gen, 0, path(), vec![1.0, 2.0]));
    let e = st.single_path().unwrap();
    assert_eq!(e.terrain, vec![1.0, 2.0]);
    assert_eq!((e.my_gain_dbi, e.dx_gain_dbi), (7.0, 9.0));
    assert_eq!(e.call, "DX1");

    st.on_event(ServiceEvent::PathFailed {
        id: st.path_gen,
        index: 1,
        error: "boom".into(),
    });
    assert_eq!(st.paths.len(), 1);
    assert!(st.status.contains("boom"));

    st.on_event(ServiceEvent::Status("downloading".into()));
    assert_eq!(st.status, "downloading");
}

fn plane_in_cache(cache: &PlaneInfoCache) {
    let p = PlaneInfo::new(
        now(),
        "CALL1",
        "",
        "abc123",
        51.3,
        1.6,
        135.0,
        30000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    );
    cache.insert_or_update_if_newer(&p);
}

#[test]
fn tick_only_while_playing() {
    let rec = Rec::default();
    let mut s = Settings {
        my_lat: 50.5,
        my_lon: 1.0,
        planes_min_alt_m: 0,
        ..Settings::default()
    };
    let band = s.band;
    s.band_settings.get_mut(&band).unwrap().max_distance = 100.0;
    let cache = Arc::new(PlaneInfoCache::new());
    plane_in_cache(&cache);
    let mut st = AppState::new(s, None, Box::new(rec.clone()), cache.clone());
    st.on_event(ready(st.path_gen, 0, path(), vec![]));

    st.on_tick(now(), &cache);
    assert!(st.tick.shown.is_empty(), "paused: no tick");

    st.play().unwrap();
    // play clears the old path; the new one arrives from the service
    assert!(st.single_path().is_none());
    st.on_event(ready(st.path_gen, 0, path(), vec![]));
    st.on_tick(now(), &cache);
    assert_eq!(st.tick.shown.len(), 1);

    st.pause();
    st.tick = Default::default();
    st.on_tick(now(), &cache);
    assert!(st.tick.shown.is_empty());
}

#[test]
fn corrupt_settings_are_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.toml");
    std::fs::write(&file, "my_call = [not toml").unwrap();
    let (s, err) = load_settings(&file);
    assert_eq!(s, Settings::default());
    let err = err.expect("parse error reported");
    assert!(err.contains("settings.toml"));

    let rec = Rec::default();
    let st = AppState::new(s, Some(err), Box::new(rec), Arc::new(PlaneInfoCache::new()));
    st.save_settings(&file).unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "my_call = [not toml"
    );
}

#[test]
fn good_settings_are_saved() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.toml");
    let (s, err) = load_settings(&file);
    assert!(err.is_none());
    let mut st = AppState::new(
        s,
        None,
        Box::new(Rec::default()),
        Arc::new(PlaneInfoCache::new()),
    );
    st.settings.my_call = "DL1XYZ".into();
    st.save_settings(&file).unwrap();
    assert_eq!(load_settings(&file).0.my_call, "DL1XYZ");
}

#[test]
fn stale_path_result_is_dropped_after_pause_edit_play() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.play().unwrap();
    let old = rec.0.borrow().requests[0].id;
    st.pause();
    st.set_dx_loc_text("JO61UF");
    st.play().unwrap();
    let new = rec.0.borrow().requests[1].id;
    assert_ne!(old, new);

    // the computation of the first request finishes late
    st.on_event(ready(old, 0, path(), vec![1.0]));
    assert!(st.single_path().is_none());
    st.on_event(ServiceEvent::PathFailed {
        id: old,
        index: 0,
        error: "old failure".into(),
    });
    assert!(!st.status.contains("old failure"));

    st.on_event(ready(new, 0, path(), vec![2.0]));
    assert_eq!(st.single_path().unwrap().terrain, vec![2.0]);
}

#[test]
fn elevation_note_survives_path_ready_until_next_play() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.play().unwrap();
    let id = st.path_gen;
    st.on_event(ServiceEvent::PathNote {
        id,
        text: "elevation data incomplete for this path, missing squares: JO50, JO60".into(),
    });
    let mut invalid = path();
    invalid.valid = false;
    st.on_event(ready(id, 0, invalid, vec![]));
    let line = st.status_line();
    assert!(line.contains("path ready"), "{line}");
    assert!(line.contains("JO50") && line.contains("JO60"), "{line}");

    // a stale note is dropped
    st.on_event(ServiceEvent::PathNote {
        id: id + 7,
        text: "stale".into(),
    });
    assert!(!st.status_line().contains("stale"));

    // new Play clears the note; a valid path afterwards shows none
    st.pause();
    st.play().unwrap();
    st.on_event(ready(st.path_gen, 0, path(), vec![]));
    assert!(!st.status_line().contains("JO50"), "{}", st.status_line());
}

#[test]
fn alarm_text_names_call_and_distance() {
    let rec = Rec::default();
    let mut st = state(&rec);
    assert_eq!(st.alarm_text(), None);
    let mut p = PlaneInfo::new(
        now(),
        "DLH123",
        "",
        "abc123",
        51.3,
        1.6,
        135.0,
        30000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    );
    p.int_qrb = 12.6;
    st.tick.alarm = Some(p);
    assert_eq!(st.alarm_text().as_deref(), Some("ALARM: DLH123 13 km"));
}

#[test]
fn play_refused_when_path_exceeds_max_length() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.set_my_loc_text("JO50AA");
    st.set_dx_loc_text("KP20LE"); // Helsinki, > 1000 km
    assert!(st.path_too_long().is_some());
    let e = st.play().unwrap_err();
    assert!(e.contains("1000") && e.contains("km"), "{e}");
    assert_eq!(st.status, e);
    assert_eq!(st.mode, RunMode::Paused);
    assert!(rec.0.borrow().requests.is_empty());
    assert_eq!(rec.0.borrow().feed_starts, 0);

    st.set_dx_loc_text("JO61UF"); // Berlin area, well below 1000 km
    assert!(st.path_too_long().is_none());
    assert!(st.play().is_ok());
}

#[test]
fn late_stopped_of_old_feed_is_ignored() {
    use as_feeds::worker::FeedEvent;
    let rec = Rec::default();
    let mut st = state(&rec);
    st.play().unwrap();
    let old = rec.0.borrow().feed_ids[0];
    st.on_event(ServiceEvent::Feed {
        id: old,
        name: "OpenSky".into(),
        event: FeedEvent::Started,
    });
    st.pause();
    st.play().unwrap();
    let new = rec.0.borrow().feed_ids[1];
    assert_ne!(old, new);
    st.on_event(ServiceEvent::Feed {
        id: new,
        name: "OpenSky".into(),
        event: FeedEvent::Started,
    });
    assert_eq!(st.feed_status, "OpenSky: running");
    // the first worker reports its stop late
    st.on_event(ServiceEvent::Feed {
        id: old,
        name: "OpenSky".into(),
        event: FeedEvent::Stopped,
    });
    st.on_event(ServiceEvent::Feed {
        id: old,
        name: "OpenSky".into(),
        event: FeedEvent::Error("old".into()),
    });
    assert_eq!(st.feed_status, "OpenSky: running");
    assert!(!st.status.contains("old"));
    // the current feed's stop still counts
    st.pause();
    st.on_event(ServiceEvent::Feed {
        id: new,
        name: "OpenSky".into(),
        event: FeedEvent::Stopped,
    });
    assert_eq!(st.feed_status, "off");
}

#[test]
fn multi_results_are_ordered_by_target_index() {
    let rec = Rec::default();
    let mut st = state(&rec);
    let id = st.path_gen;
    st.on_event(ready(id, 1, path(), vec![2.0]));
    st.on_event(ready(id, 0, path(), vec![1.0]));
    st.on_event(ServiceEvent::PathsDone { id });
    let calls: Vec<&str> = st.paths.iter().map(|e| e.call.as_str()).collect();
    assert_eq!(calls, ["DX1", "DX2"]);
    assert_eq!(st.single_path().unwrap().terrain, vec![1.0]);
}

fn item(call: &str, loc: &str, checked: bool, out_of_range: bool) -> WatchlistItem {
    WatchlistItem {
        call: call.into(),
        loc: loc.into(),
        checked,
        out_of_range,
        ..WatchlistItem::default()
    }
}

fn multi_state(rec: &Rec, items: Vec<WatchlistItem>) -> AppState {
    let mut st = state(rec);
    st.settings.watchlist.0 = items;
    assert!(st.set_control_tab(ControlTab::Multi));
    st
}

#[test]
fn multi_play_without_targets_is_refused() {
    let rec = Rec::default();
    let mut st = multi_state(
        &rec,
        vec![
            item("DL1ABC", "JO50AA", false, false),
            item("DL2XYZ", "JO51AA", true, true),
        ],
    );
    let e = st.play().unwrap_err();
    assert_eq!(e, "no stations checked in the watchlist");
    assert!(!st.is_playing());
    assert!(rec.0.borrow().requests.is_empty());
    assert_eq!(rec.0.borrow().feed_starts, 0);
}

#[test]
fn multi_play_sends_checked_in_range_targets() {
    let rec = Rec::default();
    let mut st = multi_state(
        &rec,
        vec![
            item("DL1ABC", "JO50AA", true, false),
            item("DL2XYZ", "JO51AA", true, true),
            item("DL3QQQ", "JO52AA", false, false),
            item("OE1XXX", "JN88AA", true, false),
        ],
    );
    st.play().unwrap();
    let l = rec.0.borrow();
    assert_eq!(l.requests.len(), 1);
    let calls: Vec<&str> = l.requests[0]
        .targets
        .iter()
        .map(|t| t.call.as_str())
        .collect();
    assert_eq!(calls, ["DL1ABC", "OE1XXX"]);
    let p = l.requests[0].targets[0].pos;
    assert_eq!(
        (p.lat, p.lon),
        as_geo::maidenhead::lat_lon_from_loc(
            "JO50AA",
            as_geo::maidenhead::PositionInRectangle::MiddleMiddle
        )
        .unwrap()
    );
    drop(l);
    assert!(st.is_playing());
}

#[test]
fn single_play_inserts_dx_into_watchlist_with_oor_flag() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.settings.path_max_length_km = 100.0;
    st.settings.my_call = "DL0MY".into();
    st.settings.dx_call = "DL9DX".into();
    st.set_dx_loc_text("JO70AA");
    st.set_my_loc_text("JO50AA");
    // too long: refused, nothing inserted
    assert!(st.play().is_err());
    assert!(st.settings.watchlist.0.is_empty());
    st.settings.path_max_length_km = 100_000.0;
    st.play().unwrap();
    assert_eq!(rec.0.borrow().requests[0].targets.len(), 1);
    assert_eq!(st.settings.watchlist.0.len(), 1);
    assert_eq!(st.settings.watchlist.0[0].call, "DL9DX");
    assert_eq!(st.settings.watchlist.0[0].loc, "JO70AA");
    assert!(!st.settings.watchlist.0[0].out_of_range);
    assert_eq!(st.settings.gui.my_calls[0], "DL0MY");
    // second play does not duplicate
    st.pause();
    st.play().unwrap();
    assert_eq!(st.settings.watchlist.0.len(), 1);
}

#[test]
fn dx_watch_item_flags_out_of_range() {
    let mut s = Settings {
        dx_call: "DL9DX".into(),
        my_lat: 50.5,
        my_lon: 8.5,
        dx_lat: 52.5,
        dx_lon: 13.5,
        ..Settings::default()
    };
    let d = as_geo::latlon::distance(s.my_lat, s.my_lon, s.dx_lat, s.dx_lon);
    s.path_max_length_km = d - 1.0;
    let long = airscout::app::dx_watch_item(&s).unwrap();
    assert!(long.out_of_range);
    assert_eq!(long.call, "DL9DX");
    assert_eq!(long.loc.len(), 6);
    s.path_max_length_km = d + 1.0;
    assert!(!airscout::app::dx_watch_item(&s).unwrap().out_of_range);
}

#[test]
fn checking_item_while_playing_resends_paths() {
    let rec = Rec::default();
    let mut st = multi_state(
        &rec,
        vec![
            item("DL1ABC", "JO50AA", true, false),
            item("DL3QQQ", "JO52AA", false, false),
        ],
    );
    st.play().unwrap();
    let first = st.path_gen;
    st.set_watch_checked(1, true);
    assert_eq!(rec.0.borrow().requests.len(), 1, "sent on the next tick");
    st.on_tick(now(), &PlaneInfoCache::new());
    let l = rec.0.borrow();
    assert_eq!(l.requests.len(), 2);
    assert_eq!(l.requests[1].targets.len(), 2);
    assert!(st.path_gen > first);
    assert_eq!(l.requests[1].id, st.path_gen);
    drop(l);
    // no change, no resend
    st.on_tick(now(), &PlaneInfoCache::new());
    assert_eq!(rec.0.borrow().requests.len(), 2);
    // a stale result of the first request is dropped
    st.on_event(ready(first, 0, path(), vec![]));
    assert!(st.paths.is_empty());
}

#[test]
fn tab_change_blocked_while_playing() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.play().unwrap();
    assert!(!st.set_control_tab(ControlTab::Multi));
    assert_eq!(st.control_tab, ControlTab::Single);
    st.pause();
    assert!(st.set_control_tab(ControlTab::Multi));
}

#[test]
fn multi_tick_stores_potential_per_call() {
    let rec = Rec::default();
    let mut st = multi_state(&rec, vec![item("DL1ABC", "JO50AA", true, false)]);
    st.play().unwrap();
    let id = st.path_gen;
    st.on_event(ServiceEvent::PathReady {
        id,
        index: 0,
        call: "DL1ABC".into(),
        path: path(),
        terrain: vec![],
        my_gain_dbi: 0.0,
        dx_gain_dbi: 0.0,
    });
    st.on_event(ServiceEvent::PathsDone { id });
    assert!(st.paths_done());
    st.on_tick(now(), &PlaneInfoCache::new());
    assert_eq!(st.watch_potential.get("DL1ABC"), Some(&0));
}

fn led_of(st: &AppState, led: Led) -> LedState {
    st.leds.iter().find(|l| l.0 == led).unwrap().1
}

fn led_event(generation: u64, state: LedState) -> ServiceEvent {
    ServiceEvent::Led {
        generation,
        led: Led::Stations,
        state,
        text: String::new(),
    }
}

#[test]
fn leds_reset_on_start_and_on_options_ok() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.start_background();
    let g1 = st.background_gen;
    st.on_event(led_event(g1, LedState::UpToDate));
    st.on_event(ServiceEvent::BackgroundIdle {
        generation: g1,
        led: Led::Stations,
    });
    assert_eq!(led_of(&st, Led::Stations), LedState::UpToDate);
    assert!(st.background_idle.contains(&Led::Stations));
    let new = st.settings.clone();
    st.apply_options(new);
    st.options_closed();
    assert_eq!(led_of(&st, Led::Stations), LedState::Unknown);
    assert!(st.background_idle.is_empty());
    assert_eq!(rec.0.borrow().bg_gens, vec![g1, st.background_gen]);
    assert!(st.background_gen > g1);
    // start again: reset too
    st.on_event(led_event(st.background_gen, LedState::Error));
    st.start_background();
    assert_eq!(led_of(&st, Led::Stations), LedState::Unknown);
}

#[test]
fn background_event_of_old_generation_is_ignored() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.start_background();
    let old = st.background_gen;
    let new = st.settings.clone();
    st.apply_options(new);
    st.options_closed();
    st.on_event(led_event(old, LedState::UpToDate));
    st.on_event(ServiceEvent::BackgroundIdle {
        generation: old,
        led: Led::Stations,
    });
    assert_eq!(led_of(&st, Led::Stations), LedState::Unknown);
    assert!(st.background_idle.is_empty());
    st.on_event(led_event(st.background_gen, LedState::UpToDate));
    assert_eq!(led_of(&st, Led::Stations), LedState::UpToDate);
}

#[test]
fn calculations_text_is_shown_reset_and_stale_ones_dropped() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.start_background();
    let gen = st.background_gen;
    let calc = |generation, text: &str| ServiceEvent::Calculations {
        generation,
        text: text.into(),
    };
    st.on_event(calc(gen, "GLOBE checking locations..."));
    assert_eq!(st.calculations, "GLOBE checking locations...");
    // a detached old calculator, after a restart
    st.start_background();
    assert!(st.calculations.is_empty());
    st.on_event(calc(gen, "GLOBE old text"));
    assert!(st.calculations.is_empty());
    st.on_event(calc(st.background_gen, "GLOBE new text"));
    assert_eq!(st.calculations, "GLOBE new text");
}

#[test]
fn services_get_near_field_suppression_and_my_from_the_app() {
    let rec = Rec::default();
    let s = Settings {
        path_near_field_suppression_m: 111.0,
        ..Settings::default()
    };
    let mut st = state_with(s, &rec);
    st.start_background();
    {
        let l = rec.0.borrow();
        assert_eq!(l.nfs, [111.0]);
        assert_eq!(l.my.last().unwrap().0, "DL2ALF");
    }
    // Options OK: the new value goes to the services
    let mut new = st.settings.clone();
    new.path_near_field_suppression_m = 222.0;
    st.apply_options(new);
    assert_eq!(rec.0.borrow().nfs, [111.0, 222.0]);
    // My changes without a restart of the workers: edit of the locator, of the call
    let n = rec.0.borrow().my.len();
    st.set_my_loc_text("JO31AA");
    assert_eq!(rec.0.borrow().my.len(), n + 1);
    st.settings.my_call = "DL9XYZ".into();
    st.sync_my();
    assert_eq!(rec.0.borrow().my.last().unwrap().0, "DL9XYZ");
    // nothing changed: nothing sent
    st.sync_my();
    assert_eq!(rec.0.borrow().my.len(), n + 2);
}

use airscout::sound::Beeper;
use std::cell::Cell;

#[derive(Clone, Default)]
struct CountBeeper(Rc<Cell<usize>>);

impl Beeper for CountBeeper {
    fn beep(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn plane_with_hex(cache: &PlaneInfoCache, hex: &str, lat: f64) {
    let p = PlaneInfo::new(
        now(),
        "CALL1",
        "",
        hex,
        lat,
        1.6,
        135.0,
        30000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    );
    cache.insert_or_update_if_newer(&p);
}

/// A playing app with a path and one plane that raises the alarm.
fn alarm_state(sound: bool, beeper: &CountBeeper) -> (AppState, Arc<PlaneInfoCache>) {
    let mut s = Settings {
        my_lat: 50.5,
        my_lon: 1.0,
        planes_min_alt_m: 0,
        ..Settings::default()
    };
    s.gui.alarm_activate = true;
    s.gui.alarm_play_sound = sound;
    s.gui.alarm_distance_km = 1000.0;
    let band = s.band;
    s.band_settings.get_mut(&band).unwrap().max_distance = 100.0;
    let cache = Arc::new(PlaneInfoCache::new());
    plane_in_cache(&cache);
    let mut st = AppState::new(s, None, Box::new(Rec::default()), cache.clone());
    st.set_beeper(Box::new(beeper.clone()));
    st.play().unwrap();
    st.on_event(ready(st.path_gen, 0, path(), vec![]));
    (st, cache)
}

#[test]
fn alarm_beeps_once_per_new_alarm() {
    let beeper = CountBeeper::default();
    let (mut st, cache) = alarm_state(true, &beeper);
    st.on_tick(now(), &cache);
    assert!(st.tick.alarm.is_some(), "the plane must alarm");
    assert_eq!(beeper.0.get(), 1);
    // the same plane in the next ticks: no new beep
    st.on_tick(now(), &cache);
    st.on_tick(now(), &cache);
    assert_eq!(beeper.0.get(), 1);
    // a second alarming plane beeps once more
    plane_with_hex(&cache, "def456", 51.0);
    st.on_tick(now(), &cache);
    st.on_tick(now(), &cache);
    assert_eq!(st.tick.alarm_hexes.len(), 2);
    assert_eq!(beeper.0.get(), 2);
}

#[test]
fn no_beep_when_sound_disabled() {
    let beeper = CountBeeper::default();
    let (mut st, cache) = alarm_state(false, &beeper);
    st.on_tick(now(), &cache);
    assert!(st.tick.alarm.is_some());
    assert_eq!(beeper.0.get(), 0);
}

#[test]
fn offline_time_skips_plane_updates() {
    let beeper = CountBeeper::default();
    let (mut st, cache) = alarm_state(true, &beeper);
    st.settings.time.online = false;
    st.on_tick(now(), &cache);
    assert!(st.tick.shown.is_empty(), "offline LIFE: no plane update");
    assert_eq!(beeper.0.get(), 0);
    st.settings.time.online = true;
    st.on_tick(now(), &cache);
    assert_eq!(st.tick.shown.len(), 1);
}

#[test]
fn set_time_only_while_paused() {
    let rec = Rec::default();
    let mut st = state(&rec);
    assert!(st.open_set_time_dlg());
    assert!(st.set_time_dlg.is_some());
    st.set_time_dlg = None;
    st.play().unwrap();
    assert!(!st.open_set_time_dlg());
    assert!(st.set_time_dlg.is_none());
    st.pause();
    assert!(st.open_set_time_dlg());
    let t = Utc.with_ymd_and_hms(2020, 1, 2, 3, 4, 5).unwrap();
    st.apply_time(false, t);
    assert!(!st.settings.time.online);
    assert_eq!(st.settings.time.offline, t);
}

#[test]
fn options_ok_refreshes_the_locator_fields() {
    let rec = Rec::default();
    let mut st = state(&rec);
    let mut new = st.settings.clone();
    // My moved from JO50 to JN58 in Options, DX as well
    new.my_lat = 48.5;
    new.my_lon = 11.5;
    new.dx_lat = 47.5;
    new.dx_lon = 9.5;
    st.apply_options(new);
    assert_eq!(st.my_loc_text.get(..6), st.settings.my_loc().as_deref());
    assert!(st.my_loc_text.starts_with("JN58"), "{}", st.my_loc_text);
    assert!(st.dx_loc_text.starts_with("JN47"), "{}", st.dx_loc_text);
    // an edit of the field starts from the new text and does not move My back
    let text = st.my_loc_text.clone();
    st.set_my_loc_text(&text);
    assert_eq!(st.settings.my_loc().as_deref(), text.get(..6));
}

#[test]
fn several_new_alarms_in_one_tick_beep_once() {
    use airscout::sound::AlarmBeeps;
    let mut beeper = CountBeeper::default();
    let mut beeps = AlarmBeeps::default();
    let hexes: Vec<String> = ["a1", "b2", "c3", "d4", "e5", "f6"]
        .iter()
        .map(|h| h.to_string())
        .collect();
    // six planes start to alarm together: one beep, not six stacked ones
    beeps.update(&hexes, true, &mut beeper);
    assert_eq!(beeper.0.get(), 1);
    // no new hex: silent
    beeps.update(&hexes, true, &mut beeper);
    assert_eq!(beeper.0.get(), 1);
    // one more new hex: one beep
    let mut more = hexes.clone();
    more.push("g7".into());
    beeps.update(&more, true, &mut beeper);
    assert_eq!(beeper.0.get(), 2);
}
