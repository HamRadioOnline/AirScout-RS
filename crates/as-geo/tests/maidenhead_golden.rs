use as_geo::latlon;
use as_geo::maidenhead::{self, PositionInRectangle, SubGridsFilter};
use as_golden::{assert_close_v, load, Case};

const TOL: f64 = 1e-9;

fn pos(s: &str) -> PositionInRectangle {
    match s {
        "TopLeft" => PositionInRectangle::TopLeft,
        "TopMiddle" => PositionInRectangle::TopMiddle,
        "TopRight" => PositionInRectangle::TopRight,
        "BottomLeft" => PositionInRectangle::BottomLeft,
        "BottomMiddle" => PositionInRectangle::BottomMiddle,
        "BottomRight" => PositionInRectangle::BottomRight,
        "MiddleLeft" => PositionInRectangle::MiddleLeft,
        "MiddleRight" => PositionInRectangle::MiddleRight,
        "MiddleMiddle" => PositionInRectangle::MiddleMiddle,
        o => panic!("pos {o}"),
    }
}

fn filter(s: &str) -> SubGridsFilter {
    match s {
        "All" => SubGridsFilter::All,
        "Top" => SubGridsFilter::Top,
        "Bottom" => SubGridsFilter::Bottom,
        "Left" => SubGridsFilter::Left,
        "Right" => SubGridsFilter::Right,
        o => panic!("filter {o}"),
    }
}

fn check_f64<E: std::fmt::Debug>(r: Result<f64, E>, c: &Case) {
    if c.is_error() {
        assert!(r.is_err(), "expected Err for {c:?}, got {r:?}");
    } else {
        assert_close_v(r.expect("Ok"), &c.result, TOL, c);
    }
}

fn check_str<E: std::fmt::Debug>(r: Result<Option<String>, E>, c: &Case) {
    if c.is_error() {
        assert!(r.is_err(), "expected Err for {c:?}, got {r:?}");
    } else {
        assert_eq!(r.expect("Ok").as_deref(), c.result.as_str(), "{c:?}");
    }
}

#[test]
fn maidenhead_matches_original() {
    let g = load("maidenhead.json");
    let mut seen = std::collections::BTreeSet::new();
    for c in &g.cases {
        seen.insert(c.func.clone());
        match c.func.as_str() {
            "check" => assert_eq!(
                Some(maidenhead::check(&c.s("loc"))),
                c.result.as_bool(),
                "{c:?}"
            ),
            "convert" => check_str(maidenhead::convert(&c.s("loc"), c.b("small")).map(Some), c),
            "lat_from_loc" => check_f64(maidenhead::lat_from_loc(&c.s("loc"), pos(&c.s("pos"))), c),
            "lon_from_loc" => check_f64(maidenhead::lon_from_loc(&c.s("loc"), pos(&c.s("pos"))), c),
            "bounds_from_loc" => {
                let r = maidenhead::bounds_from_loc(&c.s("loc"));
                if c.is_error() {
                    assert!(r.is_err(), "{c:?}");
                } else {
                    let r = r.expect("Ok");
                    assert_close_v(r.min_lat, &c.result["min_lat"], TOL, c);
                    assert_close_v(r.min_lon, &c.result["min_lon"], TOL, c);
                    assert_close_v(r.max_lat, &c.result["max_lat"], TOL, c);
                    assert_close_v(r.max_lon, &c.result["max_lon"], TOL, c);
                }
            }
            "get_sub_grids" => {
                let r = maidenhead::get_sub_grids(
                    &c.s("loc"),
                    filter(&c.s("filter")),
                    c.b("small"),
                    c.b("reverse"),
                );
                if c.is_error() {
                    assert!(r.is_err(), "{c:?}");
                } else {
                    let exp: Vec<String> = c
                        .result
                        .as_array()
                        .expect("array")
                        .iter()
                        .map(|v| v.as_str().expect("str").to_string())
                        .collect();
                    assert_eq!(r.expect("Ok"), exp, "{c:?}");
                }
            }
            "is_precise" => {
                let r = maidenhead::is_precise(c.f("lat"), c.f("lon"), c.i("precision") as i32);
                if c.is_error() {
                    assert!(r.is_err(), "{c:?}");
                } else {
                    assert_eq!(r.ok(), c.result.as_bool(), "{c:?}");
                }
            }
            "loc_from_lat_lon" => check_str(
                maidenhead::loc_from_lat_lon(
                    c.f("lat"),
                    c.f("lon"),
                    c.b("small"),
                    c.i("precision") as i32,
                    c.b("autolength"),
                ),
                c,
            ),
            other => panic!("unhandled golden fn {other}"),
        }
    }
    assert_eq!(seen.len(), 8, "fns seen: {seen:?}");
}

/// Locator-based helpers of LatLon.cs (they need the locator port).
#[test]
fn latlon_locator_variants_match_original() {
    let g = load("latlon.json");
    let mut n = 0;
    for c in &g.cases {
        match c.func.as_str() {
            "distance_loc" => assert_close_v(
                latlon::distance_loc(&c.s("loc1"), &c.s("loc2")).expect("Ok"),
                &c.result,
                TOL,
                c,
            ),
            "bearing_loc" => assert_close_v(
                latlon::bearing_loc(&c.s("loc1"), &c.s("loc2")).expect("Ok"),
                &c.result,
                TOL,
                c,
            ),
            "is_precise_loc" => {
                let r = latlon::is_precise_loc(&c.s("loc"), c.f("lat"), c.f("lon"));
                if c.is_error() {
                    assert!(r.is_err(), "{c:?}");
                } else {
                    assert_eq!(r.ok(), c.result.as_bool(), "{c:?}");
                }
            }
            _ => continue,
        }
        n += 1;
    }
    assert!(n > 40, "too few locator cases: {n}");
}
