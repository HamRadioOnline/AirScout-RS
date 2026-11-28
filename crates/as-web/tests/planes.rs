//! The shared `planes.json` writer: `"version":1`, and the AirScoutServer feed reads it.

use as_aircraft::PlaneInfo;
use as_feeds::feeds::airscoutserver::parse;
use as_web::planes::{planes_json, PLANES_JSON_VERSION};
use chrono::Utc;

fn plane(hex: &str, call: &str, lat: f64, lon: f64) -> PlaneInfo {
    PlaneInfo {
        hex: hex.into(),
        call: call.into(),
        lat,
        lon,
        alt_ft: 35000.0,
        speed_kts: 451.0,
        track: 90.0,
        type_code: "A320".into(),
        time: Utc::now(),
        ..PlaneInfo::default()
    }
}

#[test]
fn version_is_one() {
    assert_eq!(PLANES_JSON_VERSION, 1);
    let text = planes_json(&[], PLANES_JSON_VERSION);
    assert!(
        text.starts_with("{\"full_count\":0,\"version\":1"),
        "{text}"
    );
}

#[test]
fn airscoutserver_reads_it() {
    let planes = [
        plane("3C0001", "TE1ST", 50.1, 10.2),
        plane("3C0002", "TE2ST", 50.3, 10.4),
    ];
    let text = planes_json(&planes, PLANES_JSON_VERSION);
    let read = parse(&text, Utc::now()).expect("feed parses the writer's text");
    assert_eq!(read.len(), 2, "{text}");
    for (want, got) in planes.iter().zip(&read) {
        assert_eq!(got.hex, want.hex);
        assert_eq!(got.call, want.call);
        assert_eq!(got.type_code, want.type_code);
        assert!((got.lat - want.lat).abs() < 1e-4);
        assert!((got.lon - want.lon).abs() < 1e-4);
    }
}

#[test]
fn layout_unchanged() {
    let mut p = plane("3C0001", "TE1ST", 50.1235, 8.5);
    p.reg = "TE-ST1".into();
    p.time = chrono::DateTime::from_timestamp(1_795_521_600, 0).unwrap();
    assert_eq!(
        planes_json(&[p], PLANES_JSON_VERSION),
        "{\"full_count\":1,\"version\":1,\"00000000\":[\"3C0001\",50.1235,8.5000,90,35000,451,\
         \"\",\"\",\"A320\",\"TE-ST1\",1795521600,\"\",\"\",\"\",0,0,\"TE1ST\",0]\n}\n"
    );
}
