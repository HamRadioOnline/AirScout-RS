//! `Path_BestCaseElevation`: a station whose position is only as exact as its locator square
//! (the centre of the 6-character cell) is moved to the highest point of that square and takes
//! that elevation (`MapDlg.LocationFindOrCreate` / `LocationFindOrUpdateOrCreate`, MapDlg.cs:3551,
//! and the same block inline in `PathCalculator`). Nothing is stored: only the position used for
//! the path changes.
//!
//! The two callers differ in the original and so here:
//! - the UI path ([`ui_position`]) asks the enabled models in `GetElevation` order and takes the
//!   first one that has a tile with a real maximum;
//! - the calculator ([`calculator_shift`]) asks only its own model and shifts only when that
//!   model has a stored tile with a real maximum (a void tile keeps the station at its locator
//!   position, #94).

use as_elevation::{ElevationModel, ElevationStore, ElvMinMaxInfo, ELV_MISSING};
use as_geo::latlon::GPoint;
use as_geo::maidenhead;
use as_settings::{ElevationEnabled, Settings};

/// The models in the order of `GetElevation` / `GetMinMaxElevationLoc`: most detailed first.
pub(crate) const MODEL_ORDER: [ElevationModel; 5] = [
    ElevationModel::Aster1,
    ElevationModel::Aster3,
    ElevationModel::Srtm1,
    ElevationModel::Srtm3,
    ElevationModel::Globe,
];

/// The 6-character locator of `pos` if it is only as exact as that (`!IsPrecise(lat, lon, 3)`):
/// only then the best-case shift applies. `Err` where C# throws (latitude 90).
fn imprecise_loc(pos: GPoint) -> Result<Option<String>, String> {
    let precise = maidenhead::is_precise(pos.lat, pos.lon, 3)
        .map_err(|e| format!("position {}, {}: {e}", pos.lat, pos.lon))?;
    if precise {
        return Ok(None);
    }
    maidenhead::loc_from_lat_lon(pos.lat, pos.lon, false, 3, false)
        .map_err(|e| format!("position {}, {}: {e}", pos.lat, pos.lon))
}

/// Best-case position of the UI path (`GetMinMaxElevationLoc`): the first enabled model, most
/// detailed first, with a stored tile whose maximum is not `ELV_MISSING`. `pos` stays if there
/// is none, or if `pos` is precise. The original takes the locator from the stored location;
/// this is the 6-character locator of `pos`, which is the same for a location made from a
/// position.
pub(crate) fn ui_position(
    enabled: &ElevationEnabled,
    pos: GPoint,
    mut find: impl FnMut(ElevationModel, &str) -> Option<ElvMinMaxInfo>,
) -> Result<GPoint, String> {
    let Some(loc) = imprecise_loc(pos)? else {
        return Ok(pos);
    };
    let mut best: Option<ElvMinMaxInfo> = None;
    for m in MODEL_ORDER {
        if best.is_some_and(|b| b.max_elv != i32::from(ELV_MISSING)) {
            break;
        }
        if !enabled.is_enabled(m) {
            continue;
        }
        if let Some(info) = find(m, &loc) {
            best = Some(info);
        }
    }
    Ok(match best {
        Some(b) if b.max_elv != i32::from(ELV_MISSING) => GPoint::new(b.max_lat, b.max_lon),
        _ => pos,
    })
}

/// [`ui_position`] on the elevation store, or `pos` itself if the setting is off. A store
/// error counts as "tile not stored".
pub(crate) fn effective_position(
    store: &ElevationStore,
    s: &Settings,
    pos: GPoint,
) -> Result<GPoint, String> {
    if !s.path_best_case_elevation {
        return Ok(pos);
    }
    ui_position(&s.elevation_enabled, pos, |m, loc| {
        store.max_elv_loc(m, loc).ok().flatten()
    })
}

