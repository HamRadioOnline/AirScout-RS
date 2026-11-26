//! The export of the crossing history (Task 10): ambiguity pass, CSV, file name.
use airscout::crossing_csv::{
    crossing_file_name, mark_ambiguous, write_crossing_csv, CROSSING_HEADER,
};
mod common;
use chrono::{TimeZone, Utc};
use common::*;

#[test]
fn mark_ambiguous_quirks() {
    // 1. index 0 is never reset
    let mut c = vec![crossing("aaaaaa", 0), crossing("aaaaaa", 100)];
    c[0].ambiguous = true;
    c[1].ambiguous = true;
    mark_ambiguous(&mut c, 10.0);
    assert!(c[0].ambiguous, "index 0 keeps its value");
    // the last item is always ambiguous
    assert!(c[1].ambiguous);

    // 2. the hex at the gap joins the closed group
    let mut c = vec![
        crossing("aaaaaa", 0),
        crossing("aaaaaa", 1),
        crossing("bbbbbb", 100),
        crossing("bbbbbb", 101),
    ];
    mark_ambiguous(&mut c, 10.0);
    assert_eq!(flags(&c), [true, true, false, true]);

    // 3. the hex of index 0 never counts for the first group
    let mut c = vec![
        crossing("aaaaaa", 0),
        crossing("bbbbbb", 1),
        crossing("bbbbbb", 2),
    ];
    mark_ambiguous(&mut c, 10.0);
    assert_eq!(flags(&c), [false, false, true]);

    // a gap is strictly greater than the limit: 10 s is none, the groups run together
    let mut c = vec![
        crossing("aaaaaa", 0),
        crossing("aaaaaa", 5),
        crossing("bbbbbb", 15),
        crossing("bbbbbb", 16),
    ];
    mark_ambiguous(&mut c, 10.0);
    assert_eq!(flags(&c), [true, true, true, true]);
    mark_ambiguous(&mut c, 9.0);
    assert_eq!(flags(&c), [true, true, false, true]);

    // one item, none
    let mut one = vec![crossing("aaaaaa", 0)];
    mark_ambiguous(&mut one, 10.0);
    assert_eq!(flags(&one), [true]);
    mark_ambiguous(&mut [], 10.0);
}

#[test]
fn csv_signal_is_written_with_two_decimals() {
    let mut c = vec![crossing("abc123", 0)];
    c[0].signal_strength = -93.456;
    let mut out = Vec::new();
    write_crossing_csv(&mut out, &c, &path()).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(
        text.lines().nth(1).unwrap().split(';').nth(19),
        Some("-93.46")
    );
}

#[test]
fn file_name_has_calls_and_range() {
    let n = crossing_file_name(
        "TST/1",
        "TST2",
        Utc.with_ymd_and_hms(2024, 5, 1, 0, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2024, 5, 2, 13, 4, 5).unwrap(),
    );
    assert_eq!(
        n,
        "Path Crossing History_TST_1_TST2_20240501_000000_to_20240502_130405.csv"
    );
}

#[test]
fn csv_header_has_separators_and_21_fields() {
    assert_eq!(CROSSING_HEADER.split(';').count(), 21);
    assert!(CROSSING_HEADER.starts_with("time[utc];call;lat[deg];lon[deg];type;category;"));
    assert!(CROSSING_HEADER.ends_with(";signal_strength[dB];ambiguous"));

    let mut c = vec![crossing("abc123", 0), crossing("abc123", 1)];
    c[0].potential = 100;
    assert!(!c.is_empty());
    mark_ambiguous(&mut c, 10.0);
    let mut out = Vec::new();
    write_crossing_csv(&mut out, &c, &path()).unwrap();
    let text = String::from_utf8(out).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next().unwrap(), CROSSING_HEADER);
    let rows: Vec<Vec<&str>> = lines.map(|l| l.split(';').collect()).collect();
    assert_eq!(rows.len(), c.len());
    for r in &rows {
        assert_eq!(r.len(), 21, "{r:?}");
    }
    let r = &rows[0];
    assert_eq!(r[0], c[0].time.format("%Y-%m-%d %H:%M:%S").to_string());
    assert_eq!(r[1], "TST123");
    assert_eq!(r[2], format!("{:.8}", c[0].lat));
    assert_eq!(r[4], "A320");
    assert_eq!(r[5], "MEDIUM");
    assert_eq!(r[6], "100");
    // no signal: -255 with two decimals, as the C# `F2`
    assert_eq!(r[19], "-255.00");
    assert!(r[20] == "True" || r[20] == "False");
    // dist1 / dist2: distance of the plane to station 1 and 2
    let d1 = as_geo::latlon::distance(50.0, 8.0, c[0].lat, c[0].lon);
    assert_eq!(r[13], format!("{d1:.8}"));
    // the crossing angle is written in degrees
    assert_eq!(
        r[11],
        format!("{:.8}", c[0].angle / std::f64::consts::PI * 180.0)
    );
}
