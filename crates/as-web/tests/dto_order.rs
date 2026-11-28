//! Member order of the DTOs the harness cannot build (their C# classes need SQLite).
//! The rule: public fields first, then properties, then `SQLiteEntry.LastUpdated`.
use as_web::dto::{AirportJson, ElevationPathJson, LocationJson, PropagationPathJson, QrvJson};
use as_web::json::{to_compact, Num};
use chrono::{DateTime, TimeZone, Utc};

fn t(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, mi, s).unwrap()
}

/// Member names of a flat compact object (no nested objects), in order.
fn keys(json: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    let obj = v.as_object().unwrap();
    // serde_json::Map is sorted without `preserve_order`; read the order from the text.
    let mut out: Vec<String> = Vec::new();
    for part in json.split("\":").take(obj.len()) {
        let key = &part[part.rfind('"').unwrap() + 1..];
        assert!(obj.contains_key(key), "{key} in {json}");
        out.push(key.to_string());
    }
    out
}

fn location() -> LocationJson {
    LocationJson {
        elevation: 0,
        best_case_elevation: false,
        call: "TE1ST".into(),
        loc: "JO50AA".into(),
        lat: Num(50.0208),
        lon: Num(10.0417),
        source: 1,
        hits: 0,
        last_updated: t(2013, 5, 12, 19, 18, 12),
    }
}

fn qrv() -> QrvJson {
    QrvJson {
        call: "TE1ST".into(),
        loc: "JO50AA".into(),
        band: 144,
        antenna_height: Num(10.0),
        antenna_gain: Num(12.0),
        power: Num(100.0),
        last_updated: t(2010, 9, 4, 0, 0, 0),
    }
}

/// The shape of a real C# `LocationDesignator` (`crates/as-stations/tests/updater.rs:19`).
#[test]
fn location_as_csharp() {
    assert_eq!(
        to_compact(&location()),
        r#"{"Elevation":0,"BestCaseElevation":false,"Call":"TE1ST","Loc":"JO50AA","Lat":50.0208,"Lon":10.0417,"Source":1,"Hits":0,"LastUpdated":"2013-05-12T19:18:12Z"}"#
    );
}

/// The shape of a real C# `QRVDesignator` (`crates/as-stations/tests/updater.rs:23`).
#[test]
fn qrv_as_csharp() {
    assert_eq!(
        to_compact(&qrv()),
        r#"{"Call":"TE1ST","Loc":"JO50AA","Band":144,"AntennaHeight":10.0,"AntennaGain":12.0,"Power":100.0,"LastUpdated":"2010-09-04T00:00:00Z"}"#
    );
}

#[test]
fn elevation_path_order() {
    let mut p = ElevationPathJson {
        valid: true,
        selected: false,
        location1: None,
        location2: None,
        qrv1: None,
        qrv2: None,
        bearing12: Num(1.0),
        bearing21: Num(2.0),
        distance: Num(3.0),
        lat1: Num(4.0),
        lon1: Num(5.0),
        lat2: Num(6.0),
        lon2: Num(7.0),
        step_width: Num(8.0),
        count: 2,
        path: vec![-1, 300],
        last_updated: t(2026, 11, 24, 12, 0, 0),
    };
    let json = to_compact(&p);
    assert!(json.contains(r#""Path":[-1,300]"#));
    assert_eq!(
        keys(&json),
        [
            "Valid",
            "Selected",
            "Location1",
            "Location2",
            "QRV1",
            "QRV2",
            "Bearing12",
            "Bearing21",
            "Distance",
            "Lat1",
            "Lon1",
            "Lat2",
            "Lon2",
            "StepWidth",
            "Count",
            "Path",
            "LastUpdated"
        ]
    );
    p.location1 = Some(location());
    p.qrv2 = Some(qrv());
    let json = to_compact(&p);
    assert!(json.contains(&format!(
        r#""Location1":{},"Location2":null"#,
        to_compact(&location())
    )));
    assert!(json.contains(&format!(r#""QRV2":{},"Bearing12""#, to_compact(&qrv()))));
}

#[test]
fn propagation_path_order() {
    let p = PropagationPathJson {
        valid: false,
        selected: false,
        local_obstructed: true,
        local_obstruction: Num(0.5),
        location1: None,
        location2: None,
        qrv1: None,
        qrv2: None,
        bearing12: Num(1.0),
        bearing21: Num(2.0),
        distance: Num(3.0),
        lat1: Num(4.0),
        lon1: Num(5.0),
        h1: Num(6.0),
        lat2: Num(7.0),
        lon2: Num(8.0),
        h2: Num(9.0),
        qrg: Num(0.144),
        radius: Num(8473.43),
        f1_clearance: Num(0.6),
        step_width: Num(90.0),
        eps1_min: Num(f64::NAN),
        eps2_min: Num(-0.25),
        last_updated: t(2026, 11, 24, 12, 0, 0),
    };
    let json = to_compact(&p);
    assert!(json.contains(r#""Eps1_Min":"NaN""#));
    assert_eq!(
        keys(&json),
        [
            "Valid",
            "Selected",
            "LocalObstructed",
            "LocalObstruction",
            "Location1",
            "Location2",
            "QRV1",
            "QRV2",
            "Bearing12",
            "Bearing21",
            "Distance",
            "Lat1",
            "Lon1",
            "h1",
            "Lat2",
            "Lon2",
            "h2",
            "QRG",
            "Radius",
            "F1_Clearance",
            "StepWidth",
            "Eps1_Min",
            "Eps2_Min",
            "LastUpdated"
        ]
    );
}

#[test]
fn airport_order() {
    let a = AirportJson {
        icao: "XXAA".into(),
        iata: "XAA".into(),
        lat: Num(50.0),
        lon: Num(10.0),
        alt: Num(120.0),
        airport: "Synthetic Field".into(),
        country: "Nowhere".into(),
        last_updated: t(2026, 11, 24, 12, 0, 0),
    };
    assert_eq!(
        to_compact(&a),
        r#"{"ICAO":"XXAA","IATA":"XAA","Lat":50.0,"Lon":10.0,"Alt":120.0,"Airport":"Synthetic Field","Country":"Nowhere","LastUpdated":"2026-11-24T12:00:00Z"}"#
    );
}
