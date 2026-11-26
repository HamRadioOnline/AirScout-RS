use as_geo::propagation as p;
use as_golden::{assert_close_v, load};

const TOL: f64 = 1e-9;

#[test]
fn propagation_formulas_match_original() {
    let g = load("propagation_formulas.json");
    let mut seen = std::collections::BTreeSet::new();
    for c in &g.cases {
        seen.insert(c.func.clone());
        let r = match c.func.as_str() {
            "alpha_from_distance" => p::alpha_from_distance(c.f("dist"), c.f("re")),
            "slant_range_from_heights" => {
                p::slant_range_from_heights(c.f("h"), c.f("dist"), c.f("H"), c.f("radius"))
            }
            "slant_range_from_heights_latlon" => p::slant_range_from_heights_latlon(
                c.f("h"),
                c.f("lat1"),
                c.f("lon1"),
                c.f("lat2"),
                c.f("lon2"),
                c.f("H"),
                c.f("radius"),
            ),
            "epsilon_from_heights" => {
                p::epsilon_from_heights(c.f("h"), c.f("dist"), c.f("H"), c.f("radius"))
            }
            "theta_from_heights" => {
                p::theta_from_heights(c.f("h"), c.f("dist"), c.f("H"), c.f("radius"))
            }
            "height_from_epsilon" => {
                p::height_from_epsilon(c.f("h"), c.f("dist"), c.f("eps"), c.f("radius"))
            }
            "distance_from_epsilon" => {
                p::distance_from_epsilon(c.f("h"), c.f("H"), c.f("eps"), c.f("radius"))
            }
            "f1_radius" => p::f1_radius(c.f("qrg"), c.f("dist1"), c.f("dist2")),
            "visibility_distance" => p::visibility_distance(c.f("h1"), c.f("h2"), c.f("re")),
            "doppler_shift" => p::doppler_shift(c.f("qrg"), c.f("speed")),
            other => panic!("unhandled golden fn {other}"),
        };
        // relative tolerance for large magnitudes (heights in m up to ~1e7)
        let tol = TOL * as_golden::num(&c.result).map_or(1.0, |e| e.abs().max(1.0));
        assert_close_v(r, &c.result, tol, c);
    }
    assert_eq!(seen.len(), 10, "fns seen: {seen:?}");
}
