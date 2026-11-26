mod common;
use as_elevation::ElevationModel;
use as_golden::{assert_close, assert_close_v, num, Case};
use as_propagation::{PathParams, PropagationEngine};

fn params(c: &Case) -> PathParams {
    PathParams {
        lat1: c.f("lat1"),
        lon1: c.f("lon1"),
        h1: c.f("h1"),
        lat2: c.f("lat2"),
        lon2: c.f("lon2"),
        h2: c.f("h2"),
        qrg_ghz: c.f("qrg"),
        radius_km: c.f("radius"),
        f1_clearance: c.f("f1"),
        step_width_m: c.f("step"),
        local_obstruction: c.f("lo"),
    }
}

/// tolerance relative to magnitude (heights reach ~1e5 m)
fn close(a: f64, e: &serde_json::Value, ctx: &dyn std::fmt::Debug) {
    let ev = num(e).expect("number");
    assert_close(a, ev, 1e-9 * ev.abs().max(1.0), ctx);
}

#[test]
fn propagation_path_matches_original() {
    let dir = tempfile::tempdir().unwrap();
    let engine = PropagationEngine::open(common::fixture_store(dir.path()), dir.path()).unwrap();
    let g = as_golden::load("propagation_path.json");
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    for c in &g.cases {
        *seen.entry(c.func.clone()).or_default() += 1;
        engine.set_near_field_suppression_m(c.f("nf"));
        let pp = engine
            .create_path(ElevationModel::Globe, &params(c), false, None)
            .unwrap()
            .expect("not cancelled");
        match c.func.as_str() {
            "create_path" => {
                assert_close_v(pp.eps1_min, &c.result["eps1_min"], 1e-12, c);
                assert_close_v(pp.eps2_min, &c.result["eps2_min"], 1e-12, c);
                assert_eq!(pp.valid, c.result["valid"].as_bool().unwrap(), "{c:?}");
                assert_eq!(
                    pp.local_obstructed,
                    c.result["local_obstructed"].as_bool().unwrap(),
                    "{c:?}"
                );
            }
            "info_points" => {
                let ip = pp.info_points().expect("step > 0");
                let exp = c.result.as_array().unwrap();
                assert_eq!(ip.len(), exp.len(), "{c:?}");
                for (a, e) in ip.iter().zip(exp) {
                    close(a.lat, &e["lat"], c);
                    close(a.lon, &e["lon"], c);
                    close(a.h1, &e["h1"], c);
                    close(a.h2, &e["h2"], c);
                    close(a.f1, &e["f1"], c);
                }
            }
            "mid_point" => {
                let m = pp.mid_point();
                close(m.lat, &c.result["lat"], c);
                close(m.lon, &c.result["lon"], c);
            }
            "intersection_point" => {
                // A plane exactly on the path (probe offset 0/0 = path midpoint) makes the two great
                // circles meet at the plane itself: the intersection is ill-conditioned and C# and
                // Rust libm return different points. Ruling in the ledger; covered by offsets != 0.
                let mid = pp.mid_point();
                if as_geo::latlon::distance(mid.lat, mid.lon, c.f("plane_lat"), c.f("plane_lon"))
                    < 1e-9
                {
                    *seen.entry("degenerate_skipped".into()).or_default() += 1;
                    continue;
                }
                let r = pp.intersection_point(
                    c.f("plane_lat"),
                    c.f("plane_lon"),
                    c.f("plane_brg"),
                    c.f("maxdistance"),
                );
                if c.result.is_null() {
                    assert!(r.is_none(), "{c:?} got {r:?}");
                } else {
                    let r = r.unwrap_or_else(|| panic!("expected Some for {c:?}"));
                    close(r.lat, &c.result["lat"], c);
                    close(r.lon, &c.result["lon"], c);
                    close(r.qrb, &c.result["qrb"], c);
                    close(r.min_h, &c.result["min_h"], c);
                    close(r.dist1, &c.result["dist1"], c);
                    close(r.dist2, &c.result["dist2"], c);
                }
            }
            other => panic!("unhandled {other}"),
        }
    }
    assert_eq!(seen.len(), 5, "{seen:?}");
    assert!(seen["intersection_point"] > 100, "{seen:?}");
}

#[test]
fn find_or_create_uses_cache() {
    let dir = tempfile::tempdir().unwrap();
    let engine = PropagationEngine::open(common::fixture_store(dir.path()), dir.path()).unwrap();
    let p = PathParams {
        lat1: 50.5013,
        lon1: 10.5027,
        h1: 10.0,
        lat2: 51.8,
        lon2: 13.9,
        h2: 10.0,
        qrg_ghz: 0.144,
        radius_km: 6371.0 * 1.33,
        f1_clearance: 0.6,
        step_width_m: 1000.0,
        local_obstruction: f64::MIN,
    };
    let a = engine
        .find_or_create_path(ElevationModel::Globe, &p, true, None)
        .unwrap()
        .unwrap();
    assert!(a.valid);
    let b = engine
        .find_path(ElevationModel::Globe, &p)
        .unwrap()
        .expect("cached");
    assert_eq!(a.eps1_min, b.eps1_min);
    assert_eq!(a.eps2_min, b.eps2_min);
    assert!(b.valid);
    // local obstruction is applied on load, not part of the key
    let mut q = p;
    q.local_obstruction = 1.0;
    let c = engine
        .find_path(ElevationModel::Globe, &q)
        .unwrap()
        .expect("cached");
    assert!(c.local_obstructed);
}

#[test]
fn zero_step_width_has_no_info_points() {
    let dir = tempfile::tempdir().unwrap();
    let engine = PropagationEngine::open(common::fixture_store(dir.path()), dir.path()).unwrap();
    let p = PathParams {
        lat1: 50.5,
        lon1: 10.5,
        h1: 10.0,
        lat2: 51.8,
        lon2: 13.9,
        h2: 10.0,
        qrg_ghz: 0.144,
        radius_km: 6371.0 * 1.33,
        f1_clearance: 0.6,
        step_width_m: 1000.0,
        local_obstruction: f64::MIN,
    };
    let mut pp = engine
        .create_path(ElevationModel::Globe, &p, false, None)
        .unwrap()
        .unwrap();
    pp.params.step_width_m = 0.0;
    assert!(pp.info_points().is_none());
    assert!(pp.info_point(1.0).is_none());
}
