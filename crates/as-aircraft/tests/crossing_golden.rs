//! Golden test: crossing prediction `GetNearestPlanes` against C# (`tests/golden/nearest_planes.json`).
use as_aircraft::crossing::{get_nearest_planes, merge_active, minutes_to_crossing, nearest_plane};
use as_aircraft::{AltitudeProvenance, PlaneCategory, PlaneInfo};
use as_golden::{deltas::DeltaManifest, num};
use as_propagation::{PathParams, PropagationPath};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Golden comparison tolerance: well below any documented fix effect (none of the
/// `nearest_planes` delta rows is within 1e-6 of its C# value) and above cross-target
/// float noise.
const TOL: f64 = 1e-6;

fn at0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2023, 11, 14, 22, 0, 0).unwrap()
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

#[test]
fn finding_364_crossing_preserves_pressure_estimate_provenance() {
    let mut plane = PlaneInfo::new(
        at0(),
        "TST123",
        "",
        "3C0001",
        50.0,
        0.0,
        90.0,
        35000.0,
        450.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    );
    plane.altitude_provenance = AltitudeProvenance::PressureEstimate;
    let path = PropagationPath::new(
        PathParams {
            lat1: 50.0,
            lon1: -1.0,
            h1: 10.0,
            lat2: 50.0,
            lon2: 1.0,
            h2: 10.0,
            qrg_ghz: 0.144,
            radius_km: 6371.0 * 1.33,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: 0.0,
        },
        0.0,
        0.0,
        true,
    );
    let result = get_nearest_planes(at0(), &path, &[plane], -1.0, 50.0, 20_000.0, 1_000.0);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].altitude_provenance,
        AltitudeProvenance::PressureEstimate
    );
    assert!(result[0].altitude_estimate());
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

fn near_plane_json(g: &PlaneInfo) -> Value {
    let int_point = g
        .int_point
        .map(|p| json!({ "lat": p.lat, "lon": p.lon }))
        .unwrap_or(Value::Null);
    json!({
        "hex": g.hex,
        "potential": g.potential,
        "time": (g.time - at0()).num_milliseconds() as f64 / 1000.0,
        "lat": g.lat,
        "lon": g.lon,
        "int_point": int_point,
        "int_qrb": g.int_qrb,
        "alt_diff": g.alt_diff,
        "eps1": g.eps1,
        "eps2": g.eps2,
        "theta1": g.theta1,
        "theta2": g.theta2,
        "angle": g.angle,
        "squint": g.squint
    })
}

