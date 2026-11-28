//! AirScout-RS GUI entry point.
#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

use airscout::app::{data_dir, AirScoutApp, RootApp};
use airscout::cleanup::clean_flag;
use as_settings::Settings;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--version") => {
            println!("airscout {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help") => {
            println!(concat!(
                "Usage: airscout [OPTIONS]\n\n",
                "  --clean    Open the data cleanup dialog\n",
                "  --help     Print help\n",
                "  --version  Print version"
            ));
            return Ok(());
        }
        _ => {}
    }

    // walkers is built without default features (aws-lc-rs breaks the Windows cross build),
    // so pick the ring provider here. Err means one is already installed: fine.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut viewport = egui::ViewportBuilder::default().with_icon(egui::IconData {
        rgba: include_bytes!("../assets/app-icon.rgba").to_vec(),
        width: 32,
        height: 32,
    });
    let root = if clean_flag(&args) {
        // The dialog runs before any database is opened.
        viewport = viewport.with_inner_size([560.0, 320.0]);
        RootApp::with_cleanup(Settings::default_path(), data_dir())
    } else {
        let app = AirScoutApp::new(Settings::default_path(), &data_dir());
        let (size, pos) = app.window_geometry();
        // The default client is 1008×730, plus the Rust-only menu bar (D-002).
        let size = size.unwrap_or([1008.0, 730.0 + airscout::ui::MENU_HEIGHT]);
        viewport = viewport.with_inner_size(size);
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
