//! Map view: zoom-to-fit math and one frame through a kittest harness (no tiles, no network).
use airscout::app::PathEntry;
use airscout::app::{AppState, Services};
use airscout::services::PathRequest;
use airscout::ui::map::{nearest_view, MapView};
use airscout::ui::map_geom::{bbox_of, fit_view, hit_plane, toggle_selection, HIT_SLACK_PX};
use airscout::ui::Maps;
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_geo::latlon::{GPoint, GRect};
use as_propagation::{PathParams, PropagationPath};
use as_settings::Settings;
use eframe::egui::{pos2, Pos2};
use egui_kittest::Harness;
use std::sync::Arc;

fn entry(path: PropagationPath) -> PathEntry {
    PathEntry {
        index: 0,
        call: "DX".into(),
        path,
        terrain: Vec::new(),
        my_gain_dbi: 10.0,
        dx_gain_dbi: 12.0,
    }
}

#[test]
fn fit_world_square_is_zoom_1() {
    // The mercator square (lat +-85.05) of 512 px needs zoom 1.
    let world = GRect::new(-85.05112878, -180.0, 85.05112878, 180.0);
    let (c, z) = fit_view(world, 512.0 * 1.2, 512.0 * 1.2);
    assert!((z - 1.0).abs() < 1e-6, "{z}");
    assert!(c.lat.abs() < 1e-9 && c.lon.abs() < 1e-9);
}

#[test]
fn fit_uses_the_tighter_axis_and_centres_in_mercator() {
    // 10 degrees of longitude are 10/360 of the world: 1000 px wide -> zoom log2(1000/(256*dx)).
    let b = GRect::new(50.0, 5.0, 51.0, 15.0);
    let dx: f64 = 10.0 / 360.0 * 1.2;
    let (c, z) = fit_view(b, 1000.0, 10_000.0);
    assert!((z - (1000.0 / (256.0 * dx)).log2()).abs() < 1e-9);
    assert!((c.lon - 10.0).abs() < 1e-9);
    // mercator centre lies slightly north of the arithmetic one
    assert!(c.lat > 50.5 && c.lat < 50.6, "{}", c.lat);
    // a tall box is limited by the height instead
    let (_, z2) = fit_view(GRect::new(40.0, 5.0, 60.0, 6.0), 10_000.0, 500.0);
    assert!(z2 < 6.0);
}

#[test]
fn fit_zoom_is_clamped() {
    let p = GRect::new(50.0, 10.0, 50.0, 10.0);
    assert_eq!(fit_view(p, 800.0, 600.0).1, 18.0);
    let w = GRect::new(-85.0, -180.0, 85.0, 180.0);
    assert_eq!(fit_view(w, 10.0, 10.0).1, 1.0);
}

#[test]
fn bbox_covers_points() {
    let b = bbox_of(&[GPoint::new(1.0, 5.0), GPoint::new(-2.0, 9.0)]);
    assert_eq!(
        (b.min_lat, b.max_lat, b.min_lon, b.max_lon),
        (-2.0, 1.0, 5.0, 9.0)
    );
}

struct Null;

impl Services for Null {
    fn request_path(&mut self, _req: PathRequest) {}
    fn start_feed(
        &mut self,
        _id: u64,
        _s: &Settings,
        _c: Arc<PlaneInfoCache>,
    ) -> Result<(), String> {
        Ok(())
    }
    fn stop_feed(&mut self) {}
}

fn plane(hex: &str, lat: f64, lon: f64, potential: i32) -> PlaneInfo {
    PlaneInfo {
        hex: hex.into(),
        call: hex.to_uppercase(),
        lat,
        lon,
        track: 45.0,
        alt_ft: 30_000.0,
        potential,
        category: PlaneCategory::Heavy,
        int_point: Some(GPoint::new(lat + 0.1, lon)),
        ..PlaneInfo::default()
    }
}

