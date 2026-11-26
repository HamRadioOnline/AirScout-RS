//! Bottom panel: plane list, nearest box and path profile, driven through egui_kittest.
use airscout::app::PathEntry;
use airscout::app::{AppState, BottomTab, Services};
use airscout::services::PathRequest;
use airscout::ui::map::MapView;
use airscout::ui::planes::{plane_rows, show as show_planes};
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache};
use as_propagation::{PathParams, PropagationPath};
use as_settings::Settings;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::sync::Arc;

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

fn state() -> AppState {
    AppState::new(
        Settings::default(),
        None,
        Box::new(Null),
        Arc::new(PlaneInfoCache::new()),
    )
}

fn plane(hex: &str, potential: i32, int_qrb: f64) -> PlaneInfo {
    PlaneInfo {
        hex: hex.into(),
        call: format!("CALL{hex}"),
        potential,
        int_qrb,
        alt_ft: 30_000.0,
        speed_kts: 400.0,
        category: PlaneCategory::Heavy,
        model: "A320".into(),
        ..PlaneInfo::default()
    }
}

fn three(st: &mut AppState) {
    // Insertion order differs from the expected order.
    st.tick.shown = vec![
        plane("a", 50, 10.0),
        plane("b", 100, 30.0),
        plane("c", 100, 20.0),
    ];
}

fn harness(st: AppState) -> Harness<'static, AppState> {
    Harness::new_ui_state(|ui, st: &mut AppState| show_planes(ui, st), st)
}

#[test]
fn rows_sorted_by_potential_then_distance() {
    let mut st = state();
    three(&mut st);
    let rows = plane_rows(&st.tick);
    let hexes: Vec<&str> = rows.iter().map(|r| r.hex.as_str()).collect();
    assert_eq!(hexes, ["c", "b", "a"]);
    assert_eq!(rows[0].cat, "H");
    assert_eq!(rows[0].alt_m, 9143); // 30000 ft, truncated like the map tooltip
    assert_eq!(rows[0].minutes, None);
}

#[test]
fn plane_list_sorted_by_potential() {
    let mut st = state();
    three(&mut st);
    let mut h = harness(st);
    h.run();
    let y = |h: &Harness<'_, AppState>, l: &str| h.get_by_label(l).rect().min.y;
    assert!(y(&h, "CALLc") < y(&h, "CALLb"));
    assert!(y(&h, "CALLb") < y(&h, "CALLa"));
    h.get_by_label("3 planes");
}

#[test]
fn clicking_row_selects_plane() {
    let mut st = state();
    three(&mut st);
    let mut h = harness(st);
    h.run();
    h.get_by_label("CALLb").click();
    h.run();
    assert_eq!(h.state().selected.as_deref(), Some("b"));
    h.get_by_label("CALLb").click();
    h.run();
    assert_eq!(h.state().selected, None);
}

#[test]
fn empty_list_shows_hint() {
    let mut h = harness(state());
    h.run();
    h.get_by_label("No planes in range");
    h.get_by_label("0 planes");
}

#[test]
fn nearest_box_shows_nearest_plane() {
    let mut st = state();
    three(&mut st);
    st.tick.nearest = Some(PlaneInfo {
        angle: std::f64::consts::FRAC_PI_2,
        ..plane("n", 75, 12.0)
    });
    let mut h = harness(st);
    h.run();
    h.get_by_label_contains("Nearest: CALLn");
    h.get_by_label_contains("Angle: 90");
}

#[test]
fn bottom_panel_frame_with_profile_does_not_panic() {
    let mut st = state();
    let path = PropagationPath::new(
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
    );
    let n = path.info_points().unwrap().len();
    st.paths = vec![PathEntry {
        index: 0,
        call: "DX".into(),
        path,
        terrain: vec![300.0; n],
        my_gain_dbi: 10.0,
        dx_gain_dbi: 10.0,
    }];
    st.tick.chart_hi = vec![airscout::pipeline::ChartPlane {
        dist_km: 5.0,
        alt_m: 9000.0,
        call: "HI1".into(),
    }];
    st.tick.chart_lo = vec![airscout::pipeline::ChartPlane {
        dist_km: 8.0,
        alt_m: 3000.0,
        call: "LO1".into(),
    }];
    st.bottom_tab = BottomTab::Profile;
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1000.0, 700.0))
        .build_ui_state(
            |ui, (st, map): &mut (AppState, MapView)| airscout::ui::show(ui, st, map),
            (st, MapView::without_tiles()),
        );
    h.run();
    h.get_by_label("Path Profile");
    assert!(h.state().0.settings.gui.bottom_panel_height.is_some());
    h.get_by_label("Planes").click();
    h.run();
    assert_eq!(h.state().0.bottom_tab, BottomTab::Planes);
}
