use as_aircraft::position_files::{read_csv, read_hist, read_json, write_csv, write_json};
use as_aircraft::positions::AircraftPosition;
use chrono::{TimeZone, Utc};

fn row(hex: &str, secs: i64) -> AircraftPosition {
    AircraftPosition {
        hex: hex.into(),
        call: "TST123".into(),
        lat: 50.5,
        lon: 8.25,
        alt_ft: 31000.0,
        track: 90.5,
        speed_kts: 450.0,
        last_updated: Utc.timestamp_opt(secs, 0).unwrap(),
    }
}

fn no_db(_: &str) -> Option<String> {
    None
}

#[test]
fn json_roundtrip() {
    let rows = vec![row("AAAAAA", 1_700_000_000), row("BBBBBB", 1_700_000_060)];
    let mut buf = Vec::new();
    write_json(&mut buf, &rows).unwrap();
    let s = String::from_utf8(buf).unwrap();
    assert!(s.contains("\"LastUpdated\":\"2023-11-14T22:13:20Z\""));
    let st = read_json(&s).unwrap();
    assert_eq!(st.imported, rows);
    assert_eq!(st.skipped, 0);
}

#[test]
fn json_accepts_csharp_time_field() {
    let s = r#"[{"Hex":"abc123","Call":"X1","Lat":1.5,"Lon":2.5,"Alt":100,"Track":3,"Speed":4,"Time":"2023-11-14T22:13:20Z"}]"#;
    let st = read_json(s).unwrap();
    assert_eq!(st.imported.len(), 1);
    assert_eq!(st.imported[0].last_updated.timestamp(), 1_700_000_000);
    assert_eq!(st.imported[0].lat, 1.5);
}

#[test]
fn json_nested_objects_parsed() {
    let s = r#"[{"Hex":"A1","Lat":1,"Lon":2,"IntPoint":{"Lat":1,"Lon":2},"Time":"2023-11-14T22:13:20Z"},
        {"Lat":1,"Time":"2023-11-14T22:13:20Z"}, 5, {"Hex":"A2"}]"#;
    let st = read_json(s).unwrap();
    assert_eq!(st.imported.len(), 1);
    assert_eq!(st.skipped, 3);
}

#[test]
fn json_garbage_is_error_not_panic() {
    assert!(read_json("hello").is_err());
    assert!(read_json("{\"a\":1}").is_err());
}

#[test]
fn csv_roundtrip_and_header_exact() {
    let rows = vec![row("AAAAAA", 1_700_000_000)];
    let mut buf = Vec::new();
    write_csv(&mut buf, &rows).unwrap();
    let s = String::from_utf8(buf).unwrap();
    let mut lines = s.lines();
    assert_eq!(
        lines.next().unwrap(),
        "time[utc];hex;call;lat[deg];lon[deg];alt[ft];track[deg];speed[kts]"
    );
    assert_eq!(
        lines.next().unwrap(),
        "2023-11-14 22:13:20Z;AAAAAA;TST123;50.50000000;8.25000000;31000.00000000;90.50000000;450.00000000"
    );
    let st = read_csv(&s).unwrap();
    assert_eq!(st.imported, rows);
}

#[test]
fn csv_bad_lines_skipped_and_counted() {
    let s = "time[utc];hex;call;lat[deg];lon[deg]\n\
        no separator here\n\
        2023-11-14 22:13:20Z;aa;c;1;2\n\
        not a time;aa;c;1;2\n\
        2023-11-14 22:13:20Z;aa;c;x;2\n\
        2023-11-14 22:13:20Z;aa;c;NaN;2\n\
        2023-11-14 22:13:20Z;aa\n";
    let st = read_csv(s).unwrap();
    assert_eq!(st.imported.len(), 1);
    assert_eq!(st.imported[0].hex, "AA");
    assert_eq!(st.imported[0].call, "C");
    assert_eq!(st.skipped, 4);
    assert!(read_csv("time;hex;lat\n").is_err());
}

#[test]
fn csv_columns_in_any_order() {
    let s = "Speed[kts];LAT[deg];lon;hex;time;extra;call;alt;track\n\
        7;1.5;2.5;ab;2023-11-14T22:13:20Z;zzz;cs;300;45\n";
    let st = read_csv(s).unwrap();
    let p = &st.imported[0];
    assert_eq!(
        (p.speed_kts, p.lat, p.lon, p.alt_ft, p.track),
        (7.0, 1.5, 2.5, 300.0, 45.0)
    );
    assert_eq!((p.hex.as_str(), p.call.as_str()), ("AB", "CS"));
    let s = "time;hex;lat;lon\n2023-11-14 22:13:20;aa;1;2\n";
    assert_eq!(
        read_csv(s).unwrap().imported[0].last_updated.timestamp(),
        1_700_000_000
    );
}

