//! Options dialog, driven through egui_kittest (no rendering).
use airscout::app::{AppState, Services};
use airscout::services::{PathRequest, WatchItem};
use airscout::ui::options::OptionsTab;
use airscout::ui::{control, dialogs};
use as_aircraft::PlaneInfoCache;
use as_settings::{Secret, Settings};
use eframe::egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

type Restarts = Rc<RefCell<Vec<Settings>>>;

/// The map-layer requests a [`Fake`] got: `load_airports` ids and `resolve_watchlist` calls.
#[derive(Default)]
struct LayerCalls {
    airports: Vec<u64>,
    watchlist: Vec<(u64, Vec<WatchItem>)>,
}

struct Fake(Restarts, Rc<RefCell<LayerCalls>>);

impl Services for Fake {
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
    fn restart_background(&mut self, settings: &Settings, _generation: u64) {
        self.0.borrow_mut().push(settings.clone());
    }
    fn load_airports(&mut self, id: u64) {
        self.1.borrow_mut().airports.push(id);
    }
    fn resolve_watchlist(&mut self, id: u64, items: Vec<WatchItem>) {
        self.1.borrow_mut().watchlist.push((id, items));
    }
}

fn harness(settings: Settings) -> (Harness<'static, AppState>, Restarts) {
    let r = Restarts::default();
    let st = AppState::new(
        settings,
        None,
        Box::new(Fake(r.clone(), Default::default())),
        Arc::new(PlaneInfoCache::new()),
    );
    let h = Harness::new_ui_state(
        |ui, st: &mut AppState| {
            control::show(ui, st);
            dialogs::show(ui.ctx(), st);
        },
        st,
    );
    (h, r)
}

fn open(h: &mut Harness<'static, AppState>) {
    h.get_by_label("Options").click();
    h.run();
    assert!(h.state().options_dlg.is_some());
}

fn draft<'a>(h: &'a mut Harness<'static, AppState>) -> &'a mut Settings {
    &mut h.state_mut().options_dlg.as_mut().unwrap().draft
}

#[test]
fn cancel_discards_edits() {
    let (mut h, restarts) = harness(Settings::default());
    let before = h.state().settings.clone();
    open(&mut h);
    draft(&mut h).area.min_lat = 40.0;
    draft(&mut h).gui.alarm_distance_km = 5.0;
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert_eq!(h.state().settings, before);
    // Cancel still closes the workers that opening the dialog paused (#155): the restart carries
    // the unchanged settings, not the discarded edits.
    assert_eq!(restarts.borrow().len(), 1);
    assert_eq!(restarts.borrow()[0], before);
}

#[test]
fn ok_applies_and_saves() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let (mut h, restarts) = harness(Settings::default());
    h.state_mut().settings_path = Some(path.clone());
    h.state_mut().settings.gui.side_panel_width = Some(333.0);
    open(&mut h);
    draft(&mut h).area.min_lat = 40.0;
    draft(&mut h).gui.alarm_distance_km = 5.0;
    draft(&mut h).gui.map_update_s = 3;
    h.get_by_label("OK").click();
    h.run();
    let st = h.state();
    assert!(st.options_dlg.is_none());
    assert_eq!(st.settings.area.min_lat, 40.0);
    assert_eq!(st.settings.gui.alarm_distance_km, 5.0);
    // live layout survives the apply
    assert_eq!(st.settings.gui.side_panel_width, Some(333.0));
    let r = restarts.borrow();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].area.min_lat, 40.0);
    let saved = Settings::load_or_default(&path).unwrap();
    assert_eq!(saved.area.min_lat, 40.0);
    assert_eq!(saved.gui.map_update_s, 3);
}

#[test]
fn invalid_area_blocks_ok() {
    let (mut h, restarts) = harness(Settings::default());
    open(&mut h);
    draft(&mut h).area.min_lat = 70.0; // above max_lat 60
    h.get_by_label("OK").click();
    h.run();
    let dlg = h.state().options_dlg.as_ref().expect("dialog stays open");
    assert!(dlg.errors.iter().any(|e| e.contains("latitude")));
    assert_eq!(h.state().settings.area.min_lat, 35.0);
    assert!(restarts.borrow().is_empty());
    h.get_by_label_contains("min latitude must be below");
}

