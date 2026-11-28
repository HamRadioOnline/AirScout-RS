use airscout::style::*;
use as_aircraft::{AltitudeProvenance, PlaneCategory, PlaneInfo};
use as_geo::latlon::GPoint;
use as_settings::InfoWin;
use chrono::{TimeZone, Utc};

fn my() -> GPoint {
    GPoint {
        lat: 48.0,
        lon: 11.0,
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 10, 50, 0).unwrap()
}

fn plane() -> PlaneInfo {
    PlaneInfo {
        call: "DLH123".into(),
        lat: 48.456,
        lon: -11.234,
        alt_ft: 30000.0,
        track: 123.7,
        speed_kts: 400.0,
        manufacturer: "AIRBUS".into(),
        model: "A320".into(),
        category: PlaneCategory::Medium,
        int_qrb: 120.0,
        forward_qrb: Some(120.0),
        alt_diff: 150.4,
        potential: 100,
        eps1: 1.5_f64.to_radians(),
        eps2: 2.25_f64.to_radians(),
        squint: 3.0_f64.to_radians(),
        angle: 45.0_f64.to_radians(),
        ..PlaneInfo::default()
    }
}

#[test]
fn color_by_potential() {
    assert_eq!(plane_color(100), PlaneColor::Magenta);
    assert_eq!(plane_color(75), PlaneColor::Red);
    assert_eq!(plane_color(50), PlaneColor::DarkOrange);
    assert_eq!(plane_color(0), PlaneColor::Gray);
    assert_eq!(plane_color(60), PlaneColor::Gray);
    assert_eq!(rgb(PlaneColor::Magenta), [255, 0, 255]);
    assert_eq!(rgb(PlaneColor::Red), [255, 0, 0]);
    assert_eq!(rgb(PlaneColor::DarkOrange), [255, 140, 0]);
    assert_eq!(rgb(PlaneColor::Gray), [128, 128, 128]);
}

#[test]
fn icon_size_by_category() {
    // sizes: [L, M, H, S] order as in GuiSettings::planes_icon_size
    let sizes = [10, 20, 30, 40];
    assert_eq!(plane_icon_px(PlaneCategory::Light, sizes), 10);
    assert_eq!(plane_icon_px(PlaneCategory::Medium, sizes), 20);
    assert_eq!(plane_icon_px(PlaneCategory::Heavy, sizes), 30);
    assert_eq!(plane_icon_px(PlaneCategory::Superheavy, sizes), 40);
    assert_eq!(plane_icon_px(PlaneCategory::None, sizes), 20);
}

#[test]
fn tooltip_metric_default() {
    // minutes = (int)(120 / 740.8 * 60) = 9 -> 10:59
    let lines = tooltip_lines(&plane(), &InfoWin::default(), my(), now(), None);
    assert_eq!(
        lines,
        vec![
            "DLH123",
            "--------------------",
            "Pos: 48.46°N , 011.23°W",
            "Alt: 9143m [estimated] [+150m]",
            "Track: 123°",
            "Type: AIRBUS A320 [M]",
            "Dist: 120km",
            "Time: 10:59 [ 9min]",
            "Squint: 03.00°",
        ]
    );
}

#[test]
fn tooltip_imperial() {
    let info = InfoWin {
        metric: false,
        speed: true,
        angle: true,
        epsilon: true,
        ..InfoWin::default()
    };
    let mut p = plane();
    p.alt_diff = -100.0;
    p.lat = -3.0;
    p.lon = 7.5;
    let lines = tooltip_lines(&p, &info, my(), now(), None);
    assert_eq!(
        lines,
        vec![
            "DLH123",
            "--------------------",
            "Pos: 03.00°S , 007.50°E",
            "Alt: 30000ft [estimated] [-328ft]",
            "Track: 123°",
            "Speed: 400kts",
            "Type: AIRBUS A320 [M]",
            "Dist: 75mi",
            "Time: 10:59 [ 9min]",
            "Angle: 45°",
            "Eps: 01.50° <> 02.25°",
            "Squint: 03.00°",
        ]
    );
}

#[test]
fn tooltip_marks_pressure_altitude_as_estimated() {
    let mut p = plane();
    p.altitude_provenance = AltitudeProvenance::PressureEstimate;
    let lines = tooltip_lines(&p, &InfoWin::default(), my(), now(), None);
    assert!(lines
        .iter()
        .any(|line| line == "Alt: 9143m [estimated] [+150m]"));
}

