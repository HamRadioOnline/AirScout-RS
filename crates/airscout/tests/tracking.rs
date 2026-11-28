//! Tracking values (`bw_Track` maths, `MapDlg.cs:8054-8281`) and the mode transitions.
use airscout::app::{AppState, Services};
use airscout::services::{PathRequest, ServiceEvent};
use airscout::tracker::{TrackInput, TrackParams, TrackShown, Tracker};
use airscout::tracking::{rx_tx, status_colors, track_values, valid, TrackStatus, TrackValues};
use as_aircraft::enums::{LifeMode, TrackMode};
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_geo::latlon::GPoint;
use as_geo::latlon::{bearing, distance};
use as_geo::propagation::{epsilon_from_heights, slant_range_from_heights};
use as_propagation::{PathParams, PropagationPath};
use as_settings::{DopplerStrategy, Settings};
use chrono::{DateTime, Duration, TimeZone, Utc};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 12, 0, 0).unwrap()
}

fn settings() -> Settings {
    let mut s = Settings {
        my_lat: 50.5,
        my_lon: 1.0,
        dx_lat: 51.5,
        dx_lon: 3.0,
        planes_min_alt_m: 0,
        ..Settings::default()
    };
    let band = s.band;
    s.band_settings.get_mut(&band).unwrap().max_distance = 100.0;
    s.track.activate = true;
    s
}

fn plane(hex: &str, time: DateTime<Utc>, lat: f64, lon: f64) -> PlaneInfo {
    PlaneInfo::new(
        time,
        &format!("C{hex}"),
        "",
        hex,
        lat,
        lon,
        135.0,
        32000.0,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    )
}

#[test]
fn track_values_az_el_dist_slant() {
    let s = settings();
    let p = plane("A1B2C3", t0(), 51.0, 2.0);
    let (my_h, dx_h) = (130.0, 80.0);
    let v = track_values(
        t0(),
        Some(&p),
        TrackMode::Track,
        &TrackParams::from_settings(&s),
        my_h,
        dx_h,
        None,
    );
    // the radius carries the K factor of the band, the distance does not
    let r = 6371.0 * s.current_band_settings().k_factor;
    let alt = p.alt_m();
    assert_eq!(v.hex, "A1B2C3");
    assert_eq!(v.timestamp, t0());
    assert_eq!(v.my_azimuth, bearing(50.5, 1.0, 51.0, 2.0));
    assert_eq!(v.dx_azimuth, bearing(51.5, 3.0, 51.0, 2.0));
    assert_eq!(v.my_distance, distance(50.5, 1.0, 51.0, 2.0));
    assert_eq!(v.dx_distance, distance(51.5, 3.0, 51.0, 2.0));
    assert_eq!(
        v.my_slant_range,
        slant_range_from_heights(my_h, v.my_distance, alt, r)
    );
    assert_eq!(
        v.dx_slant_range,
        slant_range_from_heights(dx_h, v.dx_distance, alt, r)
    );
    assert_eq!(
        v.my_elevation,
        epsilon_from_heights(my_h, v.my_distance, alt, r).to_degrees()
    );
    assert_eq!(
        v.dx_elevation,
        epsilon_from_heights(dx_h, v.dx_distance, alt, r).to_degrees()
    );
    // a plane at 9.8 km height, 100 km away, is seen under a small positive angle
    assert!(v.my_elevation > 0.0 && v.my_elevation < 20.0, "{v:?}");
    assert!(valid(&v));
    assert_eq!(
        (v.my_doppler, v.dx_doppler, v.rx_hz, v.tx_hz),
        (0.0, 0.0, 0, 0)
    );
}

#[test]
fn missing_plane_gives_invalid_values() {
    let v = track_values(
        t0(),
        None,
        TrackMode::Track,
        &TrackParams::from_settings(&settings()),
        0.0,
        0.0,
        None,
    );
    assert_eq!(v.hex, "");
    assert!(v.my_azimuth.is_nan() && v.dx_slant_range.is_nan());
    assert!(!valid(&v));
}

