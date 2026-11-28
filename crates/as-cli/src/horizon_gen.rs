//! `airscout-cli horizon-gen`: port of the C# HorizonGenerator (`HorizonGenerator/MainDlg.cs:36-100`).
//! It writes one CSV of radio horizons per station of `stations.db3`, for the 12 real bands.
//!
//! Each band evaluates the terrain of the full elevation path for its bearing. The 12 columns
//! match the header (the C# loop also runs `BNONE` and `BALL`, `:64`), the header typo
//! `eps[144M[deg]` (`:80`) is fixed, and each station gets its own file instead of one
//! `Horizon.csv` (`:78`). It reads only local data: nothing is downloaded.

use crate::{bbox, parse_model, Res};
use as_elevation::{check_area, DbStatus, ElevationError, ElevationModel, ElevationStore};
use as_geo::band::Band;
use as_geo::latlon::{self, GPoint, EARTH_RADIUS_KM};
use as_propagation::epsilon_max_from_path;
use as_settings::default_band_table;
use as_stations::{Location, StationDb};
use as_wintest::dotnet::fixed;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Number of bands, and so of value columns.
const BANDS: usize = 12;

#[derive(clap::Args)]
pub struct HorizonGenArgs {
    /// Data directory: the elevation store and `stations.db3`.
    #[arg(long)]
    pub data: PathBuf,
    /// Elevation model: globe, srtm3, srtm1, aster3, aster1.
    #[arg(long, default_value = "srtm1")]
    pub model: String,
    /// Antenna height above ground [m].
    #[arg(long, default_value_t = 40.0)]
    pub height: f64,
    /// Horizon radius [km].
    #[arg(long, default_value_t = 500.0)]
    pub dist: f64,
    /// Effective earth radius factor.
    #[arg(long, default_value_t = 1.33)]
    pub k: f64,
    /// Highest obstacle [m] for the path cut; 0 = no cut.
    #[arg(long, default_value_t = 4797.0)]
    pub max_elv: f64,
    /// Use the C# F1 clearances (0.2 up to 144 MHz, 0.4 up to 432 MHz, else 0.6) instead of
    /// the default band table.
    #[arg(long)]
    pub legacy_f1: bool,
    /// `all` (any case) or one call.
    #[arg(long, default_value = "all")]
    pub stations: String,
    /// Output directory (created if needed).
    #[arg(long, default_value = ".")]
    pub out: PathBuf,
}

/// CSV header: the C# text (`MainDlg.cs:80`) with `eps[144M[deg]` corrected to `eps_144M[deg]`.
pub fn csv_header() -> String {
    let mut s = String::from("bearing[deg]");
    for b in Band::all_except_none_and_all() {
        s.push_str(&format!(";eps_{}[deg]", b.name()));
    }
    s
}

/// The C# F1 clearance thresholds (`MainDlg.cs:66-70`).
pub fn legacy_f1(b: Band) -> f64 {
    if b.to_mhz() <= 144.0 {
        0.2
    } else if b.to_mhz() <= 432.0 {
        0.4
    } else {
        0.6
    }
}

/// F1 clearance per band, in band order: the C# thresholds or the default band table.
pub fn f1_table(legacy: bool) -> [f64; BANDS] {
    let table = default_band_table();
    let mut f1 = [0.0; BANDS];
    for (x, b) in f1.iter_mut().zip(Band::all_except_none_and_all()) {
        *x = if legacy {
            legacy_f1(*b)
        } else {
            table[b].f1_clearance
        };
    }
    f1
}

/// 360 rows of horizon angles in degrees, one value per band, and whether every elevation path
/// was valid. One path per bearing is shared by all bands, as in C# (`MainDlg.cs:61-73`).
#[allow(clippy::too_many_arguments)]
pub fn horizon_rows(
    store: &ElevationStore,
    model: ElevationModel,
    lat: f64,
    lon: f64,
    h1: f64,
    dist_km: f64,
    max_elv: f64,
    re: f64,
    f1: &[f64; BANDS],
) -> Result<(Vec<[f64; BANDS]>, bool), ElevationError> {
    let step = model.default_step_width_m();
    let mut rows = Vec::with_capacity(360);
    let mut valid = true;
    for j in 0..360 {
        let ep = store
            .create_path_from_bearing(model, lat, lon, f64::from(j), dist_km, step, false, None)?
            .ok_or(ElevationError::Cancelled)?;
        valid &= ep.valid;
        let mut row = [0.0; BANDS];
        for (i, b) in Band::all_except_none_and_all().iter().enumerate() {
            let mut elv = ep.path.clone();
            let hp =
                epsilon_max_from_path(h1, &mut elv, dist_km, step, b.to_ghz(), f1[i], max_elv, re);
            // C# order of operations (`MainDlg.cs:86`)
            row[i] = hp.eps_min / std::f64::consts::PI * 180.0;
        }
        rows.push(row);
    }
    Ok((rows, valid))
}