#[test]
fn tooltip_simple_for_gray_planes() {
    // CreatePlaneSimple: no alt diff, no speed, no crossing lines, even with Speed/Dist enabled
    let info = InfoWin {
        speed: true,
        ..InfoWin::default()
    };
    let expected_metric = vec![
        "DLH123",
        "--------------------",
        "Pos: 48.46°N , 011.23°W",
        "Alt: 9143m [estimated]",
        "Track: 123°",
        "Type: AIRBUS A320 [M]",
    ];
    for potential in [0, 60] {
        let mut p = plane();
        p.potential = potential;
        assert_eq!(tooltip_lines(&p, &info, my(), now(), None), expected_metric);
    }
    let mut p = plane();
    p.potential = 0;
    let imperial = InfoWin {
        metric: false,
        ..info
    };
    assert_eq!(
        tooltip_lines(&p, &imperial, my(), now(), None),
        vec![
            "DLH123",
            "--------------------",
            "Pos: 48.46°N , 011.23°W",
            "Alt: 30000ft [estimated]",
            "Track: 123°",
            "Type: AIRBUS A320 [M]",
        ]
    );
}

#[test]
fn tooltip_time_is_na_without_forward_crossing() {
    let mut p = plane();
    p.forward_qrb = None;
    let lines = tooltip_lines(&p, &InfoWin::default(), my(), now(), None);
    assert!(lines.contains(&"Time: n/a".to_string()));
}

#[test]
fn tooltip_truncates_type_and_zero_altdiff() {
    let mut p = plane();
    p.manufacturer = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".into();
    p.model = "0123456789012345678901234".into();
    p.alt_diff = 0.4;
    p.potential = 75;
    let lines = tooltip_lines(&p, &InfoWin::default(), my(), now(), None);
    assert!(lines.contains(&"Type: ABCDEFGHIJKLMNOPQRST 01234567890123456789 [M]".to_string()));
    assert!(lines.contains(&"Alt: 9143m [estimated] [0m]".to_string()));
}

fn track_values() -> airscout::tracking::TrackValues {
    let mut v = airscout::tracking::TrackValues::new(now());
    v.my_azimuth = 5.5;
    v.my_elevation = -0.5;
    v.my_doppler = 1234.5;
    v.dx_azimuth = 123.456;
    v.dx_elevation = 12.0;
    v.dx_doppler = -77.4;
    v
}

#[test]
fn tooltip_tracking_lines_gated_by_infowin() {
    let v = track_values();
    let p = plane();
    // all off by default: nothing, not even a separator
    assert_eq!(
        tooltip_lines(&p, &InfoWin::default(), my(), now(), Some(&v)),
        tooltip_lines(&p, &InfoWin::default(), my(), now(), None)
    );
    let base = tooltip_lines(&p, &InfoWin::default(), my(), now(), None);
    let with = |info: InfoWin| {
        let l = tooltip_lines(&p, &info, my(), now(), Some(&v));
        assert_eq!(l[..base.len()], base[..]);
        l[base.len()..].to_vec()
    };
    let all = with(InfoWin {
        my_azimuth: true,
        my_elevation: true,
        my_doppler: true,
        dx_azimuth: true,
        dx_elevation: true,
        dx_doppler: true,
        ..InfoWin::default()
    });
    assert_eq!(
        all,
        vec![
            "--------------------",
            "MyAzimuth: 05.50°",
            "MyElevation: -00.50°",
            "MyDoppler: 1235Hz",
            "--------------------",
            "DXAzimuth: 123.46°",
            "DXElevation: 12.00°",
            "DXDoppler: -77Hz",
        ]
    );
    // one flag: its block with only its line
    assert_eq!(
        with(InfoWin {
            my_doppler: true,
            ..InfoWin::default()
        }),
        vec!["--------------------", "MyDoppler: 1235Hz"]
    );
    assert_eq!(
        with(InfoWin {
            dx_elevation: true,
            ..InfoWin::default()
        }),
        vec!["--------------------", "DXElevation: 12.00°"]
    );
}

#[test]
fn simple_tooltip_has_no_tracking_lines() {
    let v = track_values();
    let info = InfoWin {
        my_azimuth: true,
        dx_doppler: true,
        ..InfoWin::default()
    };
    for potential in [0, 60] {
        let mut p = plane();
        p.potential = potential;
        assert_eq!(
            tooltip_lines(&p, &info, my(), now(), Some(&v)),
            tooltip_lines(&p, &info, my(), now(), None)
        );
    }
}
