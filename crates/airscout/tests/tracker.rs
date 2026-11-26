//! `Tracker`: the tracking step over a `TrackInput` snapshot (`MapDlg.cs:8054-8155`).
use airscout::tracker::{TrackInput, TrackParams, Tracker};
use airscout::tracking::TrackStatus;
use as_aircraft::enums::TrackMode;
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_settings::{DopplerStrategy, Settings};
use chrono::{DateTime, Duration, TimeZone, Utc};

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

const DIAL: i64 = 144_300_000;

fn params() -> TrackParams {
    let mut s = settings();
    s.doppler.strategy = DopplerStrategy::A;
    s.doppler.dial_freq_hz = DIAL;
    TrackParams::from_settings(&s)
}

fn input(mode: TrackMode, hex: Option<&str>) -> TrackInput {
    TrackInput {
        rev: 7,
        life: true,
        mode,
        hex: hex.map(str::to_owned),
        heights: Some((0.0, 0.0)),
        params: Some(params()),
    }
}

fn cache() -> PlaneInfoCache {
    let c = PlaneInfoCache::new();
    c.set(plane("A1B2C3", t0(), 51.3, 1.6));
    c
}

#[test]
fn no_step_outside_life_or_without_params() {
    let (mut t, c) = (Tracker::default(), cache());
    let mut i = input(TrackMode::Track, Some("A1B2C3"));
    i.life = false;
    assert!(t.step(&i, t0(), &c).is_none());
    let mut i = input(TrackMode::Track, Some("A1B2C3"));
    i.params = None;
    assert!(t.step(&i, t0(), &c).is_none());
    assert!(t
        .step(&input(TrackMode::Track, Some("A1B2C3")), t0(), &c)
        .is_some());
}

#[test]
fn track_without_heights_is_stopped() {
    let (mut t, c) = (Tracker::default(), cache());
    let mut i = input(TrackMode::Track, Some("A1B2C3"));
    i.heights = None;
    let (step, shown) = t.step(&i, t0(), &c).unwrap();
    assert_eq!(step.values, None);
    assert_eq!(step.mode, TrackMode::Track);
    assert_eq!(shown.values, None);
    assert_eq!(shown.status, TrackStatus::Stopped);
}

#[test]
fn track_with_plane_tracks_and_sets_rx_tx() {
    let (mut t, c) = (Tracker::default(), cache());
    let i = input(TrackMode::Track, Some("A1B2C3"));
    let (_, first) = t.step(&i, t0(), &c).unwrap();
    assert_eq!(first.status, TrackStatus::Tracking);
    let v = first.values.unwrap();
    // no Doppler without old values
    assert_eq!((v.rx_hz, v.tx_hz), (DIAL, DIAL));
    let (step, second) = t.step(&i, t0() + Duration::seconds(1), &c).unwrap();
    assert_eq!(second.status, TrackStatus::Tracking);
    assert_eq!(step.mode, TrackMode::Track);
    let v = second.values.unwrap();
    assert_eq!(v.hex, "A1B2C3");
    assert_ne!(v.rx_hz, DIAL, "{v:?}");
    assert_eq!(v.tx_hz, DIAL);
    assert_eq!(step.values, Some(v));
}

#[test]
fn single_uses_set_az_and_is_stopped() {
    let (mut t, c) = (Tracker::default(), cache());
    let mut i = input(TrackMode::Single, None);
    i.heights = None;
    let p = i.params.as_mut().unwrap();
    (p.set_az, p.set_el) = (123.5, 12.25);
    let (step, shown) = t.step(&i, t0(), &c).unwrap();
    let v = shown.values.unwrap();
    assert_eq!((v.my_azimuth, v.my_elevation), (123.5, 12.25));
    assert_eq!(shown.status, TrackStatus::Stopped);
    assert_eq!(step.mode, TrackMode::Single);
    // NaN fields (the DX side is not set in SINGLE) rule out comparing whole values
    let sv = step.values.unwrap();
    assert_eq!((sv.my_azimuth, sv.my_elevation), (123.5, 12.25));
}

/// MapDlg.cs:8064-8067: `oldvalues` is dropped as soon as a step runs for another plane, even
/// when that step has no values.
#[test]
fn old_dropped_for_another_plane() {
    let (mut t, c) = (Tracker::default(), cache());
    let a = input(TrackMode::Track, Some("A1B2C3"));
    let doppler = |t: &mut Tracker, i: &TrackInput, s: i64| {
        let (_, shown) = t.step(i, t0() + Duration::seconds(s), &c).unwrap();
        shown.values.map(|v| v.my_doppler)
    };
    assert_eq!(doppler(&mut t, &a, 0), Some(0.0));
    assert!(doppler(&mut t, &a, 1).unwrap() != 0.0);
    // another plane that is not in the cache: invalid step
    let z = input(TrackMode::Track, Some("ZZZZZZ"));
    let (step, shown) = t.step(&z, t0() + Duration::seconds(2), &c).unwrap();
    assert_eq!((step.values, shown.status), (None, TrackStatus::Stopped));
    // back to the first plane: no old values, no Doppler
    assert_eq!(doppler(&mut t, &a, 3), Some(0.0));
    assert!(doppler(&mut t, &a, 4).unwrap() != 0.0);
}

#[test]
fn shown_carries_input_rev() {
    let (mut t, c) = (Tracker::default(), cache());
    for (rev, mode) in [(7, TrackMode::Track), (9, TrackMode::None)] {
        let mut i = input(mode, Some("A1B2C3"));
        i.rev = rev;
        assert_eq!(t.step(&i, t0(), &c).unwrap().1.rev, rev);
    }
}