/// The CSV text: header, then `bearing;v1;…;v12` with 8 decimals (invariant culture, `F8`).
/// Every line ends with `\n`.
pub fn csv_text(rows: &[[f64; BANDS]]) -> String {
    let mut s = csv_header();
    s.push('\n');
    for (j, row) in rows.iter().enumerate() {
        let vals: Vec<String> = row.iter().map(|v| fixed(*v, 8)).collect();
        s.push_str(&format!("{j};{}\n", vals.join(";")));
    }
    s
}

/// `{call}_{loc}.csv`, every character outside `A-Z`, `a-z`, `0-9` and `-` replaced by `_`.
pub fn file_name(call: &str, loc: &str) -> String {
    let safe: String = format!("{call}_{loc}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe}.csv")
}

/// `name`, or `stem-2.csv`, `stem-3.csv`, … if the name is taken.
/// `taken` holds lower-case names: APFS and NTFS compare file names without case.
fn unique_name(name: String, taken: &HashSet<String>) -> String {
    if !taken.contains(&name.to_lowercase()) {
        return name;
    }
    let stem = name.strip_suffix(".csv").unwrap_or(&name);
    (2..)
        .map(|n| format!("{stem}-{n}.csv"))
        .find(|n| !taken.contains(&n.to_lowercase()))
        .unwrap_or(name)
}

/// Check the station's horizon area and update the store's coverage without downloading.
fn check_station_area(
    store: &ElevationStore,
    model: ElevationModel,
    p: GPoint,
    dist_km: f64,
    data: &Path,
) -> Res<()> {
    let ring = (0..360).map(|az| latlon::destination_point(p.lat, p.lon, f64::from(az), dist_km));
    let rect = bbox(ring.chain([p]))?;
    // the same name as `crate::sea_file`
    let sea = data.join(format!(
        "{}-sea-squares.txt",
        model.dir_name().to_lowercase()
    ));
    let status = check_area(store, model, rect, &sea, None, &mut |m| eprintln!("{m}"))?;
    store.set_bounds(rect);
    if status.complete {
        store.set_status_bit(model, DbStatus::COMPLETE);
    } else {
        store.reset_status_bit(model, DbStatus::COMPLETE);
    }
    Ok(())
}

/// Horizon CSV text of one station, and whether every path was valid.
fn station_csv(
    store: &ElevationStore,
    model: ElevationModel,
    l: &Location,
    a: &HorizonGenArgs,
    f1: &[f64; BANDS],
) -> Res<(String, bool)> {
    let p = GPoint::new(l.lat, l.lon);
    check_station_area(store, model, p, a.dist, &a.data)?;
    // the C# indexer, `setinvalidtozero = true` (`ElevationDatabase.cs:738-744`); C# keeps h1
    // in a `short` (`MainDlg.cs:49`), the port keeps a fractional `--height`
    let h1 = f64::from(store.elevation(model, l.lat, l.lon, true)?) + a.height;
    let re = EARTH_RADIUS_KM * a.k;
    let (rows, valid) = horizon_rows(store, model, l.lat, l.lon, h1, a.dist, a.max_elv, re, f1)?;
    Ok((csv_text(&rows), valid))
}

fn check_args(a: &HorizonGenArgs) -> Res<()> {
    if !(a.dist.is_finite() && a.dist > 0.0) {
        return Err(format!("--dist must be above 0 km: {}", a.dist).into());
    }
    if !(a.k.is_finite() && a.k > 0.0) {
        return Err(format!("--k must be above 0: {}", a.k).into());
    }
    if !a.height.is_finite() || !a.max_elv.is_finite() {
        return Err("--height and --max-elv must be finite numbers".into());
    }
    Ok(())
}

