use as_geo::band::Band;
use as_geo::latlon::GRect;
use as_stations::{GeoSource, Location, Qrv, StationDb};
use chrono::{TimeZone, Utc};

fn loc(call: &str, l: &str, lat: f64, lon: f64) -> Location {
    let t = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    Location::new(call, l, lat, lon, GeoSource::FromUser, 1, t)
}

#[test]
fn location_get_all_in_rect() {
    let db = StationDb::open_in_memory().unwrap();
    for l in [
        loc("DL3", "JO50WC", 50.0, 11.0),
        loc("DL1", "JO40AA", 40.0, 10.0),
        loc("DL2", "JO60AA", 60.0, 20.0),
    ] {
        db.location_insert_or_update_if_newer(&l).unwrap();
    }
    let all = db.location_get_all(None).unwrap();
    let calls: Vec<_> = all.iter().map(|l| l.call.as_str()).collect();
    assert_eq!(calls, ["DL1", "DL2", "DL3"]);
    // inclusive bounds: DL1 sits exactly on min corner, DL3 inside, DL2 outside
    let r = db
        .location_get_all(Some(GRect::new(40.0, 10.0, 55.0, 15.0)))
        .unwrap();
    let calls: Vec<_> = r.iter().map(|l| l.call.as_str()).collect();
    assert_eq!(calls, ["DL1", "DL3"]);
    assert!(db
        .location_get_all(Some(GRect::new(0.0, 0.0, 1.0, 1.0)))
        .unwrap()
        .is_empty());
}

#[test]
fn location_and_qrv_delete() {
    let db = StationDb::open_in_memory().unwrap();
    db.location_insert_or_update_if_newer(&loc("DL1", "JO40AA", 40.0, 10.0))
        .unwrap();
    db.location_insert_or_update_if_newer(&loc("DL1", "JO41AA", 41.0, 10.0))
        .unwrap();
    assert!(db.location_delete("dl1", "JO40AA").unwrap());
    assert!(!db.location_delete("DL1", "JO40AA").unwrap());
    assert_eq!(db.location_count().unwrap(), 1);
    // empty loc: all locations of the call
    assert!(db.location_delete("DL1", "").unwrap());
    assert_eq!(db.location_count().unwrap(), 0);

    let t = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    for b in [Band::B144M, Band::B10G] {
        db.qrv_insert_or_update_if_newer(
            &Qrv::new("DL1", "JO40AA", b, 10.0, 10.0, 10.0, t).unwrap(),
        )
        .unwrap();
    }
    assert!(db.qrv_delete("DL1", "JO40AA", Band::B144M).unwrap());
    assert!(!db.qrv_delete("DL1", "JO40AA", Band::B144M).unwrap());
    assert!(db.qrv_find("DL1", "JO40AA", Band::B10G).unwrap().is_some());
    assert!(db.qrv_delete("DL1", "JO40AA", Band::All).unwrap());
    assert_eq!(db.qrv_count().unwrap(), 0);
}
