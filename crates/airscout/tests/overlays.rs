use airscout::overlays::*;
use as_geo::latlon::{destination_point, distance};
use as_geo::latlon::{GPoint, GRect};
use as_propagation::{PathParams, PropagationPath, PropagationPoint};

fn world() -> GRect {
    GRect::new(-90.0, -180.0, 90.0, 180.0)
}

fn path(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> PropagationPath {
    PropagationPath::new(
        PathParams {
            lat1,
            lon1,
            h1: 10.0,
            lat2,
            lon2,
            h2: 10.0,
            qrg_ghz: 0.144,
            radius_km: 6371.0 * 1.33,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: f64::MIN,
        },
        0.0,
        0.0,
        true,
    )
}

#[test]
fn rings_are_100km_apart() {
    let rings = distance_rings(50.0, 10.0);
    assert_eq!(rings.len(), 10);
    for (k, ring) in rings.iter().enumerate() {
        assert_eq!(ring.len(), 361);
        let want = (k as f64 + 1.0) * 100.0;
        for p in ring {
            assert!((distance(50.0, 10.0, p.lat, p.lon) - want).abs() < 0.01);
        }
    }
}

fn sizes(cells: &[LocatorCell]) -> Vec<(usize, f64, f64)> {
    let mut v: Vec<(usize, f64, f64)> = cells
        .iter()
        .map(|c| {
            (
                c.loc.len(),
                c.rect.max_lat - c.rect.min_lat,
                c.rect.max_lon - c.rect.min_lon,
            )
        })
        .collect();
    v.dedup_by(|a, b| a.0 == b.0);
    v
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn grid_cell_size_by_zoom() {
    // small views so the cap does not interfere
    let view = |d: f64| GRect::new(50.0, 10.0, 50.0 + d, 10.0 + 2.0 * d);
    let s24 = 1.0 / 24.0;

    let z4 = locator_grid(world(), 4.0, false);
    assert_eq!(z4.len(), 18 * 18);
    let s = sizes(&z4);
    assert_eq!(s.len(), 1);
    assert!(s[0].0 == 2 && near(s[0].1, 10.0) && near(s[0].2, 20.0));

    let z7 = locator_grid(view(5.0), 7.0, false);
    assert!(z7.iter().all(|c| c.loc.len() == 4));
    assert!(z7.iter().all(|c| near(c.rect.max_lat - c.rect.min_lat, 1.0)
        && near(c.rect.max_lon - c.rect.min_lon, 2.0)));

    let z12 = locator_grid(view(1.0), 12.0, false);
    assert!(z12
        .iter()
        .any(|c| c.loc.len() == 4 && near(c.rect.max_lat - c.rect.min_lat, 1.0)));
    assert!(z12.iter().any(|c| c.loc.len() == 6
        && near(c.rect.max_lat - c.rect.min_lat, s24)
        && near(c.rect.max_lon - c.rect.min_lon, 2.0 * s24)));

    let z15 = locator_grid(view(0.05), 15.0, false);
    assert!(z15
        .iter()
        .any(|c| c.loc.len() == 6 && near(c.rect.max_lat - c.rect.min_lat, s24)));
    assert!(z15.iter().any(|c| c.loc.len() == 8
        && near(c.rect.max_lat - c.rect.min_lat, s24 / 10.0)
        && near(c.rect.max_lon - c.rect.min_lon, 2.0 * s24 / 10.0)));

    let z19 = locator_grid(view(0.004), 19.0, false);
    assert!(z19.iter().any(|c| c.loc.len() == 8));
    assert!(z19.iter().any(|c| c.loc.len() == 10
        && near(c.rect.max_lat - c.rect.min_lat, s24 / 10.0 / 24.0)
        && near(c.rect.max_lon - c.rect.min_lon, 2.0 * s24 / 10.0 / 24.0)));

    assert!(locator_grid(world(), 0.0, false).is_empty());
}

#[test]
fn grid_cells_cover_view_and_names_match_rect() {
    let view = GRect::new(50.2, 10.3, 50.8, 11.7);
    let cells = locator_grid(view, 8.0, false);
    assert!(!cells.is_empty());
    for c in &cells {
        let b = as_geo::maidenhead::bounds_from_loc(&c.loc).unwrap();
        assert!(near(b.min_lat, c.rect.min_lat) && near(b.max_lon, c.rect.max_lon));
    }
    let covers = |lat: f64, lon: f64| {
        cells.iter().any(|c| {
            (c.rect.min_lat..=c.rect.max_lat).contains(&lat)
                && (c.rect.min_lon..=c.rect.max_lon).contains(&lon)
        })
    };
    assert!(covers(50.2, 10.3) && covers(50.8, 11.7) && covers(50.5, 11.0));
}

#[test]
fn grid_is_capped() {
    // zoom 12 over 3x6 degrees: coarse 18 cells, fine 72*72*... far above 2000 -> fine dropped
    let view = GRect::new(48.0, 8.0, 51.0, 14.0);
    let cells = locator_grid(view, 12.0, false);
    assert!(!cells.is_empty() && cells.len() <= MAX_GRID_CELLS);
    assert!(cells.iter().all(|c| c.loc.len() == 4), "fine level dropped");

    // zoom 16 over the same view: the 1/24 deg coarse level is too big -> coarser band (1 deg)
    let cells = locator_grid(view, 16.0, false);
    assert!(!cells.is_empty() && cells.len() <= MAX_GRID_CELLS);
    assert!(cells.iter().all(|c| c.loc.len() == 4));

    // zoom 14 over ~3x6 degrees: coarse 1/24 x 1/12 deg = 72*72 = 5184 cells -> coarsened, not blank
    let view14 = GRect::new(49.0, 9.0, 52.0, 15.0);
    let cells = locator_grid(view14, 14.0, false);
    assert!(!cells.is_empty() && cells.len() <= MAX_GRID_CELLS);
    assert!(cells
        .iter()
        .all(|c| near(c.rect.max_lat - c.rect.min_lat, 1.0)));

    // huge view at deep zoom falls through to the coarsest band
    let cells = locator_grid(world(), 19.0, false);
    assert_eq!(cells.len(), 18 * 18);

    // fine level kept when it fits
    let small = GRect::new(50.0, 10.0, 50.5, 11.0);
    let cells = locator_grid(small, 12.0, false);
    assert!(cells.iter().any(|c| c.loc.len() == 6));
    assert!(cells.len() <= MAX_GRID_CELLS);
}

#[test]
fn small_letters_flag() {
    let view = GRect::new(50.0, 10.0, 50.3, 10.6);
    let upper = locator_grid(view, 12.0, false);
    let lower = locator_grid(view, 12.0, true);
    let six = |v: &[LocatorCell]| v.iter().find(|c| c.loc.len() == 6).unwrap().loc.clone();
    let (u, l) = (six(&upper), six(&lower));
    assert_eq!(&u[4..], u[4..].to_uppercase());
    assert_eq!(&l[4..], l[4..].to_lowercase());
    assert_eq!(u.to_lowercase(), l.to_lowercase());
    assert_eq!(&u[..4], &l[..4]);
}

fn pp(lat: f64, lon: f64, h1: f64, h2: f64) -> PropagationPoint {
    PropagationPoint {
        lat,
        lon,
        h1,
        h2,
        f1: 0.0,
    }
}

#[test]
fn visible_segment_from_hand_built_points() {
    // max(h1,h2) per point: 0 (not visible: not > 0), 50, 200, 90, 500 (too high), 0 (last)
    let pts = [
        pp(50.0, 10.0, 0.0, 0.0),
        pp(50.1, 10.1, 50.0, 10.0),
        pp(50.2, 10.2, 20.0, 200.0),
        pp(50.3, 10.3, 90.0, 5.0),
        pp(50.4, 10.4, 500.0, 1.0),
        pp(50.5, 10.5, 0.0, 0.0),
    ];
    let dx = GPoint::new(50.55, 10.55);
    let o = overlay_from_points(&pts, dx, true, GPoint::new(0.0, 0.0), 300.0);
    assert_eq!(
        o.visible,
        vec![vec![
            GPoint::new(50.1, 10.1),
            GPoint::new(50.2, 10.2),
            GPoint::new(50.3, 10.3)
        ]]
    );
    // full: all raw points, last replaced by the DX location
    assert_eq!(o.full[0].len(), 6);
    assert_eq!(o.full[0][4], GPoint::new(50.4, 10.4));
    assert_eq!(o.full[0][5], dx);
    // hot markers: points with max < 300 are indices 0..=3 and 5 -> first 0, last 5 (raw point)
    let [a, m, c] = o.hot_markers;
    assert_eq!(a, Some(GPoint::new(50.0, 10.0)));
    assert_eq!(c, Some(GPoint::new(50.5, 10.5)));
    let m = m.unwrap();
    assert!((m.lat - 50.25).abs() < 0.01 && (m.lon - 10.25).abs() < 0.01);

    // the last point is visible too: it is replaced by the DX location
    let mut pts2 = pts;
    pts2[5] = pp(50.5, 10.5, 40.0, 10.0);
    let o = overlay_from_points(&pts2, dx, false, GPoint::new(0.0, 0.0), 300.0);
    assert_eq!(*o.visible[0].last().unwrap(), dx);
    assert!(!o.valid);
}

#[test]
fn visible_segment_of_real_path() {
    let p = path(50.5013, 10.5027, 51.8, 13.9);
    let pts = p.info_points().unwrap();
    // hand-checked: max(h1,h2) is ~1125 m at index 140 (h1 1167, h2 1125), 1868 m at 100 and
    // 1521 m at 160, rising towards both ends
    let max_alt = 1500.0;
    let pos = |i: usize| GPoint::new(pts[i].lat, pts[i].lon);
    let o = path_overlay(&p, max_alt);
    assert_eq!(o.visible.len(), 1);
    let v = &o.visible[0];
    assert!(v.contains(&pos(140)));
    assert!(!v.contains(&pos(100)) && !v.contains(&pos(160)));
    assert!(!v.contains(&GPoint::new(51.8, 13.9)));
    assert!(v.len() > 20 && v.len() < 60, "len {}", v.len());
    assert!(o.valid);
    assert_eq!(o.full[0].len(), pts.len());
    assert_eq!(*o.full[0].last().unwrap(), GPoint::new(51.8, 13.9));
    let [a, m, c] = o.hot_markers;
    let (a, m, c) = (a.unwrap(), m.unwrap(), c.unwrap());
    assert_eq!(a, v[0]);
    assert_eq!(c, *v.last().unwrap());
    // middle marker is between the two ends, near index 140
    assert!(distance(m.lat, m.lon, pts[140].lat, pts[140].lon) < 10.0);
    assert_eq!(o.mid, p.mid_point());
}

#[test]
fn zero_length_path_has_no_segments() {
    let p = path(50.0, 10.0, 50.0, 10.0);
    let o = path_overlay(&p, 1000.0);
    assert!(o.full.is_empty() && o.visible.is_empty());
    assert_eq!(o.hot_markers, [None, None, None]);
}

#[test]
fn antimeridian_path_is_split() {
    let p = path(10.0, 175.0, 12.0, -175.0);
    let o = path_overlay(&p, 1.0e9);
    assert_eq!(o.full.len(), 2);
    assert_eq!(o.visible.len(), 2);
    for lines in [&o.full, &o.visible] {
        for seg in lines.iter() {
            assert!(seg.windows(2).all(|w| (w[1].lon - w[0].lon).abs() < 180.0));
        }
        assert_eq!(lines[0].last().unwrap().lon, 180.0);
        assert_eq!(lines[1][0].lon, -180.0);
        assert!(lines[0].last().unwrap().lat > 10.0 && lines[0].last().unwrap().lat < 12.0);
    }
}

#[test]
fn beam_width_of_10dbi() {
    let want = (41253.0_f64 / 10f64.powf((10.0 + 2.15) / 10.0)).sqrt();
    assert!((beam_width_deg(10.0) - want).abs() < 1e-12);
    // the brief quotes 16.82; the C# formula gives about 50 degrees for 10 dBi
    assert!((want - 50.1).abs() < 0.1);
}

#[test]
fn beam_wedge_shape() {
    let w = beam_wedge(50.0, 10.0, 90.0, 20.0, 100.0);
    // 100 left edge + 20 arc + 100 right edge
    assert_eq!(w.len(), 220);
    assert_eq!(w[0], GPoint::new(50.0, 10.0));
    let arc = destination_point(50.0, 10.0, 80.0, 100.0);
    assert_eq!(w[100], arc);
    assert!((distance(50.0, 10.0, w[119].lat, w[119].lon) - 100.0).abs() < 0.01);
    // wraps around north
    let n = beam_wedge(50.0, 10.0, 0.0, 20.0, 10.0);
    assert!(n.iter().all(|p| p.lat.is_finite() && p.lon.is_finite()));
}

#[test]
fn grid_wrapped_view_is_not_blank() {
    // view across the antimeridian: min_lon 170 > max_lon -170
    let view = GRect::new(0.0, 170.0, 10.0, -170.0);
    let cells = locator_grid(view, 7.0, false);
    // 10 deg x (10 + 10 deg) = 10 lat rows * 10 lon cells per half
    assert_eq!(cells.len(), 10 * 5 * 2);
    assert!(cells.iter().any(|c| c.rect.max_lon <= -170.0 + 1e-9));
    assert!(cells.iter().any(|c| c.rect.min_lon >= 170.0 - 1e-9));
    // never more than the cap, never wrapped to a huge count
    let wide = GRect::new(-80.0, 100.0, 80.0, -100.0);
    let cells = locator_grid(wide, 9.0, false);
    assert!(!cells.is_empty() && cells.len() <= MAX_GRID_CELLS);
}
