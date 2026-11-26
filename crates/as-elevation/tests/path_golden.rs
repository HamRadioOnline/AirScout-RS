mod common;
use as_elevation::{DbStatus, ElevationModel, ElevationStore};
use as_geo::latlon::GRect;
use std::sync::atomic::AtomicBool;

/// Store with the fixture GLOBE tiles under GLOBE and ASTER3 (ASTER3 exercises the averaging path),
/// bounds 50..52 / 10..14 like the harness.
fn fixture_store(complete: bool) -> (tempfile::TempDir, ElevationStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    for ((model, _), t) in common::fixture_tiles() {
        if model == ElevationModel::Globe {
            store
                .insert_or_update_if_newer(ElevationModel::Globe, &t)
                .unwrap();
            store
                .insert_or_update_if_newer(ElevationModel::Aster3, &t)
                .unwrap();
        }
    }
    store.set_bounds(GRect::new(50.0, 10.0, 52.0, 14.0));
    if complete {
        store.set_status_bit(ElevationModel::Globe, DbStatus::COMPLETE);
        store.set_status_bit(ElevationModel::Aster3, DbStatus::COMPLETE);
    }
    (dir, store)
}

#[test]
fn create_path_matches_original() {
    let g = as_golden::load("elevation_path.json");
    let cases: Vec<_> = g.by_fn("create_path").collect();
    assert_eq!(cases.len(), 56);
    // the harness uses a fresh db per (complete, model, step) group: cache state is part of the behaviour
    for group in cases.chunk_by(|a, b| {
        a.b("complete") == b.b("complete")
            && a.s("model") == b.s("model")
            && a.f("step") == b.f("step")
    }) {
        let (_d, store) = fixture_store(group[0].b("complete"));
        for c in group {
            let model = common::model_from(&c.s("model"));
            let p = store
                .create_path(
                    model,
                    c.f("lat1"),
                    c.f("lon1"),
                    c.f("lat2"),
                    c.f("lon2"),
                    c.f("step"),
                    false,
                    None,
                )
                .unwrap()
                .expect("not cancelled");
            assert_eq!(p.count as i64, c.result["count"].as_i64().unwrap(), "{c:?}");
            assert_eq!(p.valid, c.result["valid"].as_bool().unwrap(), "{c:?}");
            let exp: Vec<i16> = c.result["path"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as i16)
                .collect();
            assert_eq!(p.path, exp, "{c:?}");
        }
    }
}

#[test]
fn zero_distance_path_has_count_1() {
    let (_d, store) = fixture_store(true);
    let p = store
        .create_path(
            ElevationModel::Globe,
            51.0,
            11.0,
            51.0,
            11.0,
            1000.0,
            false,
            None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(p.count, 1);
    assert_eq!(p.path.len(), 1);
    assert_eq!(p.distance(), 0.0);
}

#[test]
fn cancel_returns_none() {
    let (_d, store) = fixture_store(true);
    let cancel = AtomicBool::new(true);
    let p = store
        .create_path(
            ElevationModel::Globe,
            50.1,
            10.1,
            51.9,
            13.9,
            90.0,
            false,
            Some(&cancel),
        )
        .unwrap();
    assert!(p.is_none());
}

#[test]
fn find_or_create_caches_valid_paths() {
    let (_d, store) = fixture_store(true);
    let a = store
        .find_or_create_path(
            ElevationModel::Globe,
            50.5,
            10.5,
            51.8,
            13.9,
            1000.0,
            true,
            None,
        )
        .unwrap()
        .unwrap();
    assert!(a.valid);
    let found = store
        .find_path(ElevationModel::Globe, 50.5, 10.5, 51.8, 13.9, 1000.0)
        .unwrap()
        .expect("cached");
    assert_eq!(found.path, a.path);
    assert_eq!(found.count, a.count);
    // invalid paths are not stored
    let (_d2, store2) = fixture_store(false);
    let b = store2
        .find_or_create_path(
            ElevationModel::Globe,
            50.5,
            10.5,
            51.8,
            13.9,
            1000.0,
            true,
            None,
        )
        .unwrap()
        .unwrap();
    assert!(!b.valid);
    assert!(store2
        .find_path(ElevationModel::Globe, 50.5, 10.5, 51.8, 13.9, 1000.0)
        .unwrap()
        .is_none());
}

#[test]
fn info_points_rules() {
    let (_d, store) = fixture_store(true);
    let p = store
        .create_path(
            ElevationModel::Globe,
            50.5,
            10.5,
            51.8,
            13.9,
            90.0,
            false,
            None,
        )
        .unwrap()
        .unwrap();
    let info = p.info_points().unwrap();
    assert_eq!(info.len(), p.distance() as usize + 1);
    let mut coarse = p.clone();
    coarse.step_width_m = 1500.0;
    assert!(coarse.info_points().is_err());
}

#[test]
fn invalid_coordinate_path_is_err() {
    let (_d, store) = fixture_store(true);
    // lat 91 would be folded to 89 by DestinationPoint (asin(sin)); lon is not normalised -> C# throws
    let r = store.create_path(
        ElevationModel::Globe,
        51.0,
        181.0,
        51.0,
        10.0,
        1000.0,
        false,
        None,
    );
    assert!(r.is_err(), "{r:?}");
}

/// Review M6: non-positive, non-finite or absurdly small step widths are argument errors, not a
/// panic (debug overflow) or a huge allocation.
#[test]
fn bad_step_width_is_err() {
    let (_d, store) = fixture_store(true);
    for step in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e-9] {
        let r = store.create_path(
            ElevationModel::Globe,
            50.5,
            10.5,
            51.8,
            13.9,
            step,
            false,
            None,
        );
        assert!(r.is_err(), "step {step}: {r:?}");
    }
}