fn caller_plane_json(g: &PlaneInfo) -> Value {
    json!({
        "hex": g.hex,
        "time": (g.time - at0()).num_milliseconds() as f64 / 1000.0,
        "lat": g.lat,
        "lon": g.lon
    })
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
fn crossing_golden() {
    let g = as_golden::load("nearest_planes.json");
    let mut deltas =
        DeltaManifest::load_from("docs/logic-fixes-area2/golden-deltas.tsv", "nearest_planes");
    let (md, ma, mdl) = (10.0, 12200.0, 1000.0);
    for (index, c) in g.cases.iter().enumerate() {
        let got_result = if c.func == "nearest" {
            let pp = path(&c.args["path"]);
            let pl = planes(&c.args["planes"]);
            let result = get_nearest_planes(at0(), &pp, &pl, c.f("max_radius"), md, ma, mdl);
            json!({
                "result": result.iter().map(near_plane_json).collect::<Vec<_>>(),
                "caller_after": pl.iter().map(caller_plane_json).collect::<Vec<_>>(),
            })
        } else {
            let paths = c.args["paths"].as_array().unwrap();
            let (p1, p2) = (path(&paths[0]), path(&paths[1]));
            let pl = planes(&c.args["planes"]);
            let result1 = get_nearest_planes(at0(), &p1, &pl, 0.0, md, ma, mdl);
            let result2 =
                get_nearest_planes(at0() + Duration::seconds(30), &p2, &pl, 0.0, md, ma, mdl);
            json!({
                "result1": result1.iter().map(near_plane_json).collect::<Vec<_>>(),
                "result2": result2.iter().map(near_plane_json).collect::<Vec<_>>(),
                "caller_after": pl.iter().map(caller_plane_json).collect::<Vec<_>>(),
            })
        };
        let expected = deltas.expected(
            index,
            "result",
            &c.result,
            got_result.clone(),
            // libm differs in the last ulps across targets; alt_diff (~1 km) differs by
            // ~4e-9 m between aarch64 macOS and x86_64 Linux
            TOL,
            "#25/#26/#357/#358/#368/#82",
        );
        assert!(equivalent(&got_result, &expected, TOL), "case {index}");
    }
    deltas.finish();
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
    assert_eq!(
        nearest_plane(&[p("X", 100, 10.0, true)], 10.0).unwrap().hex,
        "X"
    );
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
    x.forward_qrb = Some(100.0);
    x.speed_kts = 400.0; // 740.8 km/h -> 8.1 min
    assert_eq!(minutes_to_crossing(&x), Some(8));
    x.speed_kts = 0.0;
    assert_eq!(minutes_to_crossing(&x), None);
}

#[test]
fn finding_25_near_branch_uses_negative_tolerance() {
    let path = PropagationPath::new(
        PathParams {
            lat1: 0.0,
            lon1: 0.0,
            h1: 0.0,
            lat2: 0.0,
            lon2: 2.0,
            h2: 0.0,
            qrg_ghz: 0.144,
            radius_km: 6371.0 * 1.33,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: 0.0,
        },
        0.0,
        0.0,
        true,
    );
    let mk = |hex: &str, alt_m: f64| {
        PlaneInfo::new(
            at0(),
            hex,
            "",
            hex,
            0.0,
            1.0,
            0.0,
            alt_m / 0.3048,
            300.0,
            "",
            "",
            "",
            PlaneCategory::Medium,
        )
    };
    let near = get_nearest_planes(
        at0(),
        &path,
        &[mk("A", -270.0), mk("B", -271.0)],
        1000.0,
        10.0,
        12200.0,
        1000.0,
    );
    let a = near.iter().find(|p| p.hex == "A").unwrap();
    let b = near.iter().find(|p| p.hex == "B").unwrap();
    assert_eq!(a.potential, 50, "{a:?}");
    assert_eq!(b.potential, 0, "{b:?}");
}

#[test]
fn finding_26_approaching_sets_theta_from_forward_crossing() {
    let path = PropagationPath::new(
        PathParams {
            lat1: 0.0,
            lon1: 0.0,
            h1: 100.0,
            lat2: 0.0,
            lon2: 2.0,
            h2: 100.0,
            qrg_ghz: 0.144,
            radius_km: 6371.0 * 1.33,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: 0.0,
        },
        0.0,
        0.0,
        true,
    );
    let plane = PlaneInfo::new(
        at0(),
        "A",
        "",
        "AAAAAA",
        1.0,
        1.99,
        180.0,
        40_000.0,
        450.0,
        "",
        "",
        "",
        PlaneCategory::Medium,
    );
    let out = get_nearest_planes(at0(), &path, &[plane], 500.0, 10.0, 12_200.0, 1_000.0);
    let p = &out[0];
    assert_eq!(p.potential, 75, "{p:?}");
    assert!(p.theta1.abs() > 1e-12, "{p:?}");
    assert!(p.theta2.abs() > 1e-12, "{p:?}");
    assert!(
        (p.squint - (p.theta1 - p.theta2).abs()).abs() < 1e-12,
        "{p:?}"
    );
}

#[test]
fn finding_24_crossing_uses_analysis_snapshot_without_mutating_inputs() {
    let path = PropagationPath::new(
        PathParams {
            lat1: 0.0,
            lon1: 0.0,
            h1: 100.0,
            lat2: 0.0,
            lon2: 2.0,
            h2: 100.0,
            qrg_ghz: 0.144,
            radius_km: 6371.0 * 1.33,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: 0.0,
        },
        0.0,
        0.0,
        true,
    );
    let at = at0();
    let mut aligned = PlaneInfo::new(
        at,
        "A",
        "",
        "AAAAAA",
        0.0,
        0.0,
        90.0,
        30000.0,
        200.0,
        "",
        "",
        "",
        PlaneCategory::Medium,
    );
    let future = PlaneInfo::new(
        at + Duration::seconds(1),
        "F",
        "",
        "FFFFFF",
        0.0,
        1.0,
        90.0,
        30000.0,
        200.0,
        "",
        "",
        "",
        PlaneCategory::Medium,
    );
    let moving_in = PlaneInfo::new(
        at - Duration::minutes(30),
        "M",
        "",
        "MMMMMM",
        0.0,
        -2.0,
        90.0,
        30000.0,
        240.0,
        "",
        "",
        "",
        PlaneCategory::Medium,
    );
    let input = vec![aligned.clone(), future, moving_in];
    let before = input.clone();
    let out = get_nearest_planes(at, &path, &input, 120.0, 1000.0, 12200.0, 1000.0);
    assert_eq!(input, before, "caller input must stay unchanged");
    assert!(
        out.iter().all(|p| p.hex != "FFFFFF"),
        "future report rejected"
    );
    aligned.time = at;
    let got = out
        .iter()
        .find(|p| p.hex == "AAAAAA")
        .expect("analysis-time plane kept");
    assert_eq!(got.time, at);
    assert!((got.lat - aligned.lat).abs() < 1e-9 && (got.lon - aligned.lon).abs() < 1e-9);
    assert!(
        out.iter().any(|p| p.hex == "MMMMMM"),
        "plane moving into radius must be evaluated"
    );
}
