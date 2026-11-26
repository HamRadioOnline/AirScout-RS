use as_geo::latlon::{self, GPoint};
use as_golden::{assert_close, assert_close_v, deltas::DeltaManifest, load, Case};

const TOL: f64 = 1e-9;

fn check_point(p: Option<GPoint>, c: &Case) {
    if c.result.is_null() {
        assert!(p.is_none(), "expected None for {c:?}, got {p:?}");
    } else {
        let p = p.unwrap_or_else(|| panic!("expected Some for {c:?}"));
        let elat = as_golden::num(&c.result["lat"]).expect("lat");
        let elon = as_golden::num(&c.result["lon"]).expect("lon");
        if elat.abs() > 89.999 {
            // asin() is ill-conditioned at the poles: libm differences between .NET and the
            // host shift lat by ~1e-6 deg and lon becomes arbitrary. Compare by separation.
            let sep_km = latlon::distance(p.lat, p.lon, elat, elon);
            assert!(
                sep_km < 1e-3,
                "pole point {p:?} vs ({elat},{elon}) sep {sep_km} km for {c:?}"
            );
        } else {
            assert_close_v(p.lat, &c.result["lat"], TOL, c);
            assert_close_v(p.lon, &c.result["lon"], TOL, c);
        }
    }
}

#[test]
fn latlon_matches_original() {
    let g = load("latlon.json");
    let mut deltas = DeltaManifest::load("latlon");
    let mut seen = std::collections::BTreeSet::new();
    for (i, c) in g.cases.iter().enumerate() {
        seen.insert(c.func.clone());
        match c.func.as_str() {
            "distance" => assert_close_v(
                latlon::distance(c.f("lat1"), c.f("lon1"), c.f("lat2"), c.f("lon2")),
                &c.result,
                TOL,
                c,
            ),
            "bearing" => assert_close_v(
                latlon::bearing(c.f("lat1"), c.f("lon1"), c.f("lat2"), c.f("lon2")),
                &c.result,
                TOL,
                c,
            ),
            "midpoint" | "destination_point" => {
                let (point, wrapped, finding) = if c.func == "midpoint" {
                    (
                        latlon::midpoint(c.f("lat1"), c.f("lon1"), c.f("lat2"), c.f("lon2")),
                        i == 278,
                        "#343",
                    )
                } else {
                    (
                        latlon::destination_point(
                            c.f("lat"),
                            c.f("lon"),
                            c.f("bearing"),
                            c.f("distance"),
                        ),
                        matches!(i, 369..=371 | 375..=377 | 381..=383 | 501..=503),
                        "#15",
                    )
                };
                if wrapped {
                    let old = as_golden::num(&c.result["lon"]).unwrap();
                    let expected =
                        deltas.number(i, "lon", &c.result["lon"], old - 360.0, TOL, finding);
                    assert_close(point.lon, expected, TOL, c);
                    assert_close_v(point.lat, &c.result["lat"], TOL, c);
                } else {
                    check_point(Some(point), c);
                }
            }
            "intersection_point" => {
                let a = [c.f("lat1"), c.f("lon1"), c.f("brg1")];
                let b = [c.f("lat2"), c.f("lon2"), c.f("brg2")];
                let result = latlon::intersection_point(a[0], a[1], a[2], b[0], b[1], b[2]);
                if (417..=497).contains(&i) {
                    // #344: the C# acos argument is out of range at the polar meridian.
                    let expected = as_golden::great_circle::directed_intersection(a, b);
                    match (result, expected) {
                        (None, None) => {
                            deltas.expected(
                                i,
                                "result",
                                &c.result,
                                Default::default(),
                                0.0,
                                "#344",
                            );
                        }
                        (Some(actual), Some((lat, lon))) => {
                            let lat = deltas.number(i, "lat", &c.result["lat"], lat, TOL, "#344");
                            let lon = deltas.number(i, "lon", &c.result["lon"], lon, TOL, "#344");
                            assert!(
                                latlon::distance(actual.lat, actual.lon, lat, lon) < 0.001,
                                "case {i}: {actual:?} vs ({lat},{lon})"
                            );
                        }
                        _ => panic!("case {i}: {result:?} vs {expected:?}"),
                    }
                } else {
                    check_point(result, c);
                }
            }
            // locator-based variants are covered in maidenhead_golden.rs (need the locator port)
            "distance_loc" | "bearing_loc" | "is_precise_loc" => continue,
            other => panic!("unhandled golden fn {other}"),
        }
    }
    deltas.finish();
    assert_eq!(seen.len(), 8, "fns seen: {seen:?}");
}

#[test]
fn finding_15_destination_wraps() {
    let p = latlon::destination_point(10.0, 179.99, 90.0, 100.0);
    assert!((p.lat - 9.998755524).abs() < TOL, "{p:?}");
    assert!((p.lon - -179.096807240).abs() < TOL, "{p:?}");
}

#[test]
fn finding_343_midpoint_wraps() {
    let p = latlon::midpoint(10.0, 178.0, 10.0, -176.0);
    assert!((p.lat - 10.013445903).abs() < TOL, "{p:?}");
    assert!((p.lon - -179.0).abs() < TOL, "{p:?}");
}

#[test]
fn finding_345_antipodal_haversine_is_finite() {
    // These exact antipodes can round a to 1 + EPSILON in the haversine.
    let distance = latlon::distance(0.08, 0.0, -0.08, 180.0);
    assert!(
        (distance - 6371.0 * std::f64::consts::PI).abs() < TOL,
        "{distance}"
    );
}

#[test]
fn finding_344_plane_on_path_returns_its_position() {
    assert_eq!(
        latlon::intersection_point(51.9987, 11.0027, 180.0, 51.0, 11.0027, 90.0),
        Some(GPoint::new(51.0, 11.0027))
    );
    assert_eq!(
        latlon::intersection_point(51.9987, 11.0027, 180.0, 51.0, 11.0027, 0.0),
        None
    );
}

#[test]
fn finding_344_nearly_meridional_tracks_are_stable() {
    for offset in [-1e-8, 1e-8] {
        for brg1 in [0.0, 45.0, 180.0, 270.0] {
            for brg2 in [0.0, 90.0, 135.0, 180.0, 270.0] {
                let a = [51.9987, 11.0027, brg1];
                let b = [51.0, 11.0027 + offset, brg2];
                let actual = latlon::intersection_point(a[0], a[1], a[2], b[0], b[1], b[2]);
                let expected = as_golden::great_circle::directed_intersection(a, b);
                match (actual, expected) {
                    (None, None) => {}
                    (Some(p), Some((lat, lon))) => {
                        assert!(
                            latlon::distance(p.lat, p.lon, lat, lon) < 0.001,
                            "{a:?} {b:?}: {p:?} vs {expected:?}"
                        );
                    }
                    _ => panic!("{a:?} {b:?}: {actual:?} vs {expected:?}"),
                }
            }
        }
    }
}

#[test]
fn finding_344_polar_crossing_across_meridian_cutoff() {
    let a = [89.0, 0.0, 180.0];
    for offset in [1e-8, 1e-7, 1e-6, 1e-5, 1e-4] {
        let b = [88.0, offset, 270.0];
        let (lat, lon) = as_golden::great_circle::directed_intersection(a, b).unwrap();
        let point = latlon::intersection_point(a[0], a[1], a[2], b[0], b[1], b[2]).unwrap();
        assert!(
            latlon::distance(point.lat, point.lon, lat, lon) < 1e-5,
            "offset {offset}: {point:?} vs ({lat}, {lon})"
        );
    }
}
