//! Golden test: `PlaneInfoCache.Get` / `GetAll` against C# (`tests/golden/plane_cache.json`).
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_golden::{assert_close, num};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::Value;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap()
}

fn at(secs: f64) -> DateTime<Utc> {
    t0() + Duration::milliseconds((secs * 1000.0) as i64)
}

fn check_plane(got: &PlaneInfo, want: &Value, ctx: &str) {
    assert_eq!(got.hex, want["hex"].as_str().unwrap(), "{ctx}");
    let secs = (got.time - t0()).num_milliseconds() as f64 / 1000.0;
    assert_close(secs, num(&want["time"]).unwrap(), 1e-9, &ctx);
    for (g, k) in [
        (got.lat, "lat"),
        (got.lon, "lon"),
        (got.alt_ft, "alt"),
        (got.track, "track"),
        (got.speed_kts, "speed"),
        (got.old_lat, "old_lat"),
        (got.old_lon, "old_lon"),
        (got.old_alt_ft, "old_alt"),
        (got.old_track, "old_track"),
        (got.old_speed_kts, "old_speed"),
    ] {
        assert_close(g, num(&want[k]).unwrap(), 1e-9, &format!("{ctx} {k}"));
    }
    let old = got
        .old_time
        .map(|t| (t - t0()).num_milliseconds() as f64 / 1000.0);
    assert_eq!(old, num(&want["old_time"]), "{ctx} old_time");
    // the C# copy constructor does not copy From/To/VSpeed
    assert_eq!(
        (got.from.as_str(), got.to.as_str(), got.vspeed),
        ("", "", 0),
        "{ctx}"
    );
}

#[test]
fn cache_golden() {
    let g = as_golden::load("plane_cache.json");
    for c in &g.cases {
        let ctx = format!("{} {} at {}", c.func, c.args["scenario"], c.args["at"]);
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
        assert_eq!(&Value::Array(inserted), &c.result["inserted"], "{ctx}");
        let q = at(c.f("at"));
        let ttl = c.i("ttl");
        if c.func == "get" {
            let hex = c.args["reports"].as_array().unwrap().last().unwrap()["hex"]
                .as_str()
                .unwrap();
            match (cache.get(hex, q, ttl), &c.result["plane"]) {
                (None, Value::Null) => {}
                (Some(p), w) if !w.is_null() => check_plane(&p, w, &ctx),
                (got, want) => panic!("{ctx}: rust {got:?} vs c# {want}"),
            }
        } else {
            let got = cache.get_all(q, ttl);
            let want = c.result["planes"].as_array().unwrap();
            assert_eq!(got.len(), want.len(), "{ctx}");
            for (g, w) in got.iter().zip(want) {
                check_plane(g, w, &ctx);
            }
        }
    }
}