#[test]
fn options_disabled_while_playing() {
    let (mut h, _) = harness(Settings::default());
    h.state_mut().play().unwrap();
    h.run();
    let b = h.get_by_role_and_label(Role::Button, "Options");
    assert!(b.accesskit_node().is_disabled());
    assert!(!h.state_mut().open_options_dlg());
    assert!(h.state().options_dlg.is_none());
}

#[test]
fn status_never_contains_key() {
    let (mut h, _) = harness(Settings::default());
    open(&mut h);
    draft(&mut h).gui.thunderforest_api_key = Secret::from("k3y-secret");
    draft(&mut h).gui.map_provider = "OCM Landscape".into();
    h.get_by_label("OK").click();
    h.run();
    let st = h.state();
    assert!(!st.status_line().contains("k3y-secret"));
    assert!(!format!("{:?}", st.settings).contains("k3y-secret"));
}

#[test]
fn every_tab_renders() {
    let (mut h, _) = harness(Settings::default());
    open(&mut h);
    for tab in [
        "Database",
        "Map",
        "Elevation",
        "Path",
        "Planes",
        "Alarm",
        "Network",
        "Track",
        "CAT",
        "Spectrum",
        "Watchlist",
        "Info",
        "General",
    ] {
        h.get_by_role_and_label(Role::Button, tab).click();
        h.run();
    }
    assert_eq!(
        h.state().options_dlg.as_ref().unwrap().tab,
        OptionsTab::General
    );
    h.get_by_role_and_label(Role::SpinButton, "Min. Latitude:");
}

/// Options › Map: the preloader checkbox, the zoom spinner (enabled only with the checkbox) and
/// the tile estimate for the area (default area, zoom 5..=11: 74,916 tiles, the Task 1 golden).
#[test]
fn map_tab_preloader_controls() {
    let (mut h, _) = harness(Settings::default());
    open(&mut h);
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Map;
    h.run();
    let estimate =
        "Preloads about 74916 tiles from www.airscout.eu (OpenStreetMap only); off by default.";
    h.get_by_label(estimate);
    assert!(h
        .get_by_role_and_label(Role::SpinButton, "down to \nZoom Level:")
        .accesskit_node()
        .is_disabled());
    h.get_by_label("Enable Map Preloading").click();
    h.run();
    assert!(draft(&mut h).gui.map_preloader_enabled);
    assert!(!h
        .get_by_role_and_label(Role::SpinButton, "down to \nZoom Level:")
        .accesskit_node()
        .is_disabled());
    draft(&mut h).gui.map_preloader_max_zoom = 5;
    h.run();
    let n5 = {
        let a = Settings::default().area;
        as_maps::preload_tile_count(
            &as_geo::latlon::GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon),
            5,
            5,
        )
    };
    h.get_by_label(&format!(
        "Preloads about {n5} tiles from www.airscout.eu (OpenStreetMap only); off by default."
    ));
}

#[test]
fn planes_alt_order_validated() {
    use airscout::ui::options::{validate, OptionsDlg};

    let mut dlg = OptionsDlg::new(&Settings::default());
    assert!(dlg.try_select_tab(OptionsTab::Planes));
    dlg.draft.planes_max_alt_m = dlg.draft.planes_min_alt_m - 1;
    assert!(validate(&dlg.draft)
        .iter()
        .any(|error| error == "MaxAlt must be greater than MinAlt."));
    // A repeated refusal shows the message once.
    for _ in 0..2 {
        assert!(!dlg.try_select_tab(OptionsTab::Map));
        assert_eq!(dlg.tab, OptionsTab::Planes);
        assert_eq!(dlg.status_texts().1, "MaxAlt must be greater than MinAlt.");
    }

    // Fixed and left: the strip is idle again.
    dlg.draft.planes_max_alt_m = dlg.draft.planes_min_alt_m;
    assert!(validate(&dlg.draft).is_empty());
    assert!(dlg.try_select_tab(OptionsTab::Map));
    assert_eq!(dlg.status_texts().0, "Status");
}

