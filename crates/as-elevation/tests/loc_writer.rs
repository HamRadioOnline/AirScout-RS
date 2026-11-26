//! `.loc` writer in the served layout, `files.cat` reader and palette lookup.
mod common;

use as_elevation::{
    framework_number_text, loc_date_text, palette_color, read_files_cat, read_files_zip,
    ElevationError, ElevationTile,
};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use std::io::Write;

/// The significant digits of a plain decimal number text (no sign, point or leading zeros).
fn significant_digits(s: &str) -> String {
    s.trim_start_matches('-')
        .replace('.', "")
        .trim_start_matches('0')
        .to_string()
}

/// The number after `"Name": ` on a `.loc` member line, without the trailing comma.
fn member_number(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.trim_start().split_once(": ")?;
    Some((name, value.trim_end_matches(',')))
}

/// Pre-flight C1: the writer keeps correctly rounded 17-digit numbers. The served tiles came
/// from the .NET Framework digit generator, which writes the 17th digit one higher in 189 of
/// the 9 220 fixture coordinates. Everything else must match byte for byte.
#[test]
fn served_tiles_round_trip_byte_exact() {
    let texts = common::fixture_texts();
    assert!(texts.len() >= 2000, "only {} fixture texts", texts.len());
    assert_eq!(texts.len(), 2305);
    let mut exact = 0;
    let mut differing_numbers = 0;
    for (name, served) in &texts {
        let ours = ElevationTile::from_loc_json(served)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .to_loc_json();
        if ours == *served {
            exact += 1;
            continue;
        }
        let ours_lines: Vec<&str> = ours.split("\r\n").collect();
        let served_lines: Vec<&str> = served.split("\r\n").collect();
        assert_eq!(ours_lines.len(), served_lines.len(), "{name}: line count");
        for (o, s) in ours_lines.iter().zip(&served_lines) {
            if o == s {
                continue;
            }
            let (Some((on, ov)), Some((sn, sv))) = (member_number(o), member_number(s)) else {
                panic!("{name}: line differs:\n ours   {o}\n served {s}");
            };
            assert_eq!(on, sn, "{name}: member differs");
            assert!(
                ["\"MinLat\"", "\"MinLon\"", "\"MaxLat\"", "\"MaxLon\""].contains(&on),
                "{name}: non-coordinate line differs:\n ours   {o}\n served {s}"
            );
            let (of, sf): (f64, f64) = (ov.parse().unwrap(), sv.parse().unwrap());
            assert_eq!(of.to_bits(), sf.to_bits(), "{name}: {on} value differs");
            let (od, sd) = (significant_digits(ov), significant_digits(sv));
            assert_eq!((od.len(), sd.len()), (17, 17), "{name}: {ov} vs {sv}");
            let (oi, si): (u64, u64) = (od.parse().unwrap(), sd.parse().unwrap());
            assert_eq!(
                si,
                oi + 1,
                "{name}: served {sv} is not ours {ov} plus one digit"
            );
            differing_numbers += 1;
        }
    }
    assert_eq!(exact, 2119, "byte-exact texts");
    assert_eq!(differing_numbers, 189, "17th-digit coordinate lines");
}

#[test]
fn number_text_framework_r() {
    for (x, want) in [
        (10.008333333333333, "10.008333333333333"),
        (50.025, "50.025"),
        (10.0, "10.0"),
        (0.0, "0.0"),
        (-0.0, "0.0"),
        (-12.5, "-12.5"),
        (0.1 + 0.2, "0.30000000000000004"),
        (1.0 / 3.0, "0.33333333333333331"),
        (1e-6, "1E-06"),
        (1e15, "1E+15"),
        (0.0001, "0.0001"),
    ] {
        assert_eq!(framework_number_text(x), want, "{x:?}");
    }
}

#[test]
fn date_text_trims_fraction() {
    let t: DateTime<Utc> = "2018-01-24T20:37:30.7992685Z".parse().unwrap();
    assert_eq!(loc_date_text(&t), "2018-01-24T20:37:30.7992685Z");
    let noon = Utc.with_ymd_and_hms(2026, 11, 24, 12, 0, 0).unwrap();
    assert_eq!(loc_date_text(&noon), "2026-11-24T12:00:00Z");
    let half = noon + chrono::Duration::milliseconds(500);
    assert_eq!(loc_date_text(&half), "2026-11-24T12:00:00.5Z");
    let ns = noon + chrono::Duration::nanoseconds(123_456_789);
    assert_eq!(loc_date_text(&ns), "2026-11-24T12:00:00.1234567Z");
}

fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(y, mo, d)
        .unwrap()
        .and_hms_opt(h, mi, s)
        .unwrap()
        .and_utc()
}

/// Review Focus 4: junk lines are skipped, the first entry of a name wins.
#[test]
fn files_cat_skips_junk() {
    let text = "// h\r\n\r\nJO50AA.loc;2018-02-27 08:26:32Z\r\njo50ab.loc;2018-02-27 08:26:33Z\r\nbroken line\r\nJO50AC.loc;not a date\r\nJO50AA.loc;2019-01-01 00:00:00Z\r\nÄÖ.loc;2018-02-27 08:26:34Z\r\n";
    let map = read_files_cat(text.as_bytes()).unwrap();
    let want: Vec<(String, DateTime<Utc>)> = vec![
        ("JO50AA".into(), at(2018, 2, 27, 8, 26, 32)),
        ("JO50AB".into(), at(2018, 2, 27, 8, 26, 33)),
        ("ÄÖ".into(), at(2018, 2, 27, 8, 26, 34)),
    ];
    assert_eq!(map.into_iter().collect::<Vec<_>>(), want);
}

#[test]
fn files_zip_without_cat_is_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("files.zip");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    z.start_file("x.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"hello").unwrap();
    z.finish().unwrap();
    let r = read_files_zip(&path);
    assert!(matches!(r, Err(ElevationError::Corrupt(_))), "{r:?}");
}

#[test]
fn palette_color_lookup() {
    assert_eq!(palette_color(0.0), [0, 97, 71]);
    assert_eq!(palette_color(100.0), [255, 255, 255]);
    assert_eq!(palette_color(50.0), [126, 75, 75]);
    assert_eq!(palette_color(-1.0), [0, 0, 0]);
    assert_eq!(palette_color(100.5), [0, 0, 0]);
    assert_eq!(palette_color(f64::NAN), [0, 0, 0]);
}