#[test]
fn nearest_view_follows_plane() {
    let mut st = AppState::new(
        Settings::default(),
        None,
        Box::new(Null),
        Arc::new(PlaneInfoCache::new()),
    );
    let intersection = GPoint::new(1.25, 2.5);
    st.tick.nearest = Some(PlaneInfo {
        hex: "3C0001".into(),
        call: "TE1ST".into(),
        lat: 1.0,
        lon: 2.0,
        int_point: Some(intersection),
        ..PlaneInfo::default()
    });

    assert_eq!(nearest_view(&st), (intersection, 10));
}

#[test]
fn nearest_view_defaults_to_my_position() {
    let mut st = AppState::new(
        Settings::default(),
        None,
        Box::new(Null),
        Arc::new(PlaneInfoCache::new()),
    );
    st.settings.my_lat = 1.0;
    st.settings.my_lon = 2.0;
    let expected = (GPoint::new(1.0, 2.0), 6);
    assert_eq!(nearest_view(&st), expected);
    st.tick.nearest = Some(PlaneInfo::default());
    assert_eq!(nearest_view(&st), expected);
}

#[test]
fn nearest_map_pans_and_zooms_with_a_geographic_marker() {
    use eframe::egui::{self, Event, MouseWheelSource, MouseWheelUnit, TouchPhase};

    let mut st = AppState::new(
        Settings::default(),
        None,
        Box::new(Null),
        Arc::new(PlaneInfoCache::new()),
    );
    st.tick.nearest = Some(PlaneInfo {
        hex: "3C0001".into(),
        call: "TE1ST".into(),
        lat: 0.0,
        lon: 0.05,
        int_point: Some(GPoint::new(0.0, 0.0)),
        ..PlaneInfo::default()
    });
    let mut h = Harness::builder().build_ui_state(
        |ui, (st, map): &mut (AppState, MapView)| map.show_nearest(ui, st),
        (st, MapView::without_tiles()),
    );
    let marker = |h: &Harness<'_, (AppState, MapView)>| {
        h.output()
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Mesh(mesh) => Some((mesh.calc_bounds().center(), clipped.clip_rect)),
                _ => None,
            })
            .expect("nearest aircraft mesh")
    };

    h.step();
    let (before, rect) = marker(&h);
    let center = pos2(rect.center().x, before.y);
    assert!(before.x > center.x, "marker is east of the intersection");
    h.hover_at(center);
    h.run();
    let wheel = |h: &mut Harness<'_, (AppState, MapView)>, unit, delta| {
        h.event(Event::MouseWheel {
            unit,
            delta: egui::vec2(0.0, delta),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
        h.step();
        marker(h).0
    };
    let below_notch = wheel(&mut h, MouseWheelUnit::Point, 20.0);
    assert!((below_notch.x - before.x).abs() < 1.0);
    let point_zoomed = wheel(&mut h, MouseWheelUnit::Point, 35.0);
    assert!(((point_zoomed.x - center.x) - 2.0 * (before.x - center.x)).abs() < 1.0);
    let point_zoomed_again = wheel(&mut h, MouseWheelUnit::Point, 45.0);
    assert!(((point_zoomed_again.x - center.x) - 4.0 * (before.x - center.x)).abs() < 1.0);

    let page_zoomed = wheel(&mut h, MouseWheelUnit::Page, -3.0);
    assert!((page_zoomed.x - point_zoomed.x).abs() < 1.0);
    let unzoomed = wheel(&mut h, MouseWheelUnit::Point, -50.0);
    assert!((unzoomed.x - before.x).abs() < 1.0);
    let zoomed = wheel(&mut h, MouseWheelUnit::Line, 3.0);
    assert!(((zoomed.x - center.x) - 2.0 * (before.x - center.x)).abs() < 1.0);
    assert_eq!(h.state().0.tick.nearest.as_ref().unwrap().lon, 0.05);

    h.drag_at(center);
    h.step();
    let dragged = center + egui::vec2(30.0, 0.0);
    h.hover_at(dragged);
    h.step();
    assert!(marker(&h).0.x > zoomed.x + 20.0);
    // Release with a stationary pointer so inertia cannot move the camera.
    h.run_steps(2);
    h.drop_at(dragged);
    h.step();
    let last_marker = marker(&h);

    h.state_mut().0.tick.nearest = None;
    h.step();
    // The upstream worker keeps both the marker and camera when there's no new fix.
    assert_eq!(marker(&h), last_marker);
}