/// `old` at `t0`, new values `dt` seconds later with the slant ranges changed by `delta` [km].
fn doppler_case(old_hex: &str, dt: i64, delta: f64) -> TrackValues {
    let s = settings();
    let p = plane("A1B2C3", t0() + Duration::seconds(dt), 51.0, 2.0);
    let probe = track_values(
        p.time,
        Some(&p),
        TrackMode::Track,
        &TrackParams::from_settings(&s),
        0.0,
        0.0,
        None,
    );
    let mut old = TrackValues::new(t0());
    old.hex = old_hex.into();
    old.my_slant_range = probe.my_slant_range + delta;
    old.dx_slant_range = probe.dx_slant_range - delta;
    track_values(
        p.time,
        Some(&p),
        TrackMode::Track,
        &TrackParams::from_settings(&s),
        0.0,
        0.0,
        Some(&old),
    )
}

#[test]
fn doppler_needs_previous_same_hex() {
    // no old values, other plane: zero
    let s = settings();
    let p = plane("A1B2C3", t0(), 51.0, 2.0);
    let v = track_values(
        t0(),
        Some(&p),
        TrackMode::Track,
        &TrackParams::from_settings(&s),
        0.0,
        0.0,
        None,
    );
    assert_eq!((v.my_doppler, v.dx_doppler), (0.0, 0.0));
    let v = doppler_case("FFFFFF", 1, 0.1);
    assert_eq!((v.my_doppler, v.dx_doppler), (0.0, 0.0));
    // same plane, one second earlier: shifted
    let v = doppler_case("A1B2C3", 1, 0.1);
    assert!(v.my_doppler != 0.0 && v.dx_doppler != 0.0);
    // no time elapsed: speed 0, no shift
    let v = doppler_case("A1B2C3", 0, 0.1);
    assert!(
        v.my_doppler.abs() < 1e-6 && v.dx_doppler.abs() < 1e-6,
        "{v:?}"
    );
}

#[test]
fn doppler_sign_positive_when_approaching() {
    // old slant range 0.1 km longer: 100 m/s closer, MY approaches and DX recedes
    let v = doppler_case("A1B2C3", 1, 0.1);
    let hz = settings().band.to_hz();
    let c = 300_000_000.0;
    // speed = (old - new) * 1000 / dt; shift = qrg - qrg * c / (c + speed)
    let want = hz - hz * c / (c + 100.0);
    assert!(v.my_doppler > 0.0, "{v:?}");
    assert!(
        (v.my_doppler - want).abs() < 1e-6,
        "{} vs {want}",
        v.my_doppler
    );
    assert!(v.dx_doppler < 0.0, "{v:?}");
}

#[test]
fn single_mode_uses_set_az_el() {
    let mut s = settings();
    s.track.set_az = 123.5;
    s.track.set_el = 12.25;
    let p = plane("A1B2C3", t0(), 51.0, 2.0);
    // a plane is ignored; nothing else is set
    let v = track_values(
        t0(),
        Some(&p),
        TrackMode::Single,
        &TrackParams::from_settings(&s),
        0.0,
        0.0,
        None,
    );
    assert_eq!((v.my_azimuth, v.my_elevation), (123.5, 12.25));
    assert_eq!(v.hex, "");
    assert!(v.dx_azimuth.is_nan() && v.my_distance.is_nan() && v.my_slant_range.is_nan());
    assert_eq!(v.my_doppler, 0.0);
    assert!(valid(&v));
}

#[test]
fn none_is_invalid() {
    let p = plane("A1B2C3", t0(), 51.0, 2.0);
    let v = track_values(
        t0(),
        Some(&p),
        TrackMode::None,
        &TrackParams::from_settings(&settings()),
        0.0,
        0.0,
        None,
    );
    assert!(v.my_azimuth.is_nan() && v.my_elevation.is_nan());
    assert!(!valid(&v));
    // azimuth range 0 <= az < 360
    let mut v = TrackValues::new(t0());
    v.my_elevation = 0.0;
    for (az, ok) in [(0.0, true), (359.99, true), (360.0, false), (-0.1, false)] {
        v.my_azimuth = az;
        assert_eq!(valid(&v), ok, "{az}");
    }
    v.my_azimuth = 10.0;
    v.my_elevation = f64::NAN;
    assert!(!valid(&v));
}

