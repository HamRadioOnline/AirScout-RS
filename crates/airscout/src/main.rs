//! AirScout-RS GUI entry point.
#![forbid(unsafe_code)]

use airscout::app::{data_dir, AirScoutApp, RootApp};
use airscout::cleanup::clean_flag;
use as_settings::Settings;

fn main() -> eframe::Result<()> {
    // walkers is built without default features (aws-lc-rs breaks the Windows cross build),
    // so pick the ring provider here. Err means one is already installed: fine.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args: Vec<String> = std::env::args().collect();
    let mut viewport = eframe::egui::ViewportBuilder::default();
    let root = if clean_flag(&args) {
        // The dialog runs before any database is opened.
        viewport = viewport.with_inner_size([560.0, 320.0]);
        RootApp::with_cleanup(Settings::default_path(), data_dir())
    } else {
        let app = AirScoutApp::new(Settings::default_path(), &data_dir());
        let (size, pos) = app.window_geometry();
        if let Some(s) = size {
            viewport = viewport.with_inner_size(s);
        }
        if let Some(p) = pos {
            viewport = viewport.with_position(p);
        }
        RootApp::new(app)
    };
    eframe::run_native(
        "AirScout-RS",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(root))),
    )
}