#[test]
fn map_defaults_match_csharp() {
    let s = Settings::default();
    // `Settings.settings:332,386,493,721-732`
    assert!(!s.gui.map_show_distances);
    assert!(!s.gui.map_show_locators);
    assert!(s.gui.map_show_watchlist);
    assert!(s.gui.map_show_airports);
    assert!(s.web.map_small_markers);
    assert_eq!(s.gui.gauge_width, 175);
    assert_eq!(s.gui.gauge_forecolor, "Black");
    // Additive: a settings file without the new keys keeps the defaults.
    let old: Settings = toml::from_str("[gui]\nmap_label_calls = false\n").unwrap();
    assert!(!old.gui.map_label_calls);
    assert!(!old.gui.map_show_distances && !old.gui.map_show_locators);
    assert!(old.gui.map_show_watchlist && old.gui.map_show_airports);
    assert_eq!(
        (old.gui.gauge_width, old.gui.gauge_forecolor.as_str()),
        (175, "Black")
    );
}

/// JO50AA and JO60AA centres.
fn stations(s: &mut Settings) {
    (s.my_call, s.my_lat, s.my_lon) = ("TE1ST".into(), 50.0208, 10.0417);
    (s.dx_call, s.dx_lat, s.dx_lon) = ("TE2ST".into(), 50.0208, 12.0417);
}

fn path_entry(s: &Settings) -> airscout::app::PathEntry {
    use as_propagation::{PathParams, PropagationPath};
    airscout::app::PathEntry {
        index: 0,
        call: s.dx_call.clone(),
        path: PropagationPath::new(
            PathParams {
                lat1: s.my_lat,
                lon1: s.my_lon,
                h1: 100.0,
                lat2: s.dx_lat,
                lon2: s.dx_lon,
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
        ),
        terrain: Vec::new(),
        my_gain_dbi: 10.0,
        dx_gain_dbi: 12.0,
    }
}

/// One map frame of `settings` with a path, a watchlist mark and an airport; what it painted.
fn drawn(settings: &Settings, multi: bool) -> airscout::ui::map::DrawnLayers {
    use airscout::services::{AirportMark, WatchMark};
    use airscout::ui::map::MapView;
    use as_geo::latlon::GPoint;
    let mut st = AppState::new(
        settings.clone(),
        None,
        Box::new(Fake(Default::default(), Default::default())),
        Arc::new(PlaneInfoCache::new()),
    );
    if multi {
        st.control_tab = airscout::app::ControlTab::Multi;
    }
    st.path_gen = 1;
    st.paths = vec![path_entry(settings)];
    st.watch_marks = vec![WatchMark {
        call: "TE2ST".into(),
        pos: GPoint::new(50.0208, 12.0417),
        from_user: true,
        checked: true,
    }];
    st.airports = vec![AirportMark {
        name: "Test Field".into(),
        iata: "TST".into(),
        icao: "ZZZZ".into(),
        pos: GPoint::new(50.5, 11.0),
    }];
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1000.0, 700.0))
        .build_ui_state(
            |ui, (st, view): &mut (AppState, MapView)| view.show(ui, st),
            (st, MapView::without_tiles()),
        );
    h.run();
    h.state().1.drawn()
}