#[test]
fn bom_is_ignored() {
    let csv = "\u{feff}time;hex;lat;lon\n2023-11-14 22:13:20Z;aa;1;2\n";
    assert_eq!(read_csv(csv).unwrap().imported.len(), 1);
    let json = "\u{feff} [{\"Hex\":\"A\",\"Lat\":1,\"Lon\":2,\"Time\":\"2023-11-14T22:13:20Z\"}]";
    assert_eq!(read_json(json).unwrap().imported.len(), 1);
    let hist = "\u{feff}{}";
    assert_eq!(read_hist(hist, &no_db).unwrap().imported.len(), 0);
}

#[test]
fn empty_file_imports_nothing() {
    for st in [
        read_csv("").unwrap(),
        read_json("").unwrap(),
        read_json("  \n").unwrap(),
        read_json("[]").unwrap(),
        read_hist("", &no_db).unwrap(),
        read_hist("{}", &no_db).unwrap(),
    ] {
        assert!(st.imported.is_empty());
        assert_eq!(st.skipped, 0);
    }
}

#[test]
fn hist_maps_fields_and_uses_db_hex() {
    let s = r#"{
        "k1": {"id":"2b1f","latitude":50.0,"longitude":8.0,"track":10,"altitude":20000,"speed":300,
               "timestamp":1700000000,"callsign":"TST1","squawk":null},
        "k2": {"id":"2b20","latitude":51.0,"longitude":9.0,"timestamp":1700000060,"callsign":""},
        "k3": {"id":"bad","timestamp":"oops"},
        "k4": 7
    }"#;
    let find = |c: &str| (c == "TST1").then(|| "ABCDEF".to_string());
    let st = read_hist(s, &find).unwrap();
    assert_eq!(st.skipped, 2);
    let a = st.imported.iter().find(|p| p.call == "TST1").unwrap();
    assert_eq!(a.hex, "ABCDEF");
    assert_eq!(
        (a.lat, a.lon, a.track, a.alt_ft, a.speed_kts),
        (50.0, 8.0, 10.0, 20000.0, 300.0)
    );
    assert_eq!(a.last_updated.timestamp(), 1_700_000_000);
    let b = st.imported.iter().find(|p| p.call == "[unknown]").unwrap();
    assert_eq!(b.hex, "2b20");
    assert!(read_hist("[1]", &no_db).is_err());
}

#[test]
fn json_missing_lat_lon_skipped_not_zero() {
    let s = r#"[{"Hex":"A","Lon":2,"Time":"2023-11-14T22:13:20Z"},
        {"Hex":"B","Lat":null,"Lon":2,"Time":"2023-11-14T22:13:20Z"},
        {"Hex":"C","Lat":"x","Lon":2,"Time":"2023-11-14T22:13:20Z"},
        {"Hex":"D","Lat":1,"Lon":2,"Time":"2023-11-14T22:13:20Z"}]"#;
    let st = read_json(s).unwrap();
    assert_eq!((st.imported.len(), st.skipped), (1, 3));
    assert_eq!(st.imported[0].hex, "D");
}

#[test]
fn bad_record_does_not_fail_file() {
    let t = "\"Time\":\"2023-11-14T22:13:20Z\"";
    let s = format!(
        "[{{\"Hex\":\"A\",\"Lat\":1,\"Lon\":2,{t}}}, {{\"Hex\":\"B\",\"Lat\":1e999,\"Lon\":2,{t}}},
         {{\"Hex\": }}, {{\"Hex\":\"C,]}}\",\"Lat\":1,\"Lon\":2,{t}}}, {{\"Hex\":\"D\",\"Lat\":1,\"Lon\":2,{t}}}]"
    );
    let st = read_json(&s).unwrap();
    let hexes: Vec<_> = st.imported.iter().map(|p| p.hex.as_str()).collect();
    assert_eq!(hexes, ["A", "C,]}", "D"]);
    assert_eq!(st.skipped, 2);
    // truncated file keeps what came before
    let st = read_json(
        "[{\"Hex\":\"A\",\"Lat\":1,\"Lon\":2,\"Time\":\"2023-11-14T22:13:20Z\"},{\"Hex\":",
    )
    .unwrap();
    assert_eq!((st.imported.len(), st.skipped), (1, 1));

    let h = r#"{"k1":{"id":"a","latitude":1,"longitude":2,"timestamp":1700000000,"callsign":"X"},
        "k2":{"id":"b","latitude":1e999,"timestamp":1700000000},
        "k3":{"id":"c","latitude":1,"longitude":2,"timestamp":1700000000,"callsign":"Y"}}"#;
    let st = read_hist(h, &no_db).unwrap();
    assert_eq!((st.imported.len(), st.skipped), (2, 1));
}
