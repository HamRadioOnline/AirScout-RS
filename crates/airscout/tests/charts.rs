use airscout::charts::profile;
use as_propagation::{PathParams, PropagationPath};

const R: f64 = 6371.0 * 1.33;

fn path(h1: f64, h2: f64) -> PropagationPath {
    PropagationPath::new(
        PathParams {
            lat1: 50.0,
            lon1: 10.0,
            h1,
            lat2: 50.5,
            lon2: 11.0,
            h2,
            qrg_ghz: 10.0,
            radius_km: R,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: f64::MIN,
        },
        0.01,
        0.02,
        true,
    )
}

fn terrain(n: usize) -> Vec<f64> {
    (0..n).map(|i| 100.0 + (i % 7) as f64 * 20.0).collect()
}

#[test]
fn series_lengths_match_info_points() {
    let p = path(300.0, 250.0);
    let n = p.info_points().unwrap().len();
    let s = profile(&p, &terrain(n), 12000.0);
    for len in [
        s.elevation.len(),
        s.min_h1.len(),
        s.min_h2.len(),
        s.max_h.len(),
        s.min_h_area.len(),
        s.los.len(),
    ] {
        assert_eq!(len, n);
    }
    for (i, pt) in s.elevation.iter().enumerate() {
        assert_eq!(pt[0], i as f64);
    }
    // mismatch returns empty series, no panic
    let e = profile(&p, &terrain(n - 1), 12000.0);
    assert!(e.elevation.is_empty() && e.los.is_empty() && e.min_h_area.is_empty());
    assert_eq!(e.elevation_y_max, 0.0);
}

#[test]
fn no_info_points_returns_empty() {
    let mut p = path(300.0, 250.0);
    p.params.step_width_m = 0.0;
    let s = profile(&p, &terrain(5), 12000.0);
    assert!(s.elevation.is_empty() && s.min_h1.is_empty());
}

#[test]
fn min_h_area_bounds() {
    let p = path(300.0, 250.0);
    let pts = p.info_points().unwrap();
    let t = terrain(pts.len());
    // large max_alt: lower bound = max(h1,h2); small max_alt: clamped to max_alt
    for max_alt in [12000.0, 5.0] {
        let s = profile(&p, &t, max_alt);
        for (i, (top, low)) in s.min_h_area.iter().enumerate() {
            assert_eq!(top[1], max_alt);
            let want = pts[i].h1.max(pts[i].h2).min(max_alt);
            assert!((low - want).abs() < 1e-9);
            assert_eq!(s.max_h[i][1], max_alt);
            assert_eq!(s.min_h1[i][1], pts[i].h1);
            assert_eq!(s.min_h2[i][1], pts[i].h2);
        }
    }
}

#[test]
fn los_line_ends_at_antenna_heights() {
    let p = path(300.0, 250.0);
    let n = p.info_points().unwrap().len();
    let s = profile(&p, &terrain(n), 12000.0);
    assert!((s.los[0][1] - 300.0).abs() < 1e-6);
    // independent geometry: chord from A (R+h1, angle 0) to B (R+h2, angle d/R)
    let d = p.distance();
    let a = d / R;
    let (ax, ay) = (0.0, R + 0.3);
    let (bx, by) = ((R + 0.25) * a.sin(), (R + 0.25) * a.cos());
    let (vx, vy) = (bx - ax, by - ay);
    for (i, pt) in s.los.iter().enumerate() {
        let al = i as f64 / R;
        let (dx, dy) = (al.sin(), al.cos());
        let t = (ax * vy - ay * vx) / (dx * vy - dy * vx);
        let want_m = (t - R) * 1000.0;
        assert!(
            (pt[1] - want_m).abs() < 1e-3,
            "i={i} got {} want {want_m}",
            pt[1]
        );
    }
    // at x = d the line is at h2: interpolate between last two points
    let k = d as usize;
    let f = d - k as f64;
    let at_d = s.los[k][1] + (s.los[k + 1][1] - s.los[k][1]) * f;
    assert!((at_d - 250.0).abs() < 0.5, "at_d={at_d}");
}

#[test]
fn y_max_is_ten_percent_above_peak() {
    let p = path(300.0, 250.0);
    let n = p.info_points().unwrap().len();
    let mut t = terrain(n);
    t[3] = 1000.0;
    let s = profile(&p, &t, 12000.0);
    assert!((s.elevation_y_max - 1100.0).abs() < 1e-9);
    assert_eq!(s.elevation[3], [3.0, 1000.0]);
}