#[test]
fn toggles_reach_renderer() {
    use airscout::ui::map::OverlayFlags;
    let mut base = Settings::default();
    stations(&mut base);
    let (mut h, _) = harness(base.clone());
    open(&mut h);
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Map;
    h.run();
    for label in [
        "Distance Circles",
        "Locator Grid",
        "Watchlist Locations",
        "Airports",
        "Beamwidths",
        "Use Small Markers for all Path Marks",
        "Show Labels with Callsign at Locations",
        "Show large Tracking Display on map",
    ] {
        h.get_by_label(label).click();
        h.run();
    }
    h.get_by_label("OK").click();
    h.run();
    let flipped = h.state().settings.clone();
    let (a, b) = (
        OverlayFlags::from_settings(&base),
        OverlayFlags::from_settings(&flipped),
    );
    assert_eq!(
        b,
        OverlayFlags {
            distances: !a.distances,
            locators: !a.locators,
            watchlist: !a.watchlist,
            airports: !a.airports,
            beams: !a.beams,
            small_markers: !a.small_markers,
            label_calls: !a.label_calls,
        }
    );
    assert!(!flipped.gui.tracking_gauges_show);
    assert!(!flipped.web.map_small_markers);

    // Defaults: no rings, no grid, watchlist and airports shown, small markers (no hot marks).
    let d = drawn(&base, false);
    assert_eq!((d.rings, d.grid_cells), (0, 0));
    assert_eq!((d.watch_marks, d.airports), (1, 1));
    assert_eq!(d.hot_markers, 0);
    assert!(d.beams == 0);
    // Flipped: rings and grid drawn, watchlist and airports hidden, the hot marks back.
    let d = drawn(&flipped, false);
    assert!(d.rings > 0, "{d:?}");
    assert!(d.grid_cells > 0, "{d:?}");
    assert_eq!((d.watch_marks, d.airports), (0, 0));
    assert_eq!(d.hot_markers, 3);
    assert!(d.beams > 0, "{d:?}");
    // Each toggle alone reaches its layer.
    let mut s = base.clone();
    s.gui.map_show_distances = true;
    assert!(drawn(&s, false).rings > 0);
    let mut s = base.clone();
    s.gui.map_show_locators = true;
    assert!(drawn(&s, false).grid_cells > 0);
    let mut s = base.clone();
    s.gui.map_show_watchlist = false;
    assert_eq!(drawn(&s, false).watch_marks, 0);
    let mut s = base.clone();
    s.gui.map_show_airports = false;
    assert_eq!(drawn(&s, false).airports, 0);
    // Watchlist labels: checked items in MULTI with `Map_LabelCalls` (`MapDlg.cs:5252`).
    assert_eq!(drawn(&base, true).watch_labels, 1);
    let mut s = base.clone();
    s.gui.map_label_calls = false;
    assert_eq!(drawn(&s, true).watch_labels, 0);
    assert_eq!(drawn(&base, false).watch_labels, 0);
}