#[test]
fn rx_tx_strategies() {
    // the Dopplers are cast to i64 one by one: 100.7 -> 100, -50.9 -> -50
    let (my, dx, dial) = (100.7, -50.9, 144_174_000);
    assert_eq!(rx_tx(DopplerStrategy::A, dial, my, dx), (dial + 50, dial));
    assert_eq!(rx_tx(DopplerStrategy::B, dial, my, dx), (dial, dial - 50));
    assert_eq!(
        rx_tx(DopplerStrategy::C, dial, my, dx),
        (dial + 50, dial - 50)
    );
    assert_eq!(
        rx_tx(DopplerStrategy::D, dial, my, dx),
        (dial + 100, dial - 100)
    );
    assert_eq!(rx_tx(DopplerStrategy::None, dial, my, dx), (0, 0));
    // 0.9 + 0.9 is 1 as a sum but 0 cast one by one
    assert_eq!(rx_tx(DopplerStrategy::A, dial, 0.9, 0.9), (dial, dial));
}

#[test]
fn rx_tx_at_dial_limit_does_not_overflow() {
    // 1 MHz each is far beyond any aircraft Doppler, even at 100 GHz
    let dial = as_cat::MAX_FREQ_HZ;
    for s in [
        DopplerStrategy::A,
        DopplerStrategy::B,
        DopplerStrategy::C,
        DopplerStrategy::D,
    ] {
        let (rx, tx) = rx_tx(s, dial, 1e6, 1e6);
        assert!(rx > 0 && tx > 0, "{s:?}");
        let (rx, tx) = rx_tx(s, dial, -1e6, -1e6);
        assert!(rx > 0 && tx > 0, "{s:?}");
    }
}

#[test]
fn status_colours() {
    let dark_gray = [169, 169, 169];
    let control = [240, 240, 240];
    assert_eq!(status_colors(TrackStatus::None), (dark_gray, control));
    assert_eq!(status_colors(TrackStatus::Stopped), (control, dark_gray));
    let on = ([255, 255, 255], [0, 100, 0]);
    assert_eq!(status_colors(TrackStatus::Single), on);
    assert_eq!(status_colors(TrackStatus::Tracking), on);
    assert_eq!(
        status_colors(TrackStatus::Error),
        ([255, 255, 0], [255, 0, 0])
    );
}

#[derive(Default)]
struct Log {
    /// The elevation requests (id, My, DX); the test delivers the answer.
    requests: Vec<(u64, GPoint, GPoint)>,
    /// Every `track_input` push.
    inputs: Vec<TrackInput>,
    /// The fake Track worker: its `oldvalues` and what it showed last.
    tracker: Tracker,
    shown: Option<TrackShown>,
}

#[derive(Clone, Default)]
struct Rec(Rc<RefCell<Log>>);

impl Rec {
    fn requests(&self) -> usize {
        self.0.borrow().requests.len()
    }
    fn last_input(&self) -> TrackInput {
        self.0.borrow().inputs.last().cloned().expect("an input")
    }
    /// One step of a fake Track worker over the last pushed input, as the real one does once a
    /// second.
    fn worker_step(&self, now: DateTime<Utc>, cache: &PlaneInfoCache) {
        let mut l = self.0.borrow_mut();
        let Some(input) = l.inputs.last().cloned() else {
            return;
        };
        if let Some((_, shown)) = l.tracker.step(&input, now, cache) {
            l.shown = Some(shown);
        }
    }
    /// Answer the newest request: every place is 100 m high.
    fn answer(&self, st: &mut AppState) {
        let id = self.0.borrow().requests.last().unwrap().0;
        st.on_event(ServiceEvent::TrackHeights {
            id,
            my_m: 100.0,
            dx_m: 100.0,
        });
    }
}