pub fn run(a: &HorizonGenArgs) -> Res<Value> {
    check_args(a)?;
    let model = parse_model(&a.model)?;
    let db_path = a.data.join("stations.db3");
    // checked first: opening would create an empty database
    if !db_path.is_file() {
        return Err(format!("no station database at {}", db_path.display()).into());
    }
    // `LocationGetAll()`, ordered by call (`StationDatabase.cs:633-643`)
    let mut list = StationDb::open(&db_path)?.location_get_all(None)?;
    let filter = a.stations.trim();
    if !filter.eq_ignore_ascii_case("all") {
        // calls are stored trimmed and upper-cased
        let call = filter.to_uppercase();
        list.retain(|l| l.call == call);
    }
    if list.is_empty() {
        return Err(format!("no station matches {} in {}", a.stations, db_path.display()).into());
    }
    let store = ElevationStore::open(&a.data)?;
    let f1 = f1_table(a.legacy_f1);
    std::fs::create_dir_all(&a.out)?;
    let (mut files, mut incomplete, mut taken) = (Vec::new(), Vec::new(), HashSet::new());
    let mut failed = 0;
    for l in &list {
        let r = station_csv(&store, model, l, a, &f1).and_then(|(text, valid)| {
            let name = unique_name(file_name(&l.call, &l.loc), &taken);
            let path = a.out.join(&name);
            std::fs::write(&path, text)?;
            eprintln!("horizon {} {}: {}", l.call, l.loc, path.display());
            Ok((name, valid))
        });
        match r {
            Ok((name, valid)) => {
                taken.insert(name.to_lowercase());
                if !valid {
                    incomplete.push(name.clone());
                }
                files.push(name);
            }
            Err(e) => {
                eprintln!("{} {}: {e}", l.call, l.loc);
                failed += 1;
            }
        }
    }
    if failed > 0 {
        return Err(format!("{failed} of {} stations failed", list.len()).into());
    }
    Ok(json!({
        "model": model.dir_name(),
        "stations": list.len(),
        "files": files,
        "incomplete": incomplete,
    }))
}

#[cfg(test)]
mod tests {
    use as_elevation::get_locs_from_rect;
    use as_geo::latlon::GRect;

    use super::*;
    use as_elevation::ElevationTile;
    use as_geo::propagation::epsilon_from_heights;

    #[test]
    fn finding_282_each_band_uses_the_full_terrain_profile() {
        let dir = tempfile::tempdir().unwrap();
        let store = ElevationStore::open(dir.path()).unwrap();
        let model = ElevationModel::Globe;
        let mut terrain = vec![0; 50];
        terrain[20..25].fill(100);
        let tile = ElevationTile::new(
            "JO51AA",
            0,
            51.01,
            10.005,
            100,
            51.01,
            10.032,
            5,
            10,
            terrain,
            chrono::DateTime::UNIX_EPOCH,
        )
        .unwrap();
        store.insert_or_update_if_newer(model, &tile).unwrap();
        let (lat, lon, h1, dist, re) = (51.01, 10.005, 10.0, 3.0, 8473.43);
        let mut clearance = [0.0; BANDS];
        clearance[0] = 10.0;
        let (rows, _) =
            horizon_rows(&store, model, lat, lon, h1, dist, 100.0, re, &clearance).unwrap();
        let step = model.default_step_width_m() / 1000.0;
        let ep = store
            .create_path_from_bearing(model, lat, lon, 90.0, dist, step * 1000.0, false, None)
            .unwrap()
            .unwrap();
        assert!(ep.path.contains(&100));
        for (i, &band) in Band::all_except_none_and_all().iter().enumerate() {
            let best = ep
                .path
                .iter()
                .enumerate()
                .skip(1)
                .map(|(j, &elv)| {
                    let d = j as f64 * step;
                    let f1 = ((0.3 / band.to_ghz()) * 1000.0 * d * (1.0 - d / dist)).sqrt();
                    epsilon_from_heights(h1, d, f64::from(elv) + clearance[i] * f1, re)
                })
                .fold(f64::MIN, f64::max)
                .to_degrees();
            assert!(
                (rows[90][i] - best).abs() < 1e-9,
                "band {band:?}: {} vs {best}",
                rows[90][i]
            );
        }
    }

    #[test]
    fn area_across_both_boundaries_gives_four_squares() {
        let r = GRect::new(50.6, 11.6, 51.4, 12.4);
        assert_eq!(
            get_locs_from_rect(r, 2).unwrap(),
            ["JO50", "JO60", "JO51", "JO61"]
        );
    }

    #[test]
    fn area_inside_one_square_gives_one() {
        assert_eq!(
            get_locs_from_rect(GRect::new(50.2, 10.2, 50.8, 11.8), 2).unwrap(),
            ["JO50"]
        );
    }
}