#[test]
fn map_layers_load_off_thread() {
    use airscout::services::{AirportMark, ServiceEvent, WatchMark};
    use as_geo::latlon::GPoint;
    use as_settings::WatchlistItem;
    let calls = Rc::new(RefCell::new(LayerCalls::default()));
    let mut s = Settings::default();
    stations(&mut s);
    s.watchlist.0.push(WatchlistItem {
        call: "TE2ST".into(),
        loc: "JO60AA".into(),
        checked: true,
        ..Default::default()
    });
    let mut st = AppState::new(
        s,
        None,
        Box::new(Fake(Default::default(), calls.clone())),
        Arc::new(PlaneInfoCache::new()),
    );
    st.sync_map_layers();
    st.sync_map_layers();
    let (airport_id, watch_id) = {
        let c = calls.borrow();
        assert_eq!(c.airports.len(), 1, "airports load once");
        assert_eq!(c.watchlist.len(), 1, "watchlist resolves once per change");
        assert_eq!(
            c.watchlist[0].1,
            vec![("TE2ST".to_string(), "JO60AA".to_string(), true)]
        );
        (c.airports[0], c.watchlist[0].0)
    };
    let airport = AirportMark {
        name: "Test Field".into(),
        iata: "TST".into(),
        icao: "ZZZZ".into(),
        pos: GPoint::new(50.5, 11.0),
    };
    let mark = WatchMark {
        call: "TE2ST".into(),
        pos: GPoint::new(50.0208, 12.0417),
        from_user: false,
        checked: true,
    };
    // A stale answer is dropped.
    st.on_event(ServiceEvent::Airports {
        id: airport_id + 1,
        result: Ok(vec![airport.clone()]),
    });
    st.on_event(ServiceEvent::WatchMarks {
        id: watch_id + 1,
        marks: vec![mark.clone()],
    });
    assert!(st.airports.is_empty() && st.watch_marks.is_empty());
    st.on_event(ServiceEvent::Airports {
        id: airport_id,
        result: Ok(vec![airport.clone()]),
    });
    st.on_event(ServiceEvent::WatchMarks {
        id: watch_id,
        marks: vec![mark.clone()],
    });
    assert_eq!(st.airports, vec![airport]);
    assert_eq!(st.watch_marks, vec![mark]);

    // A changed watchlist resolves again; switched off, it asks for nothing.
    st.settings.watchlist.0[0].checked = false;
    st.sync_map_layers();
    assert_eq!(calls.borrow().watchlist.len(), 2);
    st.settings.gui.map_show_watchlist = false;
    st.settings.watchlist.0[0].loc = "JO60AB".into();
    st.sync_map_layers();
    assert_eq!(calls.borrow().watchlist.len(), 2);
    // A failed airport load shows in the status line.
    let mut s = Settings::default();
    s.gui.map_show_airports = true;
    let calls = Rc::new(RefCell::new(LayerCalls::default()));
    let mut st = AppState::new(
        s,
        None,
        Box::new(Fake(Default::default(), calls.clone())),
        Arc::new(PlaneInfoCache::new()),
    );
    st.sync_map_layers();
    let id = calls.borrow().airports[0];
    st.on_event(ServiceEvent::Airports {
        id,
        result: Err("no aircraft database".into()),
    });
    assert!(st.airports.is_empty());
    assert!(st.status.contains("no aircraft database"), "{}", st.status);
    // Airports off at startup: nothing is loaded until they are switched on.
    let mut s = Settings::default();
    s.gui.map_show_airports = false;
    let calls = Rc::new(RefCell::new(LayerCalls::default()));
    let mut st = AppState::new(
        s,
        None,
        Box::new(Fake(Default::default(), calls.clone())),
        Arc::new(PlaneInfoCache::new()),
    );
    st.sync_map_layers();
    assert!(calls.borrow().airports.is_empty());
    st.settings.gui.map_show_airports = true;
    st.sync_map_layers();
    assert_eq!(calls.borrow().airports.len(), 1);
}

#[test]
fn airports_reload_after_options_close() {
    // `StartAllBackgroundWorkers` reruns `bw_AirportMapper` whenever the Options dialog closes
    // (`MapDlg.cs:2166-2172, 3979`), so airports filled in meanwhile appear without a restart.
    let calls = Rc::new(RefCell::new(LayerCalls::default()));
    let mut s = Settings::default();
    s.gui.map_show_airports = true;
    let mut st = AppState::new(
        s,
        None,
        Box::new(Fake(Default::default(), calls.clone())),
        Arc::new(PlaneInfoCache::new()),
    );
    st.sync_map_layers();
    st.sync_map_layers();
    assert_eq!(calls.borrow().airports.len(), 1);
    st.options_closed();
    st.sync_map_layers();
    st.sync_map_layers();
    let c = calls.borrow();
    assert_eq!(c.airports.len(), 2, "one reload per close");
    assert!(c.airports[1] > c.airports[0], "the reload has a new id");
}

/// A synthetic `w` x `h` PNG, one colour, written to `dir/name`.
fn write_png(dir: &std::path::Path, name: &str, w: u32, h: u32) -> std::path::PathBuf {
    let path = dir.join(name);
    image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]))
        .save(&path)
        .unwrap();
    path
}

/// The main map of `settings`, 1000 x 700, with no tiles.
fn overlay_map(settings: Settings) -> Harness<'static, (AppState, airscout::ui::map::MapView)> {
    use airscout::ui::map::MapView;
    let st = AppState::new(
        settings,
        None,
        Box::new(Fake(Default::default(), Default::default())),
        Arc::new(PlaneInfoCache::new()),
    );
    let mut h = Harness::builder()
        .with_size(eframe::egui::vec2(1000.0, 700.0))
        .build_ui_state(
            |ui, (st, view): &mut (AppState, MapView)| view.show(ui, st),
            (st, MapView::without_tiles()),
        );
    settle_overlay(&mut h);
    h
}

