//! Feed core: variables, wake categories, registry.
use as_feeds::vars::{format_double, substitute};
use as_feeds::{wake, FeedArgs, FeedError, FeedPlane};
use chrono::{TimeZone, Utc};
use std::path::PathBuf;

fn args() -> FeedArgs {
    FeedArgs {
        app_dir: PathBuf::from("/app"),
        data_dir: PathBuf::from("/data"),
        tmp_dir: PathBuf::from("/tmp/as"),
        log_dir: PathBuf::from("/log"),
        database_dir: PathBuf::from("/db"),
        min_lat: 47.5,
        max_lat: 54.25,
        min_lon: 4.0,
        max_lon: 16.5,
        min_alt_m: 5000,
        max_alt_m: 12200,
        my_lat: 50.937067,
        my_lon: 10.68327,
        dx_lat: 52.05626084,
        dx_lon: 1.28022909,
        keep_history: false,
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 12, 0, 0).unwrap()
}

#[test]
fn vars_substitute_all() {
    let a = args();
    assert_eq!(
        substitute(
            "https://api.adsb.one/v2/point/%MYLAT%/%MYLON%/%MAXDISTKM%",
            &a,
            now(),
            ""
        )
        .unwrap(),
        "https://api.adsb.one/v2/point/50.937067/10.68327/617.3928409746215"
    );
    assert_eq!(
        substitute(
            "http://localhost:7780/VirtualRadar/AircraftList.json?fSBnd=%MINLAT%&fNBnd=%MAXLAT%&fWBnd=%MINLON%&fEBnd=%MAXLON%",
            &a,
            now(),
            ""
        )
        .unwrap(),
        "http://localhost:7780/VirtualRadar/AircraftList.json?fSBnd=47.5&fNBnd=54.25&fWBnd=4&fEBnd=16.5"
    );
    assert_eq!(
        substitute(
            "%APPDIR%|%DATADIR%|%LOGDIR%|%DATABASEDIR%|%MINALTM%|%MAXALTM%|%MINALTFT%|%MAXALTFT%|%UNIXTIME%|%MINDISTKM%|%lastdv%",
            &a,
            now(),
            "638123"
        )
        .unwrap(),
        "/app|/data|/log|/db|5000|12200|16404|40026|1795435200|0|638123"
    );
    assert_eq!(substitute("no vars", &a, now(), "").unwrap(), "no vars");
}

#[test]
fn vars_errors_like_csharp() {
    let a = args();
    assert!(matches!(
        substitute("%MYLAT", &a, now(), ""),
        Err(FeedError::Config(_))
    ));
    assert!(matches!(
        substitute("%NOPE%", &a, now(), ""),
        Err(FeedError::Config(_))
    ));
}

#[test]
fn vars_invariant_decimal_point() {
    assert_eq!(format_double(0.1 + 0.2), "0.30000000000000004");
    assert_eq!(format_double(4.0), "4");
    assert_eq!(format_double(-12.5), "-12.5");
    assert_eq!(format_double(0.0), "0");
    assert_eq!(format_double(-0.0), "-0");
    assert_eq!(format_double(1e-5), "1E-05");
    assert_eq!(format_double(0.0001), "0.0001");
    assert_eq!(format_double(1e15), "1E+15");
    assert_eq!(format_double(123456789012345.0), "123456789012345");
    assert_eq!(format_double(f64::NAN), "NaN");
    assert_eq!(format_double(f64::INFINITY), "Infinity");
}

#[test]
fn wake_mapping() {
    for (e, t, c) in [
        ("A1", "C172", 1),
        ("A2", "", 1),
        ("A3", "A320", 2),
        ("A4", "B752", 2),
        ("A5", "B744", 3),
        ("A5", "A388", 4),
        ("", "A388", 4),
        ("A7", "", 0),
        ("a3", "", 0),
        ("", "", 0),
    ] {
        assert_eq!(wake::category(e, t), c, "{e} {t}");
    }
}

#[test]
fn feed_plane_defaults_mirror_csharp() {
    let p = FeedPlane::default();
    assert_eq!(p.time, None);
    assert_eq!(p.lat, f64::MIN);
    assert_eq!(p.alt_ft, f64::MIN);
    assert_eq!(p.speed_kts, f64::MIN);
    assert_eq!(p.category, 0);
    assert!(!p.ground);
    assert_eq!(p.hex, "");
}

#[test]
fn registry_lookup() {
    let all = as_feeds::registry::all_feeds();
    for f in &all {
        assert!(as_feeds::registry::feed_by_name(f.name()).is_some());
    }
    assert!(as_feeds::registry::feed_by_name("NoSuchFeed").is_none());
}