#[test]
fn nearest_map_keeps_view_when_my_position_becomes_invalid() {
    use eframe::egui::{self, Event, MouseWheelSource, MouseWheelUnit, TouchPhase};

    let mut st = AppState::new(
        Settings::default(),
        None,
        Box::new(Null),
        Arc::new(PlaneInfoCache::new()),
    );
    st.settings.my_lat = 50.0;
    st.settings.my_lon = 10.0;
    let mut h = Harness::new_ui_state(
        |ui, (st, map): &mut (AppState, MapView)| map.show_nearest(ui, st),
        (st, MapView::without_tiles()),
    );
    let scale = |h: &Harness<'_, (AppState, MapView)>| {
        h.output()
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) if text.galley.job.text == "100Km" => Some(text.pos),
                _ => None,
            })
    };
    let before = scale(&h).expect("initial map scale");
    h.hover_at(pos2(200.0, 200.0));
    h.run();
    h.event(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        phase: TouchPhase::Move,
        source: MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    h.step();
    let zoomed = scale(&h).expect("zoomed map scale");
    assert!(zoomed.x > before.x);

    h.state_mut().0.my_call_changed("?");
    assert!(h.state().0.settings.my_lat.is_nan());
    assert!(h.state().0.settings.my_lon.is_nan());
    for _ in 0..3 {
        h.step();
        assert_eq!(
            scale(&h),
            Some(zoomed),
            "invalid position must preserve the view"
        );
    }

    h.state_mut().0.tick.nearest = Some(PlaneInfo {
        hex: "3C0001".into(),
        int_point: Some(GPoint::new(51.0, 11.0)),
        ..PlaneInfo::default()
    });
    h.step();
    assert_eq!(
        scale(&h),
        Some(zoomed),
        "ignore a fix from the previous station"
    );

    h.state_mut().1 = MapView::without_tiles();
    h.step();
    assert!(scale(&h).is_some(), "initial view must have a finite scale");
}

#[test]
fn frame_with_path_and_planes_does_not_panic() {
    let mut st = AppState::new(
        Settings::default(),
        None,
        Box::new(Null),
        Arc::new(PlaneInfoCache::new()),
    );
    st.settings.gui.map_show_beam_width = true;
    st.path_gen = 1;
    st.paths = vec![entry(PropagationPath::new(
        PathParams {
            lat1: st.settings.my_lat,
            lon1: st.settings.my_lon,
            h1: 100.0,
            lat2: st.settings.dx_lat,
            lon2: st.settings.dx_lon,
            h2: 100.0,
            qrg_ghz: 0.144,
            radius_km: 6371.0 * 1.33,
            f1_clearance: 0.6,
            step_width_m: 1000.0,
            local_obstruction: f64::MIN,
        },
        0.0,
        0.0,
        true,
    ))];
    st.tick.shown = (0..300u32)
        .map(|i| {
            plane(
                &format!("{i:06x}"),
                48.0 + f64::from(i) * 0.01,
                11.0,
                [0, 50, 75, 100][(i % 4) as usize],
            )
        })
        .collect();
    st.selected = Some("000003".into());
    st.pending_zoom = -1;

    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1000.0, 700.0))
        .build_ui_state(
            |ui, (st, maps): &mut (AppState, Maps)| airscout::ui::show(ui, st, maps),
            (st, Maps::without_tiles()),
        );
    h.run();
    h.run();
    assert_eq!(h.state().0.pending_zoom, 0);
    assert_eq!(h.state().0.selected.as_deref(), Some("000003"));
}

fn at(planes: &[PlaneInfo], hex: &str) -> Pos2 {
    // planes sit on a 100 px grid by index
    let i = planes.iter().position(|p| p.hex == hex).unwrap();
    pos2(100.0 * i as f32, 0.0)
}

