//! Golden test: the stateful `ADSBDecoder` against C# under a fake clock
//! (`tests/golden/adsb_decoder.json`).
use as_adsb::decoder::{AdsbInfo, AdsbState, Decoder};
use as_golden::{load, num};
use serde_json::Value;
use std::sync::{Arc, Mutex};

type Ts = chrono::DateTime<chrono::Utc>;

fn t0() -> Ts {
    chrono::DateTime::parse_from_rfc3339("2026-11-23T12:00:00Z")
        .unwrap()
        .to_utc()
}

fn at(secs: f64) -> Ts {
    t0() + chrono::Duration::milliseconds((secs * 1000.0).round() as i64)
}

fn opt_ts(v: &Value) -> Option<Ts> {
    v.as_f64().map(at)
}

fn opt_i(v: &Value) -> Option<i32> {
    v.as_i64().map(|x| x as i32)
}

fn same_f(a: f64, e: &Value, ctx: &str) {
    let e = num(e).unwrap_or_else(|| panic!("{ctx}: {e:?}"));
    assert!(
        (a.is_nan() && e.is_nan()) || (a - e).abs() < 1e-9,
        "{ctx}: {a} vs {e}"
    );
}

fn check_state(s: &AdsbState, e: &Value, ctx: &str) {
    assert_eq!(s.icao24, e["icao24"].as_str().unwrap(), "{ctx}");
    assert_eq!(s.call, e["call"].as_str().unwrap(), "{ctx}");
    assert_eq!(
        Some(s.timestamp),
        opt_ts(&e["timestamp"]),
        "{ctx} timestamp"
    );
    same_f(s.lat, &e["lat"], ctx);
    same_f(s.lon, &e["lon"], ctx);
    assert_eq!(s.baro_alt_ft, opt_i(&e["baro_alt"]), "{ctx} baro_alt");
    assert_eq!(
        s.geo_minus_baro_ft,
        opt_i(&e["geo_minus_baro"]),
        "{ctx} geo"
    );
    assert_eq!(s.heading, opt_i(&e["heading"]), "{ctx} heading");
    assert_eq!(s.speed_kts, opt_i(&e["speed"]), "{ctx} speed");
    assert_eq!(s.last_even, opt_ts(&e["last_even"]), "{ctx} last_even");
    assert_eq!(s.last_odd, opt_ts(&e["last_odd"]), "{ctx} last_odd");
    assert_eq!(s.last_pos, opt_ts(&e["last_pos"]), "{ctx} last_pos");
    assert_eq!(
        i64::from(s.num_pos),
        e["num_pos"].as_i64().unwrap(),
        "{ctx} num_pos"
    );
}

fn check_info(p: &AdsbInfo, e: &Value, ctx: &str) {
    assert_eq!(p.icao24, e["icao24"].as_str().unwrap(), "{ctx}");
    assert_eq!(p.call, e["call"].as_str().unwrap(), "{ctx}");
    assert_eq!(Some(p.timestamp), opt_ts(&e["timestamp"]), "{ctx}");
    same_f(p.lat, &e["lat"], ctx);
    same_f(p.lon, &e["lon"], ctx);
    let f = |v: &Value| opt_i(v).map_or(f64::NAN, f64::from);
    for (a, k) in [
        (p.baro_alt_ft, "baro_alt"),
        (p.geo_minus_baro_ft, "geo_minus_baro"),
        (p.heading, "heading"),
        (p.speed_kts, "speed"),
    ] {
        let x = f(&e[k]);
        assert!(
            (a.is_nan() && x.is_nan()) || a == x,
            "{ctx} {k}: {a} vs {x}"
        );
    }
}

#[test]
fn decoder_golden() {
    let g = load("adsb_decoder.json");
    for c in g.by_fn("scenario") {
        let name = c.s("name");
        let clock = Arc::new(Mutex::new(t0()));
        let ck = clock.clone();
        let mut dec = Decoder::with_clock(5, Box::new(move || *ck.lock().unwrap()));
        let steps = c.args["steps"].as_array().unwrap();
        let results = c.result.as_array().unwrap();
        assert_eq!(steps.len(), results.len());
        for (k, (st, r)) in steps.iter().zip(results).enumerate() {
            let ctx = format!("{name} step {k}");
            let now = at(st["t"].as_f64().unwrap());
            *clock.lock().unwrap() = now;
            if st["op"] == "d" {
                let got = dec.decode_message(st["raw"].as_str().unwrap(), now, false);
                assert_eq!(
                    got.is_err(),
                    r.get("error").is_some(),
                    "{ctx}: {got:?} vs {r:?}"
                );
            } else {
                let planes = dec.planes();
                let exp = r["planes"].as_array().unwrap();
                assert_eq!(planes.len(), exp.len(), "{ctx} planes");
                for (p, e) in planes.iter().zip(exp) {
                    check_info(p, e, &ctx);
                }
            }
            let states = dec.states();
            let exp = r["state"].as_array().unwrap();
            assert_eq!(states.len(), exp.len(), "{ctx} state len");
            for (s, e) in states.iter().zip(exp) {
                check_state(s, e, &ctx);
            }
        }
    }
}
