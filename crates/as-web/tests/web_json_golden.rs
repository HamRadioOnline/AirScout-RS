//! Byte comparison against the text Newtonsoft.Json writes in the C# harness
//! (`tests/golden/web_json.json`, `tools/golden-harness/WebJsonCases.cs`).
use as_golden::Case;
use as_web::dto::{BandSettingJson, GPointJson, NameValueJson, PlaneInfoJson};
use as_web::json::{to_compact, to_indented, Num};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::Serialize;

fn golden() -> as_golden::Golden {
    as_golden::load("web_json.json")
}

/// The C# text of a case, with Windows line ends made `\n`.
fn expected(c: &Case) -> String {
    c.result
        .as_str()
        .unwrap_or_else(|| panic!("result not a string: {c:?}"))
        .replace("\r\n", "\n")
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 24, 12, 0, 0).unwrap()
}

#[track_caller]
fn check(c: &Case, actual: String) {
    assert_eq!(actual, expected(c), "case {:?} {}", c.func, c.args);
}

#[test]
fn numbers() {
    let g = golden();
    let mut n = 0;
    for c in g.by_fn("number") {
        let bits: i64 = c.s("bits").parse().unwrap();
        let x = f64::from_bits(bits as u64);
        check(c, to_indented(&vec![Num(x)]));
        n += 1;
    }
    assert_eq!(n, 22);
}

