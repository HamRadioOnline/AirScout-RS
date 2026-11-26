//! Number, date and layout rules of the Newtonsoft-compatible writer.
use as_web::json::{date_text, number_text, to_compact, to_indented, Num};
use chrono::{DateTime, TimeZone, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

#[test]
fn number_text_layout() {
    // Newtonsoft 13 on net8: shortest round-trip digits, exponent form from 1E+17 and
    // below 1E-04, ".0" appended to whole numbers.
    assert_eq!(number_text(1.0), "1.0");
    assert_eq!(number_text(0.0), "0.0");
    assert_eq!(number_text(0.1), "0.1");
    assert_eq!(number_text(1e-5), "1E-05");
    assert_eq!(number_text(0.0001), "0.0001");
    assert_eq!(number_text(1e15), "1000000000000000.0");
    assert_eq!(number_text(1e16), "10000000000000000.0");
    assert_eq!(number_text(1e17), "1E+17");
    assert_eq!(number_text(1.5e17), "1.5E+17");
    assert_eq!(number_text(123456789012345.0), "123456789012345.0");
    assert_eq!(number_text(f64::MAX), "1.7976931348623157E+308");
    assert_eq!(number_text(f64::MIN), "-1.7976931348623157E+308");
    assert_eq!(number_text(5e-324), "5E-324");
    assert_eq!(number_text(-2.5e-7), "-2.5E-07");
    assert_eq!(number_text(-0.0), "-0.0");
    assert_eq!(number_text(-12.75), "-12.75");
}

#[test]
fn number_text_non_finite_does_not_panic() {
    // The .NET "R" texts, unquoted; `Num` quotes them.
    assert_eq!(number_text(f64::NAN), "NaN");
    assert_eq!(number_text(f64::INFINITY), "Infinity");
    assert_eq!(number_text(f64::NEG_INFINITY), "-Infinity");
}

#[test]
fn non_finite_as_strings() {
    assert_eq!(
        to_compact(&[Num(f64::NAN), Num(f64::INFINITY)]),
        r#"["NaN","Infinity"]"#
    );
    assert_eq!(to_compact(&[Num(f64::NEG_INFINITY)]), r#"["-Infinity"]"#);
    assert_eq!(to_compact(&[Num(2.0), Num(0.5)]), "[2.0,0.5]");
}

fn noon() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 24, 12, 0, 0).unwrap()
}

#[test]
fn dates() {
    assert_eq!(date_text(&noon()), "2026-11-24T12:00:00Z");
    let half = noon() + chrono::Duration::milliseconds(500);
    assert_eq!(date_text(&half), "2026-11-24T12:00:00.5Z");
    let ticks = noon() + chrono::Duration::nanoseconds(123_456_789);
    assert_eq!(date_text(&ticks), "2026-11-24T12:00:00.1234567Z");
}

#[derive(Serialize)]
struct Dates {
    #[serde(with = "as_web::json::date")]
    a: DateTime<Utc>,
    #[serde(with = "as_web::json::date_or_min")]
    b: Option<DateTime<Utc>>,
    #[serde(with = "as_web::json::date_or_min")]
    c: Option<DateTime<Utc>>,
}

#[test]
fn date_modules() {
    let d = Dates {
        a: noon(),
        b: None,
        c: Some(noon()),
    };
    assert_eq!(
        to_compact(&d),
        r#"{"a":"2026-11-24T12:00:00Z","b":"0001-01-01T00:00:00Z","c":"2026-11-24T12:00:00Z"}"#
    );
}

#[test]
fn indent_matches_newtonsoft() {
    let mut v: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    v.insert("A", serde_json::json!([1, 2]));
    v.insert("B", serde_json::json!({}));
    assert_eq!(
        to_indented(&v),
        "{\n  \"A\": [\n    1,\n    2\n  ],\n  \"B\": {}\n}"
    );
}

#[test]
fn newtonsoft_extra_escapes() {
    assert_eq!(
        to_compact(&["\u{85}\u{2028}\u{2029}\u{7f}é"]),
        "[\"\\u0085\\u2028\\u2029\u{7f}é\"]"
    );
}
