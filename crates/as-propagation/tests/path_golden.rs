mod common;
use as_elevation::ElevationModel;
use as_geo::{
    latlon,
    propagation::{epsilon_from_heights, height_from_epsilon},
};
use as_golden::{assert_close, deltas::DeltaManifest, num, Case};
use as_propagation::{PathParams, PropagationEngine};
use serde_json::json;

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

// ITU-R P.526 first Fresnel zone, arranged using the fractional distance to the
// obstacle and the wavelength. At a terminal the radius is zero.
fn fresnel(freq: f64, length: f64, from_first: f64) -> f64 {
    if length == 0.0 {
        return 0.0;
    }
    let fraction = from_first / length;
    ((0.3 / freq) * (1000.0 * from_first) * (1.0 - fraction)).sqrt()
}

// #349/#350: derive the two Fresnel legs from the reference sample positions.
fn profile_angles(
    terrain: impl IntoIterator<Item = i16>,
    p: &PathParams,
    length: f64,
    near_field: f64,
) -> (f64, f64) {
    let mut angles = (f64::MIN, f64::MIN);
    for (i, terrain) in terrain.into_iter().enumerate() {
        let from_first = i as f64 * p.step_width_m / 1000.0;
        let from_second = (length - from_first).max(0.0);
        let height = f64::from(terrain) + p.f1_clearance * fresnel(p.qrg_ghz, length, from_first);
        angles.0 = angles.0.max(epsilon_from_heights(
            p.h1,
            from_first.max(near_field),
            height,
            p.radius_km,
        ));
        angles.1 = angles.1.max(epsilon_from_heights(
            p.h2,
            from_second.max(near_field),
            height,
            p.radius_km,
        ));
    }
    angles
}