/// Step until the off-thread decode of the overlay has been taken up.
fn settle_overlay(h: &mut Harness<'static, (AppState, airscout::ui::map::MapView)>) {
    for _ in 0..400 {
        h.step();
        if !h.state().1.user_overlay_pending() {
            h.step();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the overlay decode did not finish");
}

/// Overlay settings: on, `path`, the C# default bounds (49..59 N, 14.5 W..6 E).
fn overlay_settings(path: &std::path::Path) -> Settings {
    let mut s = Settings::default();
    s.gui.overlay_enabled = true;
    s.gui.overlay_filename = Some(path.to_path_buf());
    s
}

#[test]
fn overlay_defaults_match_csharp() {
    // `Settings.settings:781-806`
    let g = Settings::default().gui;
    assert!(!g.overlay_enabled);
    assert_eq!(g.overlay_filename, None);
    assert_eq!(g.overlay_opacity, 0.5);
    assert_eq!(
        (
            g.overlay_min_lat,
            g.overlay_max_lat,
            g.overlay_min_lon,
            g.overlay_max_lon
        ),
        (49.0, 59.0, -14.5, 6.0)
    );
    // Additive: an old settings file keeps the defaults.
    let old: Settings = toml::from_str("[gui]\nmap_label_calls = false\n").unwrap();
    assert_eq!(old.gui.overlay_opacity, 0.5);
    assert_eq!(old.gui.overlay_min_lon, -14.5);
    // Round trip with a file name.
    let mut s = Settings::default();
    s.gui.overlay_filename = Some("overlay.png".into());
    let back: Settings = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
    assert_eq!(back.gui.overlay_filename, s.gui.overlay_filename);
    // The file name (a local path) stays out of Debug output.
    s.gui.overlay_filename = Some("/home/TE1ST/overlay.png".into());
    assert!(!format!("{:?}", s.gui).contains("TE1ST"));
}

#[test]
fn overlay_load_and_draw() {
    use airscout::overlay_image::OverlayImage;
    use as_geo::latlon::GRect;
    use eframe::egui::{pos2, vec2, Rect};
    use walkers::{lon_lat, MapMemory, MercatorProjection, Projector};

    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "overlay.png", 4, 2);
    let bounds = GRect::new(49.0, -14.5, 59.0, 6.0);
    let ctx = eframe::egui::Context::default();
    let img = OverlayImage::load(&path, bounds, &ctx).expect("loads");
    assert_eq!(img.texture().size(), [4, 2]);
    assert_eq!(img.bounds(), bounds);

    // The image spans the projected corners: north-west top left, south-east bottom right.
    let mut memory = MapMemory::default();
    memory.set_zoom(5.0).unwrap();
    let view = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 700.0));
    let projector = Projector::new(view, &MercatorProjection, &memory, lon_lat(-4.0, 54.0));
    let nw = projector.project(lon_lat(-14.5, 59.0)).to_pos2();
    let se = projector.project(lon_lat(6.0, 49.0)).to_pos2();
    let rect = img.screen_rect(&projector);
    assert!((rect.min - nw).length() < 1e-3, "{rect:?} {nw:?}");
    assert!((rect.max - se).length() < 1e-3, "{rect:?} {se:?}");
    assert!(rect.width() > 0.0 && rect.height() > 0.0);

    // Load failures: a missing file, a non-image, an oversized image, a reversed box.
    let missing = OverlayImage::load(&dir.path().join("none.png"), bounds, &ctx);
    let corrupt = dir.path().join("corrupt.png");
    std::fs::write(&corrupt, b"not an image").unwrap();
    let wide = write_png(dir.path(), "wide.png", 8193, 1);
    let huge = dir.path().join("huge.png");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(65 * 1024 * 1024)
        .unwrap();
    for (what, result) in [
        ("missing", missing),
        ("corrupt", OverlayImage::load(&corrupt, bounds, &ctx)),
        ("wide", OverlayImage::load(&wide, bounds, &ctx)),
        ("huge", OverlayImage::load(&huge, bounds, &ctx)),
        (
            "reversed",
            OverlayImage::load(&path, GRect::new(59.0, -14.5, 49.0, 6.0), &ctx),
        ),
    ] {
        let Err(e) = result else {
            panic!("{what} loaded")
        };
        assert!(e.starts_with("User supplied overlay:"), "{what}: {e}");
        assert!(!e.contains(&*dir.path().to_string_lossy()), "{what}: {e}");
    }

    // The main map loads the file off the UI thread and paints it between zoom 1 and 20.
    let mut h = overlay_map(overlay_settings(&path));
    let view = &h.state().1;
    let shown = view.user_overlay().expect("overlay committed");
    assert_eq!(shown.texture().size(), [4, 2]);
    assert_eq!(shown.bounds(), bounds);
    assert!(view.drawn().user_overlay);
    assert!(h.state().0.status.is_empty(), "{}", h.state().0.status);
    // No reload while the key is unchanged.
    let id = shown.texture().id();
    h.run();
    assert_eq!(h.state().1.user_overlay().unwrap().texture().id(), id);
    assert!(!h.state().1.user_overlay_pending());
    // Below `Map_UserSuppliedOverlay_FromZoomLevel` (1) it is kept but not painted.
    h.state_mut().0.pending_zoom = -30;
    h.run();
    assert!(h.state().0.map_zoom < 1.0, "{}", h.state().0.map_zoom);
    assert!(!h.state().1.drawn().user_overlay);
    assert!(h.state().1.user_overlay().is_some());
    h.state_mut().0.pending_zoom = 3;
    h.run();
    assert!(h.state().1.drawn().user_overlay);
}

