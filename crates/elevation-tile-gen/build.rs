#[path = "../../packaging/windows/embed-icon.rs"]
mod windows;

use std::path::Path;

fn main() {
    windows::embed_icon(
        Path::new("../../packaging/icons/AirScout-RS.ico"),
        "elevation-tile-gen",
    );
}