#[test]
fn hit_plane_respects_icon_radius_plus_slack() {
    let planes = vec![plane("a", 0.0, 0.0, 0), plane("b", 0.0, 0.0, 0)];
    let size = |_: &PlaneInfo| 20.0;
    let reach = 10.0 + HIT_SLACK_PX;
    let proj = |p: &PlaneInfo| at(&planes, &p.hex);
    let hit = |x: f32| hit_plane(&planes, proj, size, pos2(x, 0.0)).map(|p| p.hex.clone());
    assert_eq!(hit(reach - 0.5), Some("a".into()));
    assert_eq!(hit(reach + 0.5), None);
    assert_eq!(hit(100.0 + reach - 0.5), Some("b".into()));
    assert_eq!(hit(50.0), None);
}

#[test]
fn hit_plane_picks_nearest_of_overlapping() {
    let planes = vec![plane("a", 0.0, 0.0, 0), plane("b", 0.0, 0.0, 0)];
    // a at 0, b at 10 px
    let proj = |p: &PlaneInfo| pos2(if p.hex == "a" { 0.0 } else { 10.0 }, 0.0);
    let size = |_: &PlaneInfo| 30.0;
    let hit = |x: f32| hit_plane(&planes, proj, size, pos2(x, 0.0)).map(|p| p.hex.clone());
    assert_eq!(hit(3.0), Some("a".into()));
    assert_eq!(hit(7.0), Some("b".into()));
}

#[test]
fn toggle_selection_on_off_switch() {
    let mut sel = None;
    toggle_selection(&mut sel, "a");
    assert_eq!(sel.as_deref(), Some("a"));
    toggle_selection(&mut sel, "b");
    assert_eq!(sel.as_deref(), Some("b"));
    toggle_selection(&mut sel, "b");
    assert_eq!(sel, None);
}

#[test]
fn fit_due_multi_waits_for_paths_done() {
    use airscout::ui::map::fit_due;
    use as_aircraft::enums::PathMode::{Multi, Single};
    // MULTI: first entries arrived, request not done -> no fit
    assert!(!fit_due(Multi, 1, false, None, 3));
    assert!(!fit_due(Multi, 2, false, None, 3));
    // done -> fit once for this path_gen (entries cover all targets)
    assert!(fit_due(Multi, 2, true, None, 3));
    assert!(!fit_due(Multi, 2, true, Some(3), 3));
    // a new request (new path_gen) fits again once done
    assert!(fit_due(Multi, 2, true, Some(3), 4));
    // nothing to fit without entries
    assert!(!fit_due(Multi, 0, true, None, 3));
    // SINGLE: fit on the first entry, no wait for PathsDone, once per path_gen
    assert!(fit_due(Single, 1, false, None, 1));
    assert!(!fit_due(Single, 1, false, Some(1), 1));
    assert!(!fit_due(Single, 0, false, None, 1));
}

/// `Map_LabelCalls` labels only the checked watchlist locations in MULTI (`MapDlg.cs:5252`);
/// the C# draws no call sign next to a plane icon.
#[test]
fn planes_have_no_call_labels() {
    let mut st = AppState::new(
        Settings::default(),
        None,
        Box::new(Null),
        Arc::new(PlaneInfoCache::new()),
    );
    assert!(st.settings.gui.map_label_calls);
    let mut p = plane("3c0001", st.settings.my_lat, st.settings.my_lon, 50);
    p.call = "TE1ST".into();
    st.tick.shown = vec![p];
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1000.0, 700.0))
        .build_ui_state(
            |ui, (st, view): &mut (AppState, MapView)| view.show(ui, st),
            (st, MapView::without_tiles()),
        );
    h.run();
    h.run();
    let texts: Vec<String> = h
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            eframe::egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect();
    assert!(!texts.is_empty(), "the map draws its scale text");
    assert!(
        !texts.iter().any(|t| t.contains("TE1ST")),
        "plane call label drawn: {texts:?}"
    );
}