#[test]
fn corrupt_overlay_keeps_committed() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "overlay.png", 4, 2);
    let mut h = overlay_map(overlay_settings(&path));
    let id = h
        .state()
        .1
        .user_overlay()
        .expect("committed")
        .texture()
        .id();

    let corrupt = dir.path().join("corrupt.png");
    std::fs::write(&corrupt, b"not an image").unwrap();
    for replacement in [corrupt, dir.path().join("missing.png")] {
        h.state_mut().0.status.clear();
        h.state_mut().0.settings.gui.overlay_filename = Some(replacement);
        settle_overlay(&mut h);
        let (st, view) = h.state();
        assert_eq!(view.user_overlay().expect("kept").texture().id(), id);
        assert!(view.drawn().user_overlay);
        assert!(
            st.status.starts_with("User supplied overlay:"),
            "{}",
            st.status
        );
        assert!(!st.status.contains(&*dir.path().to_string_lossy()));
    }
    // A failed key is not retried every frame.
    h.state_mut().0.status.clear();
    h.run();
    assert!(h.state().0.status.is_empty());
    assert!(!h.state().1.user_overlay_pending());

    // A good replacement releases the old texture.
    let other = write_png(dir.path(), "other.png", 3, 3);
    h.state_mut().0.settings.gui.overlay_filename = Some(other);
    settle_overlay(&mut h);
    let shown = h.state().1.user_overlay().unwrap();
    assert_eq!(shown.texture().size(), [3, 3]);
    assert_ne!(shown.texture().id(), id);
}

