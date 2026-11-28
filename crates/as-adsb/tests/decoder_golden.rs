//! Golden test: the stateful `ADSBDecoder` against C# under a fake clock
//! (`tests/golden/adsb_decoder.json`).
use as_adsb::decoder::{AdsbInfo, AdsbState, Decoder};
use as_golden::deltas::DeltaManifest;
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

#[allow(clippy::too_many_arguments)]
fn expected_opt_i32(
    deltas: &mut DeltaManifest,
    case_index: usize,
    step_index: usize,
    state_index: usize,
    field: &str,
    csharp: Option<i32>,
    reference: Option<i32>,
    findings: &str,
) -> Option<i32> {
    let key = format!("step[{step_index}].state[{state_index}].{field}");
    let csharp_v = csharp.map(Value::from).unwrap_or(Value::Null);
    let reference_v = reference.map(Value::from).unwrap_or(Value::Null);
    match deltas.expected(case_index, &key, &csharp_v, reference_v, 0.0, findings) {
        Value::Null => None,
        Value::Number(n) => Some(n.as_i64().expect("i64") as i32),
        other => panic!("unexpected delta value {other:?}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn expected_opt_ts(
    deltas: &mut DeltaManifest,
    case_index: usize,
    step_index: usize,
    state_index: usize,
    field: &str,
    csharp: Option<Ts>,
    reference: Option<Ts>,
    findings: &str,
) -> Option<Ts> {
    let key = format!("step[{step_index}].state[{state_index}].{field}");
    let csharp_v = csharp
        .map(|t| Value::from((t - t0()).num_milliseconds() as f64 / 1000.0))
        .unwrap_or(Value::Null);
    let reference_v = reference
        .map(|t| Value::from((t - t0()).num_milliseconds() as f64 / 1000.0))
        .unwrap_or(Value::Null);
    match deltas.expected(case_index, &key, &csharp_v, reference_v, 0.0, findings) {
        Value::Null => None,
        Value::Number(n) => Some(at(n.as_f64().expect("f64"))),
        other => panic!("unexpected timestamp delta value {other:?}"),
    }
}

fn check_state(
    s: &AdsbState,
    e: &Value,
    ctx: &str,
    deltas: &mut DeltaManifest,
    case_index: usize,
    step_index: usize,
    state_index: usize,
) {
    assert_eq!(s.icao24, e["icao24"].as_str().unwrap(), "{ctx}");
    assert_eq!(s.call, e["call"].as_str().unwrap(), "{ctx}");
    let finding_359_last_pos = match (case_index, step_index, state_index) {
        (0, 9..=usize::MAX, 0) => Some(at(9.0)),
        (3, 2..=usize::MAX, 0) => Some(at(3.0)),
        (4, 6..=usize::MAX, 0) => Some(at(7.0)),
        _ => opt_ts(&e["last_pos"]),
    };
    let finding_359_last_even = match (case_index, step_index, state_index) {
        (0, 9..=usize::MAX, 0) => Some(at(9.0)),
        _ => opt_ts(&e["last_even"]),
    };
    let finding_359_last_odd = match (case_index, step_index, state_index) {
        (3, 2..=usize::MAX, 0) => Some(at(3.0)),
        (4, 6..=usize::MAX, 0) => Some(at(7.0)),
        _ => opt_ts(&e["last_odd"]),
    };
    let expected_timestamp = expected_opt_ts(
        deltas,
        case_index,
        step_index,
        state_index,
        "timestamp",
        opt_ts(&e["timestamp"]),
        finding_359_last_pos
            .or_else(|| opt_ts(&e["last_pos"]))
            .or_else(|| opt_ts(&e["timestamp"])),
        "#45",
    );
    assert_eq!(Some(s.timestamp), expected_timestamp, "{ctx} timestamp");
    same_f(s.lat, &e["lat"], ctx);
    same_f(s.lon, &e["lon"], ctx);
    assert_eq!(s.baro_alt_ft, opt_i(&e["baro_alt"]), "{ctx} baro_alt");
    let expected_geo = expected_opt_i32(
        deltas,
        case_index,
        step_index,
        state_index,
        "geo_minus_baro",
        opt_i(&e["geo_minus_baro"]),
        match (case_index, step_index, state_index) {
            (6, 0, 0) | (6, 2, 0) => None,
            _ => opt_i(&e["geo_minus_baro"]),
        },
        "#35",
    );
    assert_eq!(s.geo_minus_baro_ft, expected_geo, "{ctx} geo");
    let expected_heading = expected_opt_i32(
        deltas,
        case_index,
        step_index,
        state_index,
        "heading",
        opt_i(&e["heading"]),
        match (case_index, step_index) {
            (6, 0) => None,
            (6, 2) => Some(183),
            _ => opt_i(&e["heading"]),
        },
        "#34",
    );
    assert_eq!(s.heading, expected_heading, "{ctx} heading");
    assert_eq!(s.speed_kts, opt_i(&e["speed"]), "{ctx} speed");
    let expected_last_even = expected_opt_ts(
        deltas,
        case_index,
        step_index,
        state_index,
        "last_even",
        opt_ts(&e["last_even"]),
        finding_359_last_even,
        "#359",
    );
    assert_eq!(s.last_even, expected_last_even, "{ctx} last_even");
    let expected_last_odd = expected_opt_ts(
        deltas,
        case_index,
        step_index,
        state_index,
        "last_odd",
        opt_ts(&e["last_odd"]),
        finding_359_last_odd,
        "#359",
    );
    assert_eq!(s.last_odd, expected_last_odd, "{ctx} last_odd");
    let expected_last_pos = expected_opt_ts(
        deltas,
        case_index,
        step_index,
        state_index,
        "last_pos",
        opt_ts(&e["last_pos"]),
        finding_359_last_pos,
        "#359",
    );
    assert_eq!(s.last_pos, expected_last_pos, "{ctx} last_pos");
    assert_eq!(
        i64::from(s.num_pos),
        e["num_pos"].as_i64().unwrap(),
        "{ctx} num_pos"
    );
}

fn check_info(p: &AdsbInfo, e: &Value, ctx: &str) {
    assert_eq!(p.icao24, e["icao24"].as_str().unwrap(), "{ctx}");
    assert_eq!(p.call, e["call"].as_str().unwrap(), "{ctx}");
    assert!(p.timestamp <= chrono::Utc::now(), "{ctx}");
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

fn expected_len(
    deltas: &mut DeltaManifest,
    case_index: usize,
    step_index: usize,
    csharp: usize,
    reference: usize,
    findings: &str,
) -> usize {
    let key = format!("step[{step_index}].planes_len");
    deltas
        .expected(
            case_index,
            &key,
            &Value::from(csharp as i64),
            Value::from(reference as i64),
            0.0,
            findings,
        )
        .as_i64()
        .expect("i64 delta") as usize
}

#[test]
fn decoder_golden() {
    let g = load("adsb_decoder.json");
    let mut deltas =
        DeltaManifest::load_from("docs/logic-fixes-area2/golden-deltas.tsv", "adsb_decoder");
    for (case_index, c) in g.by_fn("scenario").enumerate() {
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
                let got = dec.decode_message(st["raw"].as_str().unwrap(), now, None);
                assert_eq!(
                    got.is_err(),
                    r.get("error").is_some(),
                    "{ctx}: {got:?} vs {r:?}"
                );
            } else {
                let planes = dec.planes();
                let exp = r["planes"].as_array().unwrap();
                let expected_planes_len = expected_len(
                    &mut deltas,
                    case_index,
                    k,
                    exp.len(),
                    match (case_index, k) {
                        (1, 6) => 1,
                        (4, 5) => 1,
                        _ => exp.len(),
                    },
                    "#45",
                );
                assert_eq!(planes.len(), expected_planes_len, "{ctx} planes");
                for (p, e) in planes.iter().zip(exp) {
                    check_info(p, e, &ctx);
                }
            }
            let states = dec.states();
            let exp = r["state"].as_array().unwrap();
            assert_eq!(states.len(), exp.len(), "{ctx} state len");
            for (state_index, (s, e)) in states.iter().zip(exp).enumerate() {
                check_state(s, e, &ctx, &mut deltas, case_index, k, state_index);
            }
        }
    }
    deltas.finish();
}
