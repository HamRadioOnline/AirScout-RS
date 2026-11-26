//! Golden test: crossing prediction `GetNearestPlanes` against C# (`tests/golden/nearest_planes.json`).
use as_aircraft::crossing::{get_nearest_planes, merge_active, minutes_to_crossing, nearest_plane};
use as_aircraft::{PlaneCategory, PlaneInfo};
use as_golden::{assert_close, num};
use as_propagation::{PathParams, PropagationPath};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

fn at0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap()
}

fn secs(t: DateTime<Utc>) -> f64 {
    (t - at0()).num_milliseconds() as f64 / 1000.0
}

fn path(v: &Value) -> PropagationPath {
    let p: Vec<f64> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|x| num(x).unwrap())
        .collect();
    PropagationPath::new(
        PathParams {
            lat1: p[0],
            lon1: p[1],
            h1: p[2],
            lat2: p[3],
            lon2: p[4],
            h2: p[5],
            qrg_ghz: 0.144,
            radius_km: 6371.0 * 1.33,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: p[8],
        },
        p[6],
        p[7],
        true,
    )
}

fn planes(v: &Value) -> Vec<PlaneInfo> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let f = |k: &str| num(&p[k]).unwrap();
            PlaneInfo::new(
                at0() + Duration::milliseconds((f("time") * 1000.0) as i64),
                p["call"].as_str().unwrap(),
                "D-AIZZ",
                p["hex"].as_str().unwrap(),
                f("lat"),
                f("lon"),
                f("track"),
                f("alt"),
                f("speed"),
                "A320",
                "Airbus",
                "A320",
                PlaneCategory::Medium,
            )
        })
        .collect()
}

fn check_out(got: &[PlaneInfo], want: &Value, ctx: &str) {
    let want = want.as_array().unwrap();
    assert_eq!(got.len(), want.len(), "{ctx}: count");
    for (g, w) in got.iter().zip(want) {
        let c = format!("{ctx} {}", g.hex);
        assert_eq!(g.hex, w["hex"].as_str().unwrap(), "{c}");
        assert_eq!(
            g.potential as i64,
            w["potential"].as_i64().unwrap(),
            "{c} potential"
        );
        assert_close(secs(g.time), num(&w["time"]).unwrap(), 1e-9, &c);
        for (v, k) in [
            (g.lat, "lat"),
            (g.lon, "lon"),
            (g.int_qrb, "int_qrb"),
            (g.alt_diff, "alt_diff"),
            (g.eps1, "eps1"),
            (g.eps2, "eps2"),
            (g.theta1, "theta1"),
            (g.theta2, "theta2"),
            (g.angle, "angle"),
            (g.squint, "squint"),
        ] {
            assert_close(v, num(&w[k]).unwrap(), 1e-9, &format!("{c} {k}"));
        }
        match (g.int_point, &w["int_point"]) {
            (None, Value::Null) => {}
            (Some(p), o) if !o.is_null() => {
                assert_close(p.lat, num(&o["lat"]).unwrap(), 1e-9, &c);
                assert_close(p.lon, num(&o["lon"]).unwrap(), 1e-9, &c);
            }
            (a, b) => panic!("{c}: int_point {a:?} vs {b}"),
        }
    }
}

fn check_caller(got: &[PlaneInfo], want: &Value, ctx: &str) {
    for (g, w) in got.iter().zip(want.as_array().unwrap()) {
        let c = format!("{ctx} caller {}", g.hex);
        assert_close(secs(g.time), num(&w["time"]).unwrap(), 1e-9, &c);
        assert_close(g.lat, num(&w["lat"]).unwrap(), 1e-9, &c);
        assert_close(g.lon, num(&w["lon"]).unwrap(), 1e-9, &c);
    }
}

#[test]
fn crossing_golden() {
    let g = as_golden::load("nearest_planes.json");
    let (md, ma, mdl) = (10.0, 12200.0, 1000.0);
    let mut n = 0;
    for c in g.by_fn("nearest") {
        let pp = path(&c.args["path"]);
        let mut pl = planes(&c.args["planes"]);
        let r = get_nearest_planes(at0(), &pp, &mut pl, c.f("max_radius"), md, ma, mdl);
        let ctx = format!("path {} radius {}", c.args["path"], c.args["max_radius"]);
        check_out(&r, &c.result["result"], &ctx);
        check_caller(&pl, &c.result["caller_after"], &ctx);
        n += 1;
    }
    for c in g.by_fn("nearest_multi") {
        let paths = c.args["paths"].as_array().unwrap();
        let (p1, p2) = (path(&paths[0]), path(&paths[1]));
        let mut pl = planes(&c.args["planes"]);
        let r1 = get_nearest_planes(at0(), &p1, &mut pl, 0.0, md, ma, mdl);
        let r2 = get_nearest_planes(
            at0() + Duration::seconds(30),
            &p2,
            &mut pl,
            0.0,
            md,
            ma,
            mdl,
        );
        check_out(&r1, &c.result["result1"], "multi 1");
        check_out(&r2, &c.result["result2"], "multi 2");
        check_caller(&pl, &c.result["caller_after"], "multi");
        n += 1;
    }
    assert_eq!(n, 10);
}

fn p(hex: &str, potential: i32, qrb: f64, with_point: bool) -> PlaneInfo {
    let mut x = PlaneInfo::new(
        at0(),
        "C",
        "R",
        hex,
        0.0,
        0.0,
        0.0,
        30000.0,
        400.0,
        "",
        "",
        "",
        PlaneCategory::None,
    );
    x.potential = potential;
    x.int_qrb = qrb;
    x.int_point = with_point.then_some(as_geo::latlon::GPoint::new(0.0, 0.0));
    x
}

#[test]
fn nearest_plane_picks_potential_then_qrb() {
    let list = [
        p("A", 50, 3.0, true),
        p("B", 100, 8.0, true),
        p("C", 100, 6.0, true),
        p("D", 100, 12.0, true),
        p("E", 100, 6.0, true),
    ];
    assert_eq!(nearest_plane(&list, 10.0).unwrap().hex, "C");
    assert!(nearest_plane(&[p("X", 100, 10.0, true)], 10.0).is_none());
}

#[test]
fn merge_active_rules() {
    let mut active = BTreeMap::new();
    merge_active(
        &mut active,
        vec![p("A", 50, 5.0, true), p("B", 75, 5.0, true)],
    );
    // higher potential with intersection replaces; higher potential without intersection does not
    merge_active(
        &mut active,
        vec![p("A", 100, 9.0, true), p("B", 100, 1.0, false)],
    );
    assert_eq!(active["A"].potential, 100);
    assert_eq!(active["B"].potential, 75);
    // same potential and nearer replaces
    merge_active(&mut active, vec![p("B", 75, 2.0, true)]);
    assert_eq!(active["B"].int_qrb, 2.0);
}

#[test]
fn minutes_to_crossing_truncates() {
    let mut x = p("A", 100, 100.0, true);
    x.speed_kts = 400.0; // 740.8 km/h -> 8.1 min
    assert_eq!(minutes_to_crossing(&x), 8);
    x.speed_kts = 0.0;
    assert_eq!(minutes_to_crossing(&x), 0);
}