#[derive(Serialize)]
struct D(#[serde(with = "as_web::json::date")] DateTime<Utc>);

#[derive(Serialize)]
struct DMin(#[serde(with = "as_web::json::date_or_min")] Option<DateTime<Utc>>);

#[test]
fn dates() {
    let g = golden();
    let mut n = 0;
    for c in g.by_fn("date") {
        let t = Utc.timestamp_opt(c.i("unix_s"), 0).unwrap()
            + Duration::nanoseconds(c.i("ticks") * 100);
        check(c, to_indented(&D(t)));
        n += 1;
    }
    assert_eq!(n, 3);
    let c = g.by_fn("date_min").next().unwrap();
    check(c, to_indented(&DMin(None)));
}

#[test]
fn strings() {
    let g = golden();
    let c = g.by_fn("string").next().unwrap();
    check(c, to_indented(&vec![c.s("text")]));
}

fn plane_default() -> PlaneInfoJson {
    PlaneInfoJson {
        track: Num(0.0),
        int_point: None,
        int_qrb: Num(f64::MAX),
        alt_diff: Num(0.0),
        potential: 0,
        eps1: Num(0.0),
        eps2: Num(0.0),
        theta1: Num(0.0),
        theta2: Num(0.0),
        angle: Num(0.0),
        squint: Num(0.0),
        signal_strength: Num(f64::MIN),
        ambiguous: false,
        comment: String::new(),
        old_time: None,
        old_lat: Num(0.0),
        old_lon: Num(0.0),
        old_alt: Num(0.0),
        old_track: Num(0.0),
        old_speed: Num(0.0),
        time: t0(),
        call: String::new(),
        reg: String::new(),
        hex: String::new(),
        lat: Num(0.0),
        lon: Num(0.0),
        alt: Num(0.0),
        alt_m: Num(0.0),
        speed: Num(0.0),
        speed_kmh: Num(0.0),
        type_: String::new(),
        manufacturer: String::new(),
        model: String::new(),
        category: 0,
        from: String::new(),
        to: String::new(),
        v_speed: 0,
        altitude_estimate: None,
        altitude_provenance: None,
    }
}

#[test]
fn plane_info() {
    let g = golden();
    check(
        g.by_fn("plane_info_default").next().unwrap(),
        to_indented(&plane_default()),
    );

    let c = g.by_fn("plane_info_intpoint").next().unwrap();
    let p = PlaneInfoJson {
        track: Num(87.5),
        int_point: Some(GPointJson {
            lat: Num(50.125),
            lon: Num(11.0625),
        }),
        int_qrb: Num(123.456),
        alt_diff: Num(-1500.5),
        potential: 75,
        eps1: Num(0.25),
        eps2: Num(-0.125),
        theta1: Num(1.5),
        theta2: Num(2.5),
        angle: Num(33.3),
        squint: Num(4.75),
        signal_strength: Num(-97.5),
        ambiguous: true,
        comment: "synthetic".into(),
        old_time: Some(t0() - Duration::seconds(30)),
        old_lat: Num(50.2),
        old_lon: Num(11.1),
        old_alt: Num(34900.0),
        old_track: Num(87.0),
        old_speed: Num(449.0),
        time: t0(),
        call: "TST123".into(),
        reg: "X-TEST".into(),
        hex: "ABC123".into(),
        lat: Num(50.25),
        lon: Num(11.125),
        alt: Num(35000.0),
        alt_m: Num(c.f("alt_m")),
        speed: Num(450.0),
        speed_kmh: Num(c.f("speed_kmh")),
        type_: "A320".into(),
        manufacturer: "Airbus".into(),
        model: "A320-200".into(),
        category: 2,
        from: "XXAA".into(),
        to: "XXBB".into(),
        v_speed: -640,
        altitude_estimate: None,
        altitude_provenance: None,
    };
    // the C# setters: Alt_m = Alt / 3.28084, Speed_kmh = Speed * 1.852
    assert_eq!(p.alt_m, Num(35000.0 / 3.28084));
    assert_eq!(p.speed_kmh, Num(450.0 * 1.852));
    check(c, to_indented(&p));
}

fn nv(name: &str, value: i32) -> NameValueJson {
    NameValueJson {
        name: name.into(),
        value,
    }
}

#[test]
fn band_and_category_lists() {
    let g = golden();
    let bands = [
        ("50M", 50),
        ("70M", 70),
        ("144M", 144),
        ("432M", 432),
        ("1.2G", 1296),
        ("2.3G", 2320),
        ("3.4G", 3400),
        ("5.7G", 5760),
        ("10G", 10368),
        ("24G", 24048),
        ("47G", 47088),
        ("76G", 76032),
    ];
    let bands: Vec<_> = bands.iter().map(|(n, v)| nv(n, *v)).collect();
    check(g.by_fn("band_values").next().unwrap(), to_compact(&bands));

    let cats: Vec<_> = ["None", "Light", "Medium", "Heavy", "Superheavy"]
        .iter()
        .enumerate()
        .map(|(i, n)| nv(n, i as i32))
        .collect();
    check(
        g.by_fn("aircraft_categories").next().unwrap(),
        to_compact(&cats),
    );
}

#[test]
fn band_settings_table() {
    let g = golden();
    let rows = vec![
        BandSettingJson {
            band: "144M".into(),
            k_factor: Num(1.5),
            f1_clearance: Num(0.2),
            ground_clearance: Num(0.0),
            max_distance: Num(10.0),
            max_squint: Num(f64::MAX),
            max_elevation: Num(f64::MAX),
        },
        BandSettingJson {
            band: "1.2G".into(),
            k_factor: Num(1.33),
            f1_clearance: Num(0.6),
            ground_clearance: Num(2.5),
            max_distance: Num(100.0),
            max_squint: Num(45.0),
            max_elevation: Num(90.0),
        },
    ];
    check(g.by_fn("band_settings").next().unwrap(), to_indented(&rows));
}

#[test]
fn every_case_is_checked() {
    let checked = [
        "number",
        "date",
        "date_min",
        "string",
        "plane_info_default",
        "plane_info_intpoint",
        "band_values",
        "aircraft_categories",
        "band_settings",
    ];
    for c in &golden().cases {
        assert!(
            checked.contains(&c.func.as_str()),
            "unchecked case {}",
            c.func
        );
    }
}
