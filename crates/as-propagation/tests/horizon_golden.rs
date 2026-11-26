mod common;
use as_elevation::{ElevationModel, LocalObstruction};
use as_geo::propagation::epsilon_from_heights;
use as_golden::{assert_close, assert_close_v, deltas::DeltaManifest, num};
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
    let mut deltas = DeltaManifest::load("propagation_horizon");
    let cases: Vec<_> = g.by_fn("horizon").collect();
    assert_eq!(cases.len(), 16);
    for (index, c) in cases.into_iter().enumerate() {
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
            let ctx = ("#3", index, az);
            assert_close_v(a.dist, &e["dist"], 1e-12, &ctx);
            let elv = e["elv"].as_i64().unwrap();
            assert_eq!(i64::from(a.elv), elv, "{ctx:?}");
            // #3: the C# winning sample remains the maximum on this terrain. Its
            // reference clearance uses wavelength times the harmonic product of
            // the two legs, with the same clamped distance used by C#.
            let p = &hor.params;
            let d = num(&e["dist"]).unwrap().max(c.f("nf") / 1000.0);
            let clearance = p.f1_clearance
                * ((0.3 / p.qrg_ghz) * (1000.0 * d) * (1.0 - d / p.dist_km).max(0.0)).sqrt();
            let reference = epsilon_from_heights(p.h, d, elv as f64 + clearance, p.radius_km);
            let old = num(&e["eps_min"]).unwrap();
            assert!(a.eps_min <= old + 1.5e-8, "{ctx:?}");
            let expected = deltas.number(
                index,
                &format!("points[{az}].eps_min"),
                &e["eps_min"],
                reference,
                1e-12,
                "#3",
            );
            assert_close(a.eps_min, expected, 1e-12, &ctx);
            if index == 0 && az == 322 {
                assert_close(a.eps_min, 0.2022434, 1e-7, &ctx);
            }
        }
    }
    deltas.finish();
}

#[test]
fn epsilon_max_from_path_matches_original() {
    let g = as_golden::load("propagation_horizon.json");
    let mut deltas = DeltaManifest::load("epsilon_max_from_path");
    let cases: Vec<_> = g
        .cases
        .iter()
        .enumerate()
        .filter(|(_, c)| c.func == "epsilon_max_from_path")
        .collect();
    assert!(cases.len() >= 30, "{}", cases.len());
    let mut cut = 0;
    for (index, c) in cases {
        let mut elv: Vec<i16> = c.args["elv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i16)
            .collect();
        let n0 = elv.len();
        // #282: the optimal point also wins without an early cut.
        let step = c.f("step") / 1000.0;
        let length = c.f("dist");
        let mut distance = 0.0;
        let mut uncut = HorizonPoint::default();
        for &height in &elv {
            let clearance = c.f("f1")
                * ((0.3 / c.f("qrg")) * 1000.0 * distance * (1.0 - distance / length)).sqrt();
            if distance > 0.1 {
                let angle = epsilon_from_heights(
                    c.f("h1"),
                    distance,
                    f64::from(height) + clearance,
                    c.f("radius"),
                );
                if angle > uncut.eps_min {
                    uncut = HorizonPoint {
                        dist: distance,
                        eps_min: angle,
                        elv: height,
                    };
                }
            }
            distance += step;
        }
        let hp = epsilon_max_from_path(
            c.f("h1"),
            &mut elv,
            length,
            c.f("step"),
            c.f("qrg"),
            c.f("f1"),
            c.f("maxelv"),
            c.f("radius"),
        );
        check_point(&hp, &c.result, c);
        assert_close(hp.eps_min, uncut.eps_min, 1e-12, &index);
        assert_eq!((hp.dist, hp.elv), (uncut.dist, uncut.elv), "case {index}");
        let expected_len = match index {
            20 => 39,
            30 => 7,
            32 => 62,
            48 | 54 => 9,
            50 | 56 => 86,
            _ => c.result["len"].as_i64().unwrap(),
        };
        let expected = deltas.expected(
            index,
            "len",
            &c.result["len"],
            expected_len.into(),
            0.0,
            "#282",
        );
        assert_eq!(
            Some(elv.len() as i64),
            expected.as_i64(),
            "#282 case {index}"
        );
        if elv.len() < n0 {
            cut += 1;
        }
    }
    deltas.finish();
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
    engine.set_near_field_suppression_m(100.0);
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

    // #354: changing the setting must match an uncached run at the new value.
    let model = ElevationModel::Globe;
    engine.set_near_field_suppression_m(500.0);
    let uncached = engine
        .create_horizon(model, &p, None, false, None, &|_, _| {})
        .unwrap()
        .unwrap();
    let second = engine
        .find_or_create_horizon(model, &p, None, true, None, &|_, _| {})
        .unwrap()
        .unwrap();
    assert_ne!(a.horizon, uncached.horizon);
    assert_eq!(second.horizon, uncached.horizon);
    assert_eq!(engine.find_horizon(model, &p, None).unwrap(), Some(second));

    // A setting changed during a parallel run only affects the next horizon.
    engine.set_near_field_suppression_m(100.0);
    let updating = engine
        .create_horizon(model, &p, None, true, None, &|_, _| {
            engine.set_near_field_suppression_m(500.0);
        })
        .unwrap()
        .unwrap();
    assert_eq!(updating.horizon, a.horizon);
    engine.set_near_field_suppression_m(100.0);
    assert_eq!(
        engine.find_horizon(model, &p, None).unwrap(),
        Some(updating)
    );
}

#[test]
fn finding_11_local_obstruction_is_applied_at_create_and_load() {
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
    assert!(hor.local_obstructed);
    let found = engine
        .find_horizon(ElevationModel::Globe, &p, Some(&lo))
        .unwrap()
        .unwrap();
    assert!(found.local_obstructed);
    assert!(
        !engine
            .find_horizon(ElevationModel::Globe, &p, None)
            .unwrap()
            .unwrap()
            .local_obstructed
    );
}
