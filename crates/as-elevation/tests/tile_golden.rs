mod common;
use as_elevation::{ElevationError, ElevationModel, ElevationTile, ELV_MISSING, TILE_MISSING};

#[test]
fn get_elevation_matches_original() {
    let tiles = common::fixture_tiles();
    let g = as_golden::load("elevation_tile.json");
    let mut n = 0;
    for c in g.by_fn("get_elevation") {
        let key = (common::model_from(&c.s("model")), c.s("tile"));
        let t = tiles
            .get(&key)
            .unwrap_or_else(|| panic!("fixture tile {key:?} missing"));
        let r = t.get_elevation(c.f("lat"), c.f("lon"));
        assert_eq!(Some(r as i64), c.result.as_i64(), "{c:?}");
        n += 1;
    }
    assert!(n > 10_000, "too few cases: {n}");
}

#[test]
fn flags_match_original() {
    assert_eq!(ELV_MISSING, -32768);
    assert_eq!(TILE_MISSING, -32767);
}

#[test]
fn blob_roundtrip() {
    let tiles = common::fixture_tiles();
    for t in tiles.values().take(50) {
        let b = t.to_blob();
        let back = ElevationTile::from_blob(&t.index, t.rows, t.columns, &b).unwrap();
        assert_eq!(back.elv, t.elv);
        assert_eq!(
            back.get_elevation(t.base_lat() + 0.01, t.base_lon() + 0.01),
            t.get_elevation(t.base_lat() + 0.01, t.base_lon() + 0.01)
        );
    }
}

#[test]
fn blob_with_wrong_length_is_err() {
    let r = ElevationTile::from_blob("JO50AA", 5, 10, &[0u8; 7]);
    assert!(matches!(r, Err(ElevationError::Corrupt(_))), "{r:?}");
}

#[test]
fn malformed_json_is_err() {
    let full = std::fs::read_to_string(common::fixture_dir().join("SRTM3/JO/JO50AA.loc")).unwrap();
    let r = ElevationTile::from_loc_json(&full[..full.len() / 2]);
    assert!(matches!(r, Err(ElevationError::Parse(_))), "{r:?}");
}

#[test]
fn elv_shape_mismatch_is_err() {
    let json = r#"{"TileIndex":"JO50AA","Rows":5,"Columns":10,"Elv":[[1,2,3]],"LastUpdated":"2018-02-27T08:26:32Z"}"#;
    assert!(matches!(
        ElevationTile::from_loc_json(json),
        Err(ElevationError::Corrupt(_))
    ));
}

#[test]
fn model_constants_match_original() {
    assert_eq!(ElevationModel::Srtm1.default_step_width_m(), 30.0);
    assert_eq!(ElevationModel::Aster1.default_step_width_m(), 30.0);
    assert_eq!(ElevationModel::Srtm3.default_step_width_m(), 90.0);
    assert_eq!(ElevationModel::Aster3.default_step_width_m(), 90.0);
    assert_eq!(ElevationModel::Globe.default_step_width_m(), 1000.0);
    assert_eq!(ElevationModel::Aster3.average_period(), 5);
    assert_eq!(ElevationModel::Srtm3.average_period(), 0);
    assert_eq!(ElevationModel::Srtm3.dir_name(), "SRTM3");
}

#[test]
fn nan_coordinate_is_missing_like_csharp() {
    // C#: (int)NaN == int.MinValue -> IndexOutOfRange -> ElvMissingFlag. Rust `as` gives 0.
    let tiles = common::fixture_tiles();
    let t = tiles.values().next().unwrap();
    assert_eq!(t.get_elevation(f64::NAN, t.base_lon()), ELV_MISSING);
    assert_eq!(t.get_elevation(t.base_lat(), f64::NAN), ELV_MISSING);
}
