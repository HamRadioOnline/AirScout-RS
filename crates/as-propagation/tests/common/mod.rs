#![allow(dead_code)]
use as_elevation::{DbStatus, ElevationModel, ElevationStore, ElevationTile};
use as_geo::latlon::GRect;
use std::io::Read;
use std::sync::Arc;

/// Elevation store with the GLOBE fixture tiles, complete, bounds 50..52/10..14 (like the harness).
pub fn fixture_store(dir: &std::path::Path) -> Arc<ElevationStore> {
    let store = ElevationStore::open(dir).unwrap();
    let base = as_golden::repo_root().join("tests/fixtures/elevation/GLOBE/JO");
    let mut tiles = Vec::new();
    for e in std::fs::read_dir(base).unwrap() {
        let mut z = zip::ZipArchive::new(std::fs::File::open(e.unwrap().path()).unwrap()).unwrap();
        for i in 0..z.len() {
            let mut s = String::new();
            z.by_index(i).unwrap().read_to_string(&mut s).unwrap();
            tiles.push(ElevationTile::from_loc_json(&s).unwrap());
        }
    }
    store
        .insert_tiles_atomic(ElevationModel::Globe, &tiles)
        .unwrap();
    store.set_bounds(GRect::new(50.0, 10.0, 52.0, 14.0));
    store.set_status_bit(ElevationModel::Globe, DbStatus::COMPLETE);
    Arc::new(store)
}
