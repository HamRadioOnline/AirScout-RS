use as_geo::latlon::{self, GPoint};
use as_golden::{assert_close_v, load, Case};

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
    let mut seen = std::collections::BTreeSet::new();
    for c in &g.cases {
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
            "midpoint" => check_point(
                Some(latlon::midpoint(
                    c.f("lat1"),
                    c.f("lon1"),
                    c.f("lat2"),
                    c.f("lon2"),
                )),
                c,
            ),
            "destination_point" => check_point(
                Some(latlon::destination_point(
                    c.f("lat"),
                    c.f("lon"),
                    c.f("bearing"),
                    c.f("distance"),
                )),
                c,
            ),
            "intersection_point" => check_point(
                latlon::intersection_point(
                    c.f("lat1"),
                    c.f("lon1"),
                    c.f("brg1"),
                    c.f("lat2"),
                    c.f("lon2"),
                    c.f("brg2"),
                ),
                c,
            ),
            // locator-based variants are covered in maidenhead_golden.rs (need the locator port)
            "distance_loc" | "bearing_loc" | "is_precise_loc" => continue,
            other => panic!("unhandled golden fn {other}"),
        }
    }
    assert_eq!(seen.len(), 8, "fns seen: {seen:?}");
}
