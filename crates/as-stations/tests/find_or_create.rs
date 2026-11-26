use as_geo::maidenhead::{gpoint_from_loc, PositionInRectangle};
use as_stations::{GeoSource, StationDb};

#[test]
fn location_find_or_create_creates_centre_once() {
    let db = StationDb::open_in_memory().unwrap();
    let centre = gpoint_from_loc("JO50AA", PositionInRectangle::MiddleMiddle).unwrap();
    let a = db
        .location_find_or_create("TE1ST", "JO50AA")
        .unwrap()
        .expect("created");
    assert_eq!(a.call, "TE1ST");
    assert_eq!(a.loc, "JO50AA");
    assert_eq!((a.lat, a.lon), (centre.lat, centre.lon));
    assert_eq!(a.source, GeoSource::FromLoc);
    assert_eq!(db.location_count().unwrap(), 1);
    let b = db
        .location_find_or_create("TE1ST", "JO50AA")
        .unwrap()
        .expect("found");
    assert_eq!(b, a);
    assert_eq!(db.location_count().unwrap(), 1);
    assert!(db.location_find_or_create("", "JO50AA").unwrap().is_none());
    assert!(db.location_find_or_create("TE1ST", "").unwrap().is_none());
    assert_eq!(db.location_count().unwrap(), 1);
}