impl Services for Rec {
    fn request_path(&mut self, _r: PathRequest) {}
    fn start_feed(
        &mut self,
        _i: u64,
        _s: &Settings,
        _c: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop_feed(&mut self) {}
    fn request_ground_elevations(&mut self, id: u64, my: GPoint, dx: GPoint) {
        self.0.borrow_mut().requests.push((id, my, dx));
    }
    fn track_input(&mut self, i: TrackInput) {
        self.0.borrow_mut().inputs.push(i);
    }
    fn track_shown(&self) -> Option<TrackShown> {
        self.0.borrow().shown.clone()
    }
}

/// A frame, a step of the worker at `now`, a frame: what the UI sees of one step.
fn tick(st: &mut AppState, rec: &Rec, now: DateTime<Utc>) {
    st.track_sync();
    let cache = st.cache.clone();
    rec.worker_step(now, &cache);
    st.track_sync();
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

/// A state with a computed path and one plane `A1B2C3` in the cache at `t0`.
fn state(rec: &Rec) -> (AppState, Arc<PlaneInfoCache>) {
    let cache = Arc::new(PlaneInfoCache::new());
    cache.set(plane("A1B2C3", t0(), 51.3, 1.6));
    let mut st = AppState::new(settings(), None, Box::new(rec.clone()), cache.clone());
    st.play().unwrap();
    st.paths = vec![airscout::app::PathEntry {
        index: 0,
        call: "DX".into(),
        path: path(),
        terrain: Vec::new(),
        my_gain_dbi: 10.0,
        dx_gain_dbi: 12.0,
    }];
    (st, cache)
}

#[test]
fn click_while_playing_sets_track_and_deselect_none() {
    let rec = Rec::default();
    let (mut st, cache) = state(&rec);
    st.select_plane("A1B2C3");
    assert_eq!(st.selected.as_deref(), Some("A1B2C3"));
    assert_eq!(st.track_mode, TrackMode::Track);
    // C#: with tracking activated the click sets TRACK even when it deselects; the next tick
    // (no selected plane) resets it
    st.select_plane("A1B2C3");
    assert_eq!(st.selected, None);
    assert_eq!(st.track_mode, TrackMode::Track);
    st.on_tick(t0(), &cache);
    assert!(!st.tick.active.is_empty());
    assert_eq!(st.track_mode, TrackMode::None);

    // not activated: a click never tracks, a deselect resets at once
    st.settings.track.activate = false;
    st.select_plane("A1B2C3");
    assert_eq!(st.track_mode, TrackMode::None);
    st.track_mode = TrackMode::Track;
    st.select_plane("A1B2C3");
    assert_eq!(st.selected, None);
    assert_eq!(st.track_mode, TrackMode::None);

    // paused: the selection changes, tracking does not start
    st.settings.track.activate = true;
    st.pause();
    st.select_plane("A1B2C3");
    assert_eq!(st.selected.as_deref(), Some("A1B2C3"));
    assert_eq!(st.track_mode, TrackMode::None);
}

#[test]
fn pause_sets_none() {
    let rec = Rec::default();
    let (mut st, _) = state(&rec);
    st.select_plane("A1B2C3");
    assert_eq!(st.track_mode, TrackMode::Track);
    st.pause();
    assert_eq!(st.track_mode, TrackMode::None);
}

#[test]
fn right_click_dx_sets_single_with_qtf() {
    let rec = Rec::default();
    let (mut st, _) = state(&rec);
    st.settings.track.set_el = 33.0;
    st.right_click_dx();
    assert_eq!(st.track_mode, TrackMode::Single);
    assert_eq!(st.settings.track.set_az, bearing(50.5, 1.0, 51.5, 3.0));
    assert_eq!(st.settings.track.set_el, 0.0);
}

#[test]
fn no_selection_tick_resets_to_none() {
    let rec = Rec::default();
    let (mut st, cache) = state(&rec);
    st.right_click_dx();
    st.on_tick(t0(), &cache);
    assert!(st.tick.shown.iter().any(|p| p.hex == "A1B2C3"));
    // nothing selected: SINGLE lasts one tick
    assert_eq!(st.track_mode, TrackMode::None);
    // a selected plane keeps it
    st.selected = Some("A1B2C3".into());
    st.right_click_dx();
    st.on_tick(t0(), &cache);
    assert_eq!(st.track_mode, TrackMode::Single);
    // without any plane `DrawPlanes` returns early and leaves the mode
    st.selected = None;
    st.on_tick(t0() + Duration::minutes(60), &cache);
    assert!(st.tick.active.is_empty());
    assert_eq!(st.track_mode, TrackMode::Single);
}

#[test]
fn tracking_off_in_history() {
    let rec = Rec::default();
    let mut st = AppState::new(
        settings(),
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    );
    st.enter_history().unwrap();
    assert_eq!(st.life_mode, LifeMode::History);
    st.settings.track.activate = true;
    st.cache.set(plane("A1B2C3", t0(), 51.3, 1.6));
    st.select_plane("A1B2C3");
    assert_eq!(st.track_mode, TrackMode::None);
    st.track_mode = TrackMode::Track;
    tick(&mut st, &rec, t0());
    assert!(st.track.is_none());
    assert_eq!(rec.requests(), 0);
    // the worker gets the input, marked HISTORY, and steps nothing
    assert!(!rec.last_input().life);
    assert!(rec.0.borrow().shown.is_none());
}

#[test]
fn track_sync_waits_for_the_heights_from_the_service() {
    let rec = Rec::default();
    let (mut st, _) = state(&rec);
    st.select_plane("A1B2C3");
    // the first step asks the service (a worker) and computes nothing: no blocking lookup
    tick(&mut st, &rec, t0());
    assert_eq!(rec.requests(), 1);
    {
        let r = &rec.0.borrow().requests;
        assert_eq!(r[0].1, GPoint::new(50.5, 1.0));
        assert_eq!(r[0].2, GPoint::new(51.5, 3.0));
    }
    assert!(st.track.is_none());
    assert_eq!(st.track_status, TrackStatus::Stopped);
    // still waiting: no second request
    tick(&mut st, &rec, t0());
    assert_eq!(rec.requests(), 1);
    assert!(st.track.is_none());
    // an answer of another Play is ignored
    st.on_event(ServiceEvent::TrackHeights {
        id: 999,
        my_m: 1.0,
        dx_m: 1.0,
    });
    tick(&mut st, &rec, t0());
    assert!(st.track.is_none());
    rec.answer(&mut st);
    tick(&mut st, &rec, t0());
    assert!(st.track.is_some());
}

#[test]
fn track_sync_shows_each_worker_step() {
    let rec = Rec::default();
    let (mut st, cache) = state(&rec);
    st.settings.track.offset_s = 5;
    st.settings.doppler.strategy = DopplerStrategy::C;
    st.settings.doppler.dial_freq_hz = 144_174_000;
    st.select_plane("A1B2C3");
    tick(&mut st, &rec, t0());
    rec.answer(&mut st);
    tick(&mut st, &rec, t0());
    let v = st.track.clone().expect("values");
    assert_eq!(v.hex, "A1B2C3");
    assert_eq!(v.timestamp, t0() + Duration::seconds(5));
    assert_eq!(st.track_status, TrackStatus::Tracking);
    // heights: ground elevation (100 m here) plus the antenna height
    let p = cache.get("A1B2C3", v.timestamp, 15).unwrap();
    let r = st.settings.radius_km();
    let my_h = 100.0 + st.settings.my_height_m;
    assert_eq!(
        v.my_slant_range,
        slant_range_from_heights(my_h, v.my_distance, p.alt_m(), r)
    );
    // no Doppler without old values
    assert_eq!(
        (v.my_doppler, v.rx_hz, v.tx_hz),
        (0.0, 144_174_000, 144_174_000)
    );
    // frames between two worker steps change nothing
    st.track_sync();
    st.track_sync();
    assert_eq!(st.track.as_ref(), Some(&v));
    // the next step has old values of the same plane
    tick(&mut st, &rec, t0() + Duration::seconds(1));
    let w = st.track.clone().unwrap();
    assert_eq!(w.timestamp, v.timestamp + Duration::seconds(1));
    assert!(w.my_doppler != 0.0, "{w:?}");
    // the heights are asked for once per Play
    assert_eq!(rec.requests(), 1);
    // a new Play asks again and waits for the answer
    st.pause();
    st.play().unwrap();
    st.select_plane("A1B2C3");
    tick(&mut st, &rec, t0());
    assert_eq!(rec.requests(), 2);
    let before = st.track.clone();
    tick(&mut st, &rec, t0());
    assert_eq!(st.track, before);
}

/// MapDlg.cs:8064-8067: `oldvalues` is dropped as soon as a step runs for another plane, even
/// when that step has no values.
#[test]
fn old_values_dropped_by_an_invalid_step_of_another_plane() {
    let rec = Rec::default();
    let (mut st, _) = state(&rec);
    st.select_plane("A1B2C3");
    tick(&mut st, &rec, t0());
    rec.answer(&mut st);
    tick(&mut st, &rec, t0());
    tick(&mut st, &rec, t0() + Duration::seconds(1));
    assert!(st.track.as_ref().unwrap().my_doppler != 0.0);
    // another plane that is not in the cache: invalid step
    st.select_plane("ZZZZZZ");
    tick(&mut st, &rec, t0() + Duration::seconds(2));
    assert_eq!(st.track_status, TrackStatus::Stopped);
    // back to the first plane: no old values, no Doppler
    st.select_plane("ZZZZZZ");
    st.select_plane("A1B2C3");
    tick(&mut st, &rec, t0() + Duration::seconds(3));
    let v = st.track.clone().unwrap();
    assert_eq!(v.hex, "A1B2C3");
    assert_eq!((v.my_doppler, v.dx_doppler), (0.0, 0.0));
    // the step after that has old values again
    tick(&mut st, &rec, t0() + Duration::seconds(4));
    assert!(st.track.as_ref().unwrap().my_doppler != 0.0);
}

#[test]
fn track_sync_needs_activate_and_plane() {
    let rec = Rec::default();
    let (mut st, _) = state(&rec);
    st.select_plane("A1B2C3");
    st.settings.track.activate = false;
    tick(&mut st, &rec, t0());
    assert!(st.track.is_none());
    assert_eq!(rec.requests(), 0);
    // no params while `Track_Activate` is off: the worker steps nothing
    assert_eq!(rec.last_input().params, None);
    st.settings.track.activate = true;
    tick(&mut st, &rec, t0());
    rec.answer(&mut st);
    tick(&mut st, &rec, t0());
    let first = st.track.clone();
    assert!(first.is_some());
    st.track = None;
    // a selected plane that is not in the cache: no values, STOPPED
    st.selected = Some("ZZZZZZ".into());
    tick(&mut st, &rec, t0());
    assert!(st.track.is_none());
    assert_eq!(st.track_status, TrackStatus::Stopped);
    // SINGLE reports and then stops, as the C# does in one step
    st.right_click_dx();
    tick(&mut st, &rec, t0());
    let v = st.track.clone().unwrap();
    assert_eq!(v.my_azimuth, bearing(50.5, 1.0, 51.5, 3.0));
    assert_eq!(v.my_elevation, 0.0);
    assert_eq!(st.track_status, TrackStatus::Stopped);
}

#[test]
fn readout_renders_only_while_tracking() {
    use airscout::ui::Maps;
    let rec = Rec::default();
    let (mut st, _) = state(&rec);
    st.settings.doppler.strategy = DopplerStrategy::C;
    st.select_plane("A1B2C3");
    tick(&mut st, &rec, t0());
    rec.answer(&mut st);
    tick(&mut st, &rec, t0());
    assert!(airscout::ui::map_track::shown(&st).is_some());
    st.settings.gui.tracking_gauges_show = false;
    assert!(airscout::ui::map_track::shown(&st).is_none());
    st.settings.gui.tracking_gauges_show = true;
    st.track_mode = TrackMode::Single;
    assert!(airscout::ui::map_track::shown(&st).is_none());
    st.track_mode = TrackMode::Track;
    assert!(airscout::ui::map_track::shown(&st).is_some());
    st.pause();
    assert!(airscout::ui::map_track::shown(&st).is_none());
    st.play().unwrap();
    st.track_mode = TrackMode::Track;
    st.track_status = TrackStatus::Tracking;
    let mut h = egui_kittest::Harness::builder()
        .with_size(eframe::egui::vec2(900.0, 600.0))
        .build_ui_state(
            |ui, (st, maps): &mut (AppState, Maps)| airscout::ui::show(ui, st, maps),
            (st, Maps::without_tiles()),
        );
    h.run();
    h.run();
}