/// Best-case shift of the calculator (`PathCalculator.OnDoWork`): the tile info of `model` if
/// `pos` is imprecise and the tile is stored with a real maximum. A stored tile whose maximum is
/// the void flag (`ELV_MISSING`) does not shift the station (#94). The caller takes position and
/// elevation from it.
pub(crate) fn calculator_shift(
    model: ElevationModel,
    pos: GPoint,
    find: impl FnOnce(ElevationModel, &str) -> Option<ElvMinMaxInfo>,
) -> Result<Option<ElvMinMaxInfo>, String> {
    Ok(imprecise_loc(pos)?
        .and_then(|loc| find(model, &loc))
        .filter(|info| info.max_elv != i32::from(ELV_MISSING)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(max_elv: i32, max_lat: f64, max_lon: f64) -> ElvMinMaxInfo {
        ElvMinMaxInfo {
            min_elv: 0,
            min_lat: 0.0,
            min_lon: 0.0,
            max_elv,
            max_lat,
            max_lon,
        }
    }

    fn all() -> ElevationEnabled {
        ElevationEnabled {
            globe: true,
            srtm3: true,
            srtm1: true,
            aster3: true,
            aster1: true,
        }
    }

    /// Centre of JO50ab; an exact 8-character position is "precise".
    fn centre() -> GPoint {
        maidenhead::gpoint_from_loc("JO50AB", maidenhead::PositionInRectangle::MiddleMiddle)
            .unwrap()
    }

    #[test]
    fn imprecise_position_moves_to_the_maximum() {
        let mut asked = Vec::new();
        let p = ui_position(&all(), centre(), |m, loc| {
            asked.push((m, loc.to_string()));
            Some(info(300, 50.01, 10.02))
        })
        .unwrap();
        assert_eq!((p.lat, p.lon), (50.01, 10.02));
        // the first (most detailed) model answered
        assert_eq!(asked, [(ElevationModel::Aster1, "JO50AB".to_string())]);
    }

    #[test]
    fn precise_position_stays() {
        let p = GPoint::new(centre().lat + 0.001, centre().lon);
        let r = ui_position(&all(), p, |_, _| panic!("no lookup for a precise position")).unwrap();
        assert_eq!((r.lat, r.lon), (p.lat, p.lon));
    }

    #[test]
    fn walk_skips_disabled_and_missing_maximum() {
        let en = ElevationEnabled {
            globe: true,
            srtm3: true,
            srtm1: false,
            aster3: true,
            aster1: false,
        };
        let mut asked = Vec::new();
        let p = ui_position(&en, centre(), |m, _| {
            asked.push(m);
            Some(match m {
                // a stored tile without data: the walk goes on
                ElevationModel::Aster3 => info(i32::from(ELV_MISSING), 1.0, 1.0),
                _ => info(250, 50.5, 10.5),
            })
        })
        .unwrap();
        assert_eq!(asked, [ElevationModel::Aster3, ElevationModel::Srtm3]);
        assert_eq!((p.lat, p.lon), (50.5, 10.5));
    }

    #[test]
    fn no_tile_keeps_the_position() {
        let p = ui_position(&all(), centre(), |_, _| None).unwrap();
        assert_eq!((p.lat, p.lon), (centre().lat, centre().lon));
        let only_missing = ui_position(&all(), centre(), |_, _| {
            Some(info(i32::from(ELV_MISSING), 1.0, 1.0))
        })
        .unwrap();
        assert_eq!(
            (only_missing.lat, only_missing.lon),
            (centre().lat, centre().lon)
        );
    }

    #[test]
    fn calculator_skips_a_void_tile_but_shifts_on_a_real_maximum() {
        // A stored tile with only the void flag (`max_elv == ELV_MISSING`) must not shift the
        // station to its stored `(0, 0)` (#94).
        let r = calculator_shift(ElevationModel::Srtm3, centre(), |m, loc| {
            assert_eq!((m, loc), (ElevationModel::Srtm3, "JO50AB"));
            Some(info(i32::from(ELV_MISSING), 0.0, 0.0))
        })
        .unwrap();
        assert!(r.is_none());
        // a real maximum still shifts, and only the calculator's own model is asked
        let r = calculator_shift(ElevationModel::Srtm3, centre(), |m, loc| {
            assert_eq!((m, loc), (ElevationModel::Srtm3, "JO50AB"));
            Some(info(300, 50.01, 10.02))
        })
        .unwrap();
        assert_eq!(r.unwrap().max_lat, 50.01);
        // a precise position is never looked up
        let precise = GPoint::new(centre().lat + 0.001, centre().lon);
        let r =
            calculator_shift(ElevationModel::Srtm3, precise, |_, _| panic!("no lookup")).unwrap();
        assert!(r.is_none());
    }
}