#[test]
fn reversed_bounds_keeps_committed() {
    use as_geo::latlon::GRect;
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "overlay.png", 4, 2);
    let mut h = overlay_map(overlay_settings(&path));
    let id = h
        .state()
        .1
        .user_overlay()
        .expect("committed")
        .texture()
        .id();
    for (min_lat, max_lat, min_lon, max_lon) in [
        (59.0, 49.0, -14.5, 6.0),
        (49.0, 59.0, 6.0, -14.5),
        (49.0, 49.0, -14.5, 6.0),
        (49.0, 95.0, -14.5, 6.0),
    ] {
        h.state_mut().0.status.clear();
        let g = &mut h.state_mut().0.settings.gui;
        (g.overlay_min_lat, g.overlay_max_lat) = (min_lat, max_lat);
        (g.overlay_min_lon, g.overlay_max_lon) = (min_lon, max_lon);
        settle_overlay(&mut h);
        let (st, view) = h.state();
        let kept = view.user_overlay().expect("kept");
        assert_eq!(kept.texture().id(), id);
        assert_eq!(kept.bounds(), GRect::new(49.0, -14.5, 59.0, 6.0));
        assert!(
            st.status.starts_with("User supplied overlay:"),
            "{min_lat} {max_lat} {min_lon} {max_lon}: {}",
            st.status
        );
        // The stored bounds are not rewritten.
        assert_eq!(st.settings.gui.overlay_max_lat, max_lat);
    }
    // A valid new box reloads.
    h.state_mut().0.settings.gui.overlay_max_lat = 60.0;
    h.state_mut().0.settings.gui.overlay_min_lat = 50.0;
    settle_overlay(&mut h);
    assert_eq!(
        h.state().1.user_overlay().unwrap().bounds(),
        GRect::new(50.0, -14.5, 60.0, 6.0)
    );
}

#[test]
fn toggle_off_removes_texture() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "overlay.png", 4, 2);
    let mut h = overlay_map(overlay_settings(&path));
    assert!(h.state().1.user_overlay().is_some());

    h.state_mut().0.settings.gui.overlay_enabled = false;
    settle_overlay(&mut h);
    assert!(h.state().1.user_overlay().is_none());
    assert!(!h.state().1.drawn().user_overlay);
    // An empty file name removes it too, without an error.
    h.state_mut().0.settings.gui.overlay_enabled = true;
    settle_overlay(&mut h);
    assert!(h.state().1.user_overlay().is_some());
    h.state_mut().0.settings.gui.overlay_filename = None;
    settle_overlay(&mut h);
    assert!(h.state().1.user_overlay().is_none());
    assert!(h.state().0.status.is_empty(), "{}", h.state().0.status);

    // Off from the start: nothing is loaded.
    let mut s = overlay_settings(&path);
    s.gui.overlay_enabled = false;
    let h = overlay_map(s);
    assert!(h.state().1.user_overlay().is_none());
    assert!(!h.state().1.drawn().user_overlay);
}

#[test]
fn overlay_options_reach_renderer() {
    let data = tempfile::tempdir().unwrap();
    let tmp = data.path().join("Tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let png = write_png(&tmp, "overlay.png", 4, 2);
    std::fs::write(tmp.join("notes.txt"), b"synthetic").unwrap();
    let (mut h, _) = harness(Settings::default());
    h.state_mut().data_dir = Some(data.path().into());
    open(&mut h);
    h.state_mut().options_dlg.as_mut().unwrap().tab = OptionsTab::Map;
    h.run();
    h.get_by_label("User Supplied").click();
    h.run();
    // `btn_Options_UserSupplied_Filename`: the image files of the Tmp listing (Decision 7).
    h.get_by_role_and_label(Role::Button, "Select").click();
    h.run();
    assert!(h.query_by_label("notes.txt").is_none());
    h.get_by_label("overlay.png").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Use").click();
    h.run();
    assert_eq!(draft(&mut h).gui.overlay_filename.as_deref(), Some(&*png));
    // Cancel in the picker keeps the draft.
    h.get_by_role_and_label(Role::Button, "Select").click();
    h.run();
    h.get_all_by_role_and_label(Role::Button, "Cancel")
        .last()
        .unwrap()
        .click();
    h.run();
    assert_eq!(draft(&mut h).gui.overlay_filename.as_deref(), Some(&*png));
    h.get_by_label("OK").click();
    h.run();
    let g = &h.state().settings.gui;
    assert!(g.overlay_enabled);
    assert_eq!(g.overlay_filename.as_deref(), Some(&*png));
}
