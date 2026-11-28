//! Golden test: `PlaneInfoCache.Get` / `GetAll` against C# (`tests/golden/plane_cache.json`).
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_golden::{deltas::DeltaManifest, load, num};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{json, Value};

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap()
}

fn at(secs: f64) -> DateTime<Utc> {
    t0() + Duration::milliseconds((secs * 1000.0) as i64)
}

fn plane_json(got: &PlaneInfo) -> Value {
    json!({
        "hex": got.hex,
        "time": (got.time - t0()).num_milliseconds() as f64 / 1000.0,
        "lat": got.lat,
        "lon": got.lon,
        "alt": got.alt_ft,
        "track": got.track,
        "speed": got.speed_kts,
        "old_time": got.old_time.map(|t| (t - t0()).num_milliseconds() as f64 / 1000.0),
        "old_lat": got.old_lat,
        "old_lon": got.old_lon,
        "old_alt": got.old_alt_ft,
        "old_track": got.old_track,
        "old_speed": got.old_speed_kts,
    })
}

fn findings(index: usize) -> &'static str {
    let has_23 = [22, 23, 24, 25, 26, 27, 28, 29, 30, 31].contains(&index);
    let has_355 = [12, 13, 14, 15, 32, 33, 34, 35, 58, 59, 62, 63].contains(&index);
    if index <= 1 || (has_23 && has_355) {
        "#23/#355/#356"
    } else if has_23 {
        "#23/#356"
    } else if has_355 {
        "#355/#356"
    } else {
        "#356"
    }
}

fn equivalent(a: &Value, b: &Value, tol: f64) -> bool {
    if let (Some(x), Some(y)) = (num(a), num(b)) {
        (x.is_nan() && y.is_nan()) || x == y || (x - y).abs() <= tol
    } else {
        match (a, b) {
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| equivalent(x, y, tol))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, x)| b.get(k).is_some_and(|y| equivalent(x, y, tol)))
            }
            _ => a == b,
        }
    }
}

#[test]
fn cache_golden() {
    let g = load("plane_cache.json");
    let mut deltas =
        DeltaManifest::load_from("docs/logic-fixes-area2/golden-deltas.tsv", "plane_cache");
    for (index, c) in g.cases.iter().enumerate() {
        let cache = PlaneInfoCache::new();
        let mut inserted = Vec::new();
        for r in c.args["reports"].as_array().unwrap() {
            let f = |k: &str| r[k].as_f64().unwrap();
            let mut p = PlaneInfo::new(
                at(f("t")),
                "DLH1A",
                "D-AIZZ",
                r["hex"].as_str().unwrap(),
                f("lat"),
                f("lon"),
                f("track"),
                f("alt"),
                f("speed"),
                "A320",
                "Airbus",
                "A320",
                PlaneCategory::Medium,
            );
            p.from = "EDDF".into();
            p.vspeed = 500;
            inserted.push(Value::from(cache.insert_or_update_if_newer(&p) as i64));
        }
        let q = at(c.f("at"));
        let ttl = c.i("ttl");
        let got_result = if c.func == "get" {
            let hex = c.args["reports"].as_array().unwrap().last().unwrap()["hex"]
                .as_str()
                .unwrap();
            let plane = cache
                .get(hex, q, ttl)
                .map(|p| plane_json(&p))
                .unwrap_or(Value::Null);
            json!({ "inserted": inserted, "plane": plane })
        } else {
            let planes = cache
                .get_all(q, ttl)
                .iter()
                .map(plane_json)
                .collect::<Vec<_>>();
            json!({ "inserted": inserted, "planes": planes })
        };
        let expected = deltas.expected(
            index,
            "result",
            &c.result,
            got_result.clone(),
            1e-9,
            findings(index),
        );
        assert!(equivalent(&got_result, &expected, 1e-9), "case {index}");
    }
    deltas.finish();
}