#[test]
fn propagation_path_matches_original() {
    let dir = tempfile::tempdir().unwrap();
    let engine = PropagationEngine::open(common::fixture_store(dir.path()), dir.path()).unwrap();
    let g = as_golden::load("propagation_path.json");
    let elevation = as_golden::load("elevation_path.json");
    let mut deltas = DeltaManifest::load("propagation_path");
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    for (index, c) in g.cases.iter().enumerate() {
        *seen.entry(c.func.clone()).or_default() += 1;
        engine.set_near_field_suppression_m(c.f("nf"));
        let params = params(c);
        let p = &params;
        let pp = engine
            .create_path(ElevationModel::Globe, p, false, None)
            .unwrap()
            .expect("not cancelled");
        // These same transects were captured independently by the C# elevation
        // harness. Use those heights as input to the corrected geometry.
        let profile = elevation
            .by_fn("create_path")
            .find(|ep| {
                ep.args["model"] == "GLOBE"
                    && ep.b("complete")
                    && ["lat1", "lon1", "lat2", "lon2", "step"]
                        .into_iter()
                        .all(|field| ep.args[field] == c.args[field])
            })
            .expect("C# terrain for propagation golden");
        let terrain = profile.result["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i16);
        let length = latlon::distance(p.lat1, p.lon1, p.lat2, p.lon2);
        let (eps1, eps2) = profile_angles(terrain, p, length, c.f("nf") / 1000.0);
        let ep1_effective = eps1.max(p.local_obstruction);
        match c.func.as_str() {
            "create_path" => {
                let finding = if length == 0.0 { "#350" } else { "#349" };
                for (field, actual, reference) in [
                    ("eps1_min", pp.eps1_min, eps1),
                    ("eps2_min", pp.eps2_min, eps2),
                ] {
                    let expected =
                        deltas.number(index, field, &c.result[field], reference, 1e-12, finding);
                    assert_close(actual, expected, 1e-12, &(index, field));
                }
                assert_eq!(pp.valid, c.result["valid"].as_bool().unwrap(), "{c:?}");
                let expected_flag = deltas.expected(
                    index,
                    "local_obstructed",
                    &c.result["local_obstructed"],
                    json!(eps1 < p.local_obstruction),
                    0.0,
                    finding,
                );
                assert_eq!(Some(pp.local_obstructed), expected_flag.as_bool(), "{c:?}");
            }
            "info_points" => {
                let ip = pp.info_points().expect("step > 0");
                let exp = c.result.as_array().unwrap();
                assert_eq!(ip.len(), exp.len(), "{c:?}");
                for (i, (a, e)) in ip.iter().zip(exp).enumerate() {
                    let d = (i as f64).min(length);
                    let terminal = i + 1 == exp.len();
                    if terminal {
                        for (field, actual, station) in
                            [("lat", a.lat, p.lat2), ("lon", a.lon, p.lon2)]
                        {
                            let expected = deltas.number(
                                index,
                                &format!("[{i}].{field}"),
                                &e[field],
                                station,
                                1e-9,
                                "#352",
                            );
                            assert_close(actual, expected, 1e-9, &(index, i, field));
                        }
                    } else {
                        close(a.lat, &e["lat"], c);
                        close(a.lon, &e["lon"], c);
                    }
                    let h1 = height_from_epsilon(p.h1, d, ep1_effective, p.radius_km);
                    let h2 = if d == length {
                        p.h2
                    } else {
                        height_from_epsilon(p.h2, length - d, eps2, p.radius_km)
                    };
                    let heights_finding = match (length == 0.0, terminal) {
                        (true, _) => "#350/#352",
                        (false, true) => "#349/#352",
                        (false, false) => "#349",
                    };
                    let fresnel_finding = if length == 0.0 {
                        "#350"
                    } else if terminal {
                        "#1/#352"
                    } else {
                        "#1"
                    };
                    for (field, actual, reference, tol, finding) in [
                        ("h1", a.h1, h1, 1e-9 * h1.abs().max(1.0), heights_finding),
                        ("h2", a.h2, h2, 1e-9 * h2.abs().max(1.0), heights_finding),
                        (
                            "f1",
                            a.f1,
                            fresnel(p.qrg_ghz, length, d),
                            1e-9,
                            fresnel_finding,
                        ),
                    ] {
                        let expected = deltas.number(
                            index,
                            &format!("[{i}].{field}"),
                            &e[field],
                            reference,
                            tol,
                            finding,
                        );
                        assert_close(actual, expected, tol, &(index, i, field));
                    }
                }
                if index == 9 {
                    assert_close(ip[100].f1, 369.925, 0.001, c);
                    assert_eq!(ip.last().unwrap().h2, 10.0);
                }
            }
            "mid_point" => {
                let m = pp.mid_point();
                close(m.lat, &c.result["lat"], c);
                close(m.lon, &c.result["lon"], c);
            }
            "intersection_point" => {
                let plane = [c.f("plane_lat"), c.f("plane_lon"), c.f("plane_brg")];
                let max_distance = c.f("maxdistance");
                // #2: each group lists the unlimited crossing before the two limits.
                let raw = &g.cases
                    [index - (max_distance >= 0.0) as usize - (max_distance > 0.0) as usize]
                    .result;
                let result = pp.intersection_point(plane[0], plane[1], plane[2], max_distance);
                let on_path = {
                    let mid = latlon::midpoint(p.lat1, p.lon1, p.lat2, p.lon2);
                    latlon::distance(mid.lat, mid.lon, plane[0], plane[1]) < 1e-9
                };
                let on_meridian = plane[1] == p.lon1;
                let affected_by_344 = (on_path || on_meridian) && length > 0.0;
                let point = if affected_by_344 {
                    as_golden::great_circle::directed_intersection(
                        [
                            p.lat1,
                            p.lon1,
                            latlon::bearing(p.lat1, p.lon1, p.lat2, p.lon2),
                        ],
                        plane,
                    )
                } else if raw.is_null() {
                    None
                } else {
                    Some((num(&raw["lat"]).unwrap(), num(&raw["lon"]).unwrap()))
                };
                let expected = point.filter(|&(lat, lon)| {
                    let qrb = latlon::distance(plane[0], plane[1], lat, lon);
                    if max_distance == 0.0 {
                        qrb <= length / 2.0 || qrb.is_nan()
                    } else if max_distance > 0.0 {
                        qrb <= max_distance || qrb.is_nan()
                    } else {
                        true
                    }
                });
                let geometry_finding = match (affected_by_344, max_distance > 0.0) {
                    (true, true) => "#344/#2",
                    (true, false) => "#344",
                    (false, true) => "#2",
                    (false, false) => "",
                };
                if let Some((lat, lon)) = expected {
                    let r = result.unwrap_or_else(|| panic!("case {index}: missing crossing"));
                    let (dist1, dist2, qrb) = if affected_by_344 {
                        (
                            latlon::distance(p.lat1, p.lon1, lat, lon),
                            latlon::distance(p.lat2, p.lon2, lat, lon),
                            latlon::distance(plane[0], plane[1], lat, lon),
                        )
                    } else {
                        (
                            num(&raw["dist1"]).unwrap(),
                            num(&raw["dist2"]).unwrap(),
                            num(&raw["qrb"]).unwrap(),
                        )
                    };
                    for (field, actual, reference) in [
                        ("lat", r.lat, lat),
                        ("lon", r.lon, lon),
                        ("dist1", r.dist1, dist1),
                        ("dist2", r.dist2, dist2),
                        ("qrb", r.qrb, qrb),
                    ] {
                        let tol = if affected_by_344 {
                            if matches!(field, "lat" | "lon") {
                                1e-7
                            } else {
                                1e-5
                            }
                        } else {
                            1e-9 * reference.abs().max(1.0)
                        };
                        let expected = deltas.number(
                            index,
                            field,
                            &c.result[field],
                            reference,
                            tol,
                            geometry_finding,
                        );
                        assert_close(actual, expected, tol, &(index, field));
                    }
                    let min_h = height_from_epsilon(p.h1, dist1, ep1_effective, p.radius_km)
                        .max(height_from_epsilon(p.h2, dist2, eps2, p.radius_km));
                    let tol = if affected_by_344 { 1e-6 } else { 1e-9 } * min_h.abs().max(1.0);
                    let height_finding = match geometry_finding {
                        "#344/#2" => "#349/#344/#2",
                        "#344" => "#349/#344",
                        "#2" if c.result.is_null() => "#349/#2",
                        _ => "#349",
                    };
                    let expected = deltas.number(
                        index,
                        "min_h",
                        &c.result["min_h"],
                        min_h,
                        tol,
                        height_finding,
                    );
                    assert_close(r.min_h, expected, tol, &(index, "min_h"));
                } else {
                    let expected = deltas.expected(
                        index,
                        "result",
                        &c.result,
                        serde_json::Value::Null,
                        0.0,
                        geometry_finding,
                    );
                    assert!(
                        result.is_none() && expected.is_null(),
                        "case {index}: {result:?}"
                    );
                }
            }
            other => panic!("unhandled {other}"),
        }
    }
    deltas.finish();
    assert_eq!(seen.len(), 4, "{seen:?}");
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
    engine.set_near_field_suppression_m(100.0);
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
    q.local_obstruction = std::f64::consts::FRAC_PI_2;
    let c = engine
        .find_path(ElevationModel::Globe, &q)
        .unwrap()
        .expect("cached");
    assert!(c.local_obstructed);

    // #354: the near-field setting belongs to the cache key.
    let model = ElevationModel::Globe;
    engine.set_near_field_suppression_m(500.0);
    let uncached = engine.create_path(model, &p, false, None).unwrap().unwrap();
    let second = engine
        .find_or_create_path(model, &p, true, None)
        .unwrap()
        .unwrap();
    assert_ne!(a.eps1_min, uncached.eps1_min);
    assert_eq!(
        (second.eps1_min, second.eps2_min),
        (uncached.eps1_min, uncached.eps2_min)
    );
    assert_eq!(engine.find_path(model, &p).unwrap(), Some(second));
    engine.set_near_field_suppression_m(100.0);
    assert_eq!(engine.find_path(model, &p).unwrap(), Some(a));
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

#[test]
fn finding_1_info_point_fresnel_uses_two_legs() {
    let g = as_golden::load("propagation_path.json");
    let p = as_propagation::PropagationPath::new(params(&g.cases[9]), 0.1, 0.1, true);
    assert_eq!(p.info_point(0.0).unwrap().f1, 0.0);
    assert_eq!(p.info_point(p.distance()).unwrap().f1, 0.0);
    assert_close(p.info_point(100.0).unwrap().f1, 369.925, 0.001, &p.params);
}

#[test]
fn finding_349_short_path_keeps_its_station_two_leg() {
    let dir = tempfile::tempdir().unwrap();
    let engine = PropagationEngine::open(common::fixture_store(dir.path()), dir.path()).unwrap();
    let g = as_golden::load("propagation_path.json");
    let c = &g.cases[1232];
    let pp = engine
        .create_path(ElevationModel::Globe, &params(c), false, None)
        .unwrap()
        .unwrap();
    assert_close(pp.eps2_min, 1.3690, 1e-4, c);
}

#[test]
fn finding_351_half_step_samples_a_one_cell_ridge() {
    use as_elevation::{ElevationStore, ElevationTile};
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(ElevationStore::open(dir.path()).unwrap());
    let model = ElevationModel::Globe;
    let mut terrain = vec![0; 50];
    terrain[5..10].fill(100);
    let tile = ElevationTile::new(
        "JO51AA",
        0,
        51.01,
        10.005,
        100,
        51.01,
        10.01,
        5,
        10,
        terrain,
        chrono::DateTime::UNIX_EPOCH,
    )
    .unwrap();
    store.insert_or_update_if_newer(model, &tile).unwrap();
    let engine = PropagationEngine::open(store, dir.path()).unwrap();
    engine.set_near_field_suppression_m(100.0);
    let end = latlon::destination_point(51.01, 10.005, 90.0, 3.0);
    let mut p = PathParams {
        lat1: 51.01,
        lon1: 10.005,
        h1: 10.0,
        lat2: end.lat,
        lon2: end.lon,
        h2: 10.0,
        qrg_ghz: 0.144,
        radius_km: 8473.43,
        f1_clearance: 0.0,
        step_width_m: 1000.0,
        local_obstruction: f64::MIN,
    };
    let coarse = engine
        .create_path(model, &p, false, None)
        .unwrap()
        .unwrap()
        .eps1_min;
    p.step_width_m = model.default_step_width_m();
    let half = engine
        .create_path(model, &p, false, None)
        .unwrap()
        .unwrap()
        .eps1_min;
    // Dense reference visits the synthetic ridge at 50 m intervals.
    p.step_width_m = 50.0;
    let dense_profile = engine
        .elevation()
        .create_path(
            model,
            p.lat1,
            p.lon1,
            p.lat2,
            p.lon2,
            p.step_width_m,
            false,
            None,
        )
        .unwrap()
        .unwrap();
    let length = dense_profile.distance();
    let dense = profile_angles(dense_profile.path, &p, length, 0.1).0;
    assert!(
        coarse < 0.0 && half > 0.0 && half <= dense,
        "{coarse} {half} {dense}"
    );
}

#[test]
fn finding_352_profile_ends_at_the_receiving_station() {
    let g = as_golden::load("propagation_path.json");
    let p = as_propagation::PropagationPath::new(params(&g.cases[9]), 0.1, 0.1, true);
    let points = p.info_points().unwrap();
    assert_eq!(points.len(), p.distance() as usize + 2);
    let end = points.last().unwrap();
    assert_close(end.lat, p.params.lat2, 1e-9, &p.params);
    assert_close(end.lon, p.params.lon2, 1e-9, &p.params);
    assert_eq!((end.h2, end.f1), (10.0, 0.0));
}

#[test]
fn finding_2_positive_distance_is_a_maximum() {
    let g = as_golden::load("propagation_path.json");
    for (i, want) in [(25, None), (644, Some(10.412777184929238))] {
        let c = &g.cases[i];
        let p = as_propagation::PropagationPath::new(params(c), 0.1, 0.1, true);
        let crossing =
            p.intersection_point(c.f("plane_lat"), c.f("plane_lon"), c.f("plane_brg"), 50.0);
        assert_eq!(crossing.is_some(), want.is_some(), "case {i}: {crossing:?}");
        if let Some(qrb) = want {
            assert_close(crossing.unwrap().qrb, qrb, 1e-9, &i);
        }
    }
}

#[test]
fn finding_351_aster_half_step_preserves_and_adds_filtered_ridges() {
    use as_elevation::{ElevationStore, ElevationTile};
    use std::sync::Arc;

    const ROWS: usize = 576;
    const LAT: f64 = 75.005;
    const LON: f64 = 10.005;
    const LENGTH_KM: f64 = 3.5;
    const RADIUS_KM: f64 = 8473.43;
    let latitude = |metres: f64| LAT + (metres / 6_371_000.0).to_degrees();
    let row = |metres| ((latitude(metres) - 75.0) * 24.0 * ROWS as f64).floor() as usize;
    for (model, old_step) in [
        (ElevationModel::Aster3, 90.0),
        (ElevationModel::Aster1, 30.0),
    ] {
        for shifted in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let store = Arc::new(ElevationStore::open(dir.path()).unwrap());
            let engine = PropagationEngine::open(store.clone(), dir.path()).unwrap();
            engine.set_near_field_suppression_m(100.0);
            let half_step = old_step / 2.0;
            let centre = 1800.0 + if shifted { half_step } else { 0.0 };
            let mut terrain = vec![0; ROWS];
            for offset in -2..=2 {
                terrain[row(centre + f64::from(offset) * old_step)] = 1000;
            }
            let tile = ElevationTile::new(
                "JQ55AA",
                0,
                LAT,
                LON,
                1000,
                latitude(centre),
                LON,
                ROWS,
                1,
                terrain.clone(),
                chrono::DateTime::UNIX_EPOCH,
            )
            .unwrap();
            store.insert_or_update_if_newer(model, &tile).unwrap();
            let p = PathParams {
                lat1: LAT,
                lon1: LON,
                h1: 10.0,
                lat2: latitude(LENGTH_KM * 1000.0),
                lon2: LON,
                h2: 10.0,
                qrg_ghz: 0.144,
                radius_km: RADIUS_KM,
                f1_clearance: 0.0,
                step_width_m: old_step,
                local_obstruction: f64::MIN,
            };
            let calculate = |step_width_m| {
                engine
                    .create_path(model, &PathParams { step_width_m, ..p }, false, None)
                    .unwrap()
                    .unwrap()
                    .eps1_min
            };
            // Independent meridional row lookup and tangent-plane angle.
            // Every reference grid uses taps 90 m (ASTER3) or 30 m (ASTER1)
            // apart, regardless of how far apart the window centres are.
            let reference = |step: f64| {
                (0..=(LENGTH_KM * 1000.0 / step) as usize)
                    .map(|i| {
                        let d = i as f64 * step;
                        let sum: i32 = (-2..=2)
                            .map(|j| i32::from(terrain[row(d + f64::from(j) * old_step)]))
                            .sum();
                        let r = RADIUS_KM + f64::from(sum / 5) / 1000.0;
                        let theta = d.max(100.0) / (1000.0 * RADIUS_KM);
                        (r * theta.cos() - (RADIUS_KM + 0.010)).atan2(r * theta.sin())
                    })
                    .fold(f64::MIN, f64::max)
            };
            let coarse = calculate(old_step);
            let half = calculate(model.default_step_width_m());
            // 0.5 m visits cells at less than 1/10 of a row and nests both grids.
            let dense = reference(0.5);
            let ctx = (model, shifted, coarse, half, dense);
            assert_close(coarse, reference(old_step), 1e-7, &ctx);
            assert!(half >= coarse && half <= dense + 1e-7, "{ctx:?}");
            assert_close(half, reference(half_step), 1e-7, &ctx);
            if shifted {
                assert!(coarse < 0.0 && half > 0.0, "{ctx:?}");
            }
        }
    }
}
