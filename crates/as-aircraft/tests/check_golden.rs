//! Golden test: `PlaneInfoChecker` / `PlaneInfoConverter` against C# (`tests/golden/plane_checker.json`).
//! The lookup table mirrors `StubAircraftDatabase` in `tools/golden-harness/UpstreamAircraft.cs`.
use as_aircraft::check::{self, find_reg_prefix, Lookup};
use as_aircraft::convert;
use as_aircraft::{PlaneCategory, PlaneInfo};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};

struct StaticLookup;

impl Lookup for StaticLookup {
    fn airline_by_icao(&self, s: &str) -> bool {
        ["DLH", "BAW", "KLM"].contains(&s)
    }
    fn airline_by_iata(&self, s: &str) -> bool {
        ["LH", "BA", "KL"].contains(&s)
    }
    fn registration_by_reg(&self, reg: &str) -> bool {
        find_reg_prefix(reg, |p| {
            Ok::<_, ()>(["D", "G", "HB", "N", "PH"].contains(&p))
        })
        .ok()
        .flatten()
        .is_some()
    }
    fn airport_by_icao(&self, s: &str) -> bool {
        ["EDDF", "EGLL"].contains(&s)
    }
    fn type_by_icao(&self, s: &str) -> bool {
        s == "A320"
    }
    fn registration_min_len(&self) -> usize {
        1
    }
    fn type_icao_min_len(&self) -> usize {
        4
    }
}

fn opt<T: Into<Value>>(v: Option<T>) -> Value {
    v.map(Into::into).unwrap_or(Value::Null)
}

fn utc(t: Option<DateTime<Utc>>) -> Value {
    opt(t.map(|t| {
        use chrono::Timelike;
        format!(
            "{}.{:07}",
            t.format("%Y-%m-%dT%H:%M:%S"),
            t.nanosecond() / 100
        )
    }))
}

#[test]
fn checker_and_converter_match_csharp() {
    let g = as_golden::load("plane_checker.json");
    let l = StaticLookup;
    let mut n = 0;
    for c in &g.cases {
        let got: Value = match c.func.as_str() {
            "check_lat" | "check_lon" | "check_alt" | "check_track" | "check_speed" => {
                let v = c.f("v");
                json!(match c.func.as_str() {
                    "check_lat" => check::lat(v),
                    "check_lon" => check::lon(v),
                    "check_alt" => check::alt(v),
                    "check_track" => check::track(v),
                    _ => check::speed(v),
                })
            }
            "check_plane" => {
                let t = if c.b("min_time") {
                    DateTime::<Utc>::MIN_UTC
                } else {
                    Utc.with_ymd_and_hms(2023, 11, 14, 22, 13, 20).unwrap()
                };
                let p = PlaneInfo::new(
                    t,
                    &c.s("call"),
                    "D-AIZZ",
                    &c.s("hex"),
                    c.f("lat"),
                    c.f("lon"),
                    c.f("track"),
                    c.f("alt"),
                    c.f("speed"),
                    "A320",
                    "Airbus",
                    "A320",
                    PlaneCategory::Medium,
                );
                json!(check::plane(&p))
            }
            f => {
                let s = c.s("s");
                match f {
                    "check_hex" => json!(check::hex(&s)),
                    "check_call" => json!(check::call(&s, &l)),
                    "check_reg" => json!(check::reg(&s, &l)),
                    "check_type" => json!(check::type_code(&s, &l)),
                    "to_utc" => utc(convert::to_utc(&s)),
                    "to_hex" => opt(convert::to_hex(&s)),
                    "to_reg" => opt(convert::to_reg(&s, &l)),
                    "to_flight" => opt(convert::to_flight(&s, &l)),
                    "to_type" => opt(convert::to_type(&s, true, &l)),
                    "to_type_nocheck" => opt(convert::to_type(&s, false, &l)),
                    "to_radar" => opt(convert::to_radar(&s)),
                    "to_route" => opt(convert::to_route(&s, &l)),
                    "to_call" => opt(convert::to_call(&s, true, &l)),
                    "to_call_nocheck" => opt(convert::to_call(&s, false, &l)),
                    "to_squawk" => opt(convert::to_squawk(&s)),
                    "to_lat" => opt(convert::to_lat(&s)),
                    "to_lon" => opt(convert::to_lon(&s)),
                    "to_alt" => opt(convert::to_alt(&s)),
                    "to_track" => opt(convert::to_track(&s)),
                    "to_speed" => opt(convert::to_speed(&s)),
                    other => panic!("unknown fn {other}"),
                }
            }
        };
        let want = &c.result;
        let same = match (got.as_f64(), want.as_f64()) {
            (Some(a), Some(b)) => a == b,
            _ => &got == want,
        };
        assert!(same, "{} {:?}: rust {got} != c# {want}", c.func, c.args);
        n += 1;
    }
    assert!(n > 2000, "{n}");
}
