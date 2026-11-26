mod common;
use as_elevation::{elevation_color, ElevationModel, ElevationStore};

#[test]
fn tile_exists_true_false() {
    let dir = tempfile::tempdir().unwrap();
    let store = ElevationStore::open(dir.path()).unwrap();
    let tiles = common::fixture_tiles();
    let ((_, index), tile) = tiles
        .iter()
        .find(|((m, _), _)| *m == ElevationModel::Globe)
        .unwrap();
    store
        .insert_or_update_if_newer(ElevationModel::Globe, tile)
        .unwrap();
    assert!(store.tile_exists(ElevationModel::Globe, index).unwrap());
    // another tile of the same model, and the same tile of another model
    assert!(!store.tile_exists(ElevationModel::Globe, "AA00AA").unwrap());
    assert!(!store.tile_exists(ElevationModel::Srtm3, index).unwrap());
}

/// Values worked out by hand from `DEMColorPalette` (`ElevationDatabase.cs:2059-2100`): the
/// value is divided by 100, clamped to 0..100 and looked up in 1000 steps; the colours between
/// two stops are interpolated per channel with `(int)` truncation.
#[test]
fn elevation_color_stops() {
    assert_eq!(elevation_color(0.0), [0, 97, 71]);
    assert_eq!(elevation_color(10_000.0), [255, 255, 255]);
    // 50.0 % lies between the stops 34.69 and 57.14
    assert_eq!(elevation_color(5000.0), [126, 75, 75]);
    // 30.0 % lies between the stops 24.49 and 34.69
    assert_eq!(elevation_color(3000.0), [160, 31, 0]);
    // clamped on both sides
    assert_eq!(elevation_color(-500.0), [0, 97, 71]);
    assert_eq!(elevation_color(1.0e9), [255, 255, 255]);
    // NaN (flat view) is taken as 0
    assert_eq!(elevation_color(f64::NAN), [0, 97, 71]);
}
