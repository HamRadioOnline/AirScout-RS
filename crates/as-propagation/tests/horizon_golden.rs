mod common;
use as_elevation::{ElevationModel, LocalObstruction};
use as_golden::assert_close_v;
use as_propagation::{epsilon_max_from_path, HorizonParams, HorizonPoint, PropagationEngine};
use std::sync::atomic::{AtomicUsize, Ordering};

fn engine(dir: &std::path::Path) -> PropagationEngine {
    PropagationEngine::open(common::fixture_store(dir), dir).unwrap()
}

fn hparams(c: &as_golden::Case) -> HorizonParams {
    HorizonParams {
        lat: c.f("lat"),
        lon: c.f("lon"),
        h: c.f("h"),
        dist_km: c.f("dist"),
        qrg_ghz: c.f("qrg"),
        radius_km: c.f("radius"),
        f1_clearance: c.f("f1"),
        step_width_m: c.f("step"),
    }
}

fn check_point(a: &HorizonPoint, e: &serde_json::Value, ctx: &dyn std::fmt::Debug) {
    assert_close_v(a.dist, &e["dist"], 1e-12, ctx);
    assert_close_v(a.eps_min, &e["eps_min"], 1e-12, ctx);
    assert_eq!(a.elv as i64, e["elv"].as_i64().unwrap(), "{ctx:?}");
}

#[test]
fn horizon_matches_original() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    let g = as_golden::load("propagation_horizon.json");
    let cases: Vec<_> = g.by_fn("horizon").collect();
    assert_eq!(cases.len(), 16);
    for c in cases {
        engine.set_near_field_suppression_m(c.f("nf"));
        let calls = AtomicUsize::new(0);
        let hor = engine
            .create_horizon(
                ElevationModel::Globe,
                &hparams(c),
                None,
                false,
                None,
                &|_, _| {
                    calls.fetch_add(1, Ordering::Relaxed);
                },
            )
            .unwrap()
            .expect("not cancelled");
        assert_eq!(calls.load(Ordering::Relaxed), 360);
        assert_eq!(hor.valid, c.result["valid"].as_bool().unwrap(), "{c:?}");
        let exp = c.result["points"].as_array().unwrap();
        assert_eq!(exp.len(), 360);
        for (az, (a, e)) in hor.horizon.iter().zip(exp).enumerate() {
            check_point(a, e, &(az, c));
        }
    }
}

#[test]
fn epsilon_max_from_path_matches_original() {
    let g = as_golden::load("propagation_horizon.json");
    let cases: Vec<_> = g.by_fn("epsilon_max_from_path").collect();
    assert!(cases.len() >= 30, "{}", cases.len());
    let mut cut = 0;
    for c in cases {
        let mut elv: Vec<i16> = c.args["elv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i16)
            .collect();
        let n0 = elv.len();
        let hp = epsilon_max_from_path(
            c.f("h1"),
            &mut elv,
            c.f("dist"),
            c.f("step"),
            c.f("qrg"),
            c.f("f1"),
            c.f("maxelv"),
            c.f("radius"),
        );
        check_point(&hp, &c.result, c);
        assert_eq!(elv.len() as i64, c.result["len"].as_i64().unwrap(), "{c:?}");
        if elv.len() < n0 {
            cut += 1;
        }
    }
    assert!(cut > 0, "no case exercised the max-elevation cut");
}

#[test]
fn find_or_create_horizon_uses_cache() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    let p = HorizonParams {
        lat: 51.0013,
        lon: 11.5027,
        h: 10.0,
        dist_km: 50.0,
        qrg_ghz: 0.144,
        radius_km: 6371.0 * 1.33,
        f1_clearance: 0.6,
        step_width_m: 1000.0,
    };
    let a = engine
        .find_or_create_horizon(ElevationModel::Globe, &p, None, true, None, &|_, _| {})
        .unwrap()
        .unwrap();
    assert!(a.valid);
    let b = engine
        .find_horizon(ElevationModel::Globe, &p, None)
        .unwrap()
        .expect("cached");
    assert_eq!(a.horizon, b.horizon);
    assert!(b.valid);
}

/// logic-findings #11: the C# loop `for (i = 0; i > 360; i++)` never runs, so a horizon is never
/// flagged as locally obstructed, even when every azimuth is below the obstruction.
#[test]
fn local_obstructed_is_never_set() {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine(dir.path());
    let p = HorizonParams {
        lat: 51.0013,
        lon: 11.5027,
        h: 10.0,
        dist_km: 50.0,
        qrg_ghz: 0.144,
        radius_km: 6371.0 * 1.33,
        f1_clearance: 0.6,
        step_width_m: 1000.0,
    };
    let lo = LocalObstruction::new(p.lat, p.lon, Some([0.01; 360]), Some([5000.0; 360]));
    let hor = engine
        .create_horizon(ElevationModel::Globe, &p, Some(&lo), true, None, &|_, _| {})
        .unwrap()
        .unwrap();
    assert!(hor
        .horizon
        .iter()
        .all(|x| x.eps_min < lo.obstruction(p.h, 0.0)));
    assert!(!hor.local_obstructed);
    let found = engine
        .find_horizon(ElevationModel::Globe, &p, Some(&lo))
        .unwrap()
        .unwrap();
    assert!(!found.local_obstructed);
}
