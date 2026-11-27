//! Shared test-side reference model for the GUI 1:1 rebuild.
//!
//! Several integration tests use `mod gui_ref;` and each compiles this module
//! independently, so items that only one test uses would otherwise trip
//! `dead_code` under `-D warnings`. The allow below covers the whole module tree.
#![allow(dead_code)]

pub mod designer;
pub mod fidelity;
pub mod model;
pub mod resx;

/// Load the committed reference for a window from `docs/gui-reference/<window>.json`.
///
/// The directory is located relative to the crate manifest (`crates/airscout`),
/// so it works from any test working directory.
pub fn load(window: &str) -> model::Window {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/gui-reference");
    let path = dir.join(format!("{window}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reference file {} not found: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("reference file {} is not valid JSON: {e}", path.display()))
}
