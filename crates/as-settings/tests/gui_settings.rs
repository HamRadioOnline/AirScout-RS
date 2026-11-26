use as_aircraft::PlaneCategory;
use as_settings::{Area, GuiSettings, InfoWin, Settings};

#[test]
fn gui_defaults_match_csharp() {
    let a = Area::default();
    assert_eq!(
        (a.min_lat, a.max_lat, a.min_lon, a.max_lon),
        (35.0, 60.0, -15.0, 30.0)
    );

    let g = GuiSettings::default();
    assert_eq!(g.map_provider, "OpenStreetMap");
    assert_eq!(g.map_update_s, 1);
    assert!(g.map_auto_center);
    assert_eq!(g.map_opacity, 1.0);
    assert!(g.map_label_calls);
    assert!(!g.map_show_beam_width);
    assert!(!g.locator_small_letters_for_subsquares);
    assert_eq!(g.planes_filter_min_alt_m, 0);
    assert_eq!(g.planes_filter_max_circumcircle_km, 0.0);
    assert_eq!(g.planes_filter_min_category, PlaneCategory::None);
    assert_eq!(g.planes_icon_size, [16, 24, 36, 48]);
    assert!(!g.alarm_activate);
    assert_eq!(g.alarm_distance_km, 100.0);
    assert!(g.alarm_bring_to_front);
    assert!(g.alarm_play_sound);
    assert!(g.my_calls.is_empty());
    assert_eq!(g.window_size, None);
    assert_eq!(g.window_pos, None);
    assert_eq!(g.side_panel_width, None);
    assert_eq!(g.bottom_panel_height, None);
    assert!(g.show_info_box && g.show_zoom_box && g.show_filter_box && g.show_alarm_box);
    assert_eq!(g.elevation_base_url, as_elevation::DEFAULT_BASE_URL);
    assert!(g.elevation_download);

    let w = InfoWin::default();
    assert!(w.metric && w.position && w.time && w.squint && w.track);
    assert!(w.type_ && w.alt && w.dist);
    assert!(!(w.imperial || w.epsilon || w.speed || w.angle));
    assert!(!(w.my_azimuth || w.my_elevation || w.my_doppler));
    assert!(!(w.dx_azimuth || w.dx_elevation || w.dx_doppler));
    assert_eq!(g.info_win, w);
}

#[test]
fn stage2_file_loads_with_gui_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    std::fs::write(
        &p,
        "my_call = \"DL1ABC\"\nmy_lat = 51.0\nfeed = \"OpenSky\"\n",
    )
    .unwrap();
    let s = Settings::load_or_default(&p).unwrap();
    assert_eq!(s.my_call, "DL1ABC");
    assert_eq!(s.gui, GuiSettings::default());
    assert_eq!(s.area, Area::default());
}

#[test]
fn gui_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.toml");
    let mut s = Settings::default();
    s.gui.map_provider = "Other".into();
    s.gui.map_opacity = 0.5;
    s.gui.planes_filter_min_category = PlaneCategory::Heavy;
    s.gui.my_calls = vec!["DL2ALF".into(), "DL1ABC".into()];
    s.gui.window_size = Some([1280.0, 800.0]);
    s.gui.info_win.speed = true;
    s.area.max_lat = 55.5;
    s.save(&p).unwrap();
    let back = Settings::load_or_default(&p).unwrap();
    assert_eq!(back, s);
}
