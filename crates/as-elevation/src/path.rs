//! Port of `ElevationPath.cs` and the path functions of `ElevationDatabase.cs`
//! (`ElevationPathCreateFromLatLon`, `MovingAverage`, find/insert).

use crate::{ElevationError, ElevationModel, ElevationStore, ELV_MISSING, TILE_MISSING};
use as_geo::latlon::{self, GRect};
use as_geo::maidenhead::check_lat_lon;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use std::sync::atomic::{AtomicBool, Ordering};

/// Elevation profile between two points, sampled every `step_width_m` metres.
#[derive(Clone, Debug, PartialEq)]
pub struct ElevationPath {
    pub lat1: f64,
    pub lon1: f64,
    pub lat2: f64,
    pub lon2: f64,
    pub step_width_m: f64,
    pub count: usize,
    pub path: Vec<i16>,
    pub valid: bool,
    pub last_updated: DateTime<Utc>,
}

/// What a path calculation may assume about the stored data: the original's `COMPLETE` status
/// bit and database bounds (ElevationDatabase.cs:1407, 1481-1498), given by the caller instead of
/// read from the store's shared state. A missing tile on a path inside `bounds` of complete data
/// counts as sea; any other missing tile, or incomplete data, makes the path invalid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coverage {
    pub complete: bool,
    pub bounds: GRect,
}

/// Upper limit of samples per path (20 000 km at 1 m), guards against absurd step widths.
const MAX_PATH_POINTS: f64 = 2e7;

impl ElevationPath {
    /// New path with every value set to [`ELV_MISSING`]; `count = trunc(distance*1000/step)+1`.
    pub fn new(lat1: f64, lon1: f64, lat2: f64, lon2: f64, step_width_m: f64) -> Self {
        let distance = latlon::distance(lat1, lon1, lat2, lon2);
        let count = (distance * 1000.0 / step_width_m) as i64 + 1;
        let count = count.max(0) as usize;
        Self {
            lat1,
            lon1,
            lat2,
            lon2,
            step_width_m,
            count,
            path: vec![ELV_MISSING; count],
            valid: true,
            last_updated: Utc::now(),
        }
    }

    /// Great-circle distance [km].
    pub fn distance(&self) -> f64 {
        latlon::distance(self.lat1, self.lon1, self.lat2, self.lon2)
    }
    /// Bearing from point 1 to point 2 [deg].
    pub fn bearing12(&self) -> f64 {
        latlon::bearing(self.lat1, self.lon1, self.lat2, self.lon2)
    }
    /// Bearing from point 2 to point 1 [deg].
    pub fn bearing21(&self) -> f64 {
        latlon::bearing(self.lat2, self.lon2, self.lat1, self.lon1)
    }

    /// `GetInfoPoints`: one value per km for diagrams.
    pub fn info_points(&self) -> Result<Vec<i16>, ElevationError> {
        if self.step_width_m <= 0.0 {
            return Err(ElevationError::InvalidArgument(
                "step width must be > 0".into(),
            ));
        }
        if self.step_width_m > 1000.0 {
            return Err(ElevationError::InvalidArgument(format!(
                "path step width must be <= 1 km: {}",
                self.step_width_m
            )));
        }
        let distance = self.distance();
        let n = distance as usize + 1;
        (0..n)
            .map(|i| {
                let idx = (i as f64 * self.count as f64 / distance) as usize;
                self.path.get(idx).copied().ok_or_else(|| {
                    ElevationError::InvalidArgument(format!("info point index {idx} out of range"))
                })
            })
            .collect()
    }

    fn to_blob(&self) -> Vec<u8> {
        self.path.iter().flat_map(|v| v.to_le_bytes()).collect()
    }
}

/// `MovingAverage`: simple moving average, result is `periods - 1` shorter than `values`.
/// The C# `(short)` cast truncates toward zero, like Rust `as`.
pub(crate) fn moving_average(values: &[i16], periods: usize) -> Option<Vec<i16>> {
    if periods == 0 || values.len() < periods {
        return None;
    }
    let mut averages = vec![0i16; values.len() - periods + 1];
    let mut sum = 0.0f64;
    for i in 0..values.len() {
        if i < periods {
            sum += values[i] as f64;
            averages[0] = if i == periods - 1 {
                (sum / periods as f64) as i16
            } else {
                0
            };
        } else {
            sum = sum - values[i - periods] as f64 + values[i] as f64;
            averages[i - periods + 1] = (sum / periods as f64) as i16;
        }
    }
    Some(averages)
}

const PATH_ALGORITHM_VERSION: i64 = 3;

pub(crate) const LEGACY_PATH_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS ElevationPath (
    Lat1 REAL NOT NULL, Lon1 REAL NOT NULL, Lat2 REAL NOT NULL, Lon2 REAL NOT NULL,
    StepWidth REAL NOT NULL, Count INTEGER NOT NULL, Path BLOB NOT NULL, LastUpdated INTEGER NOT NULL,
    PRIMARY KEY (Lat1, Lon1, Lat2, Lon2, StepWidth))";

pub(crate) const PATH_CACHE_MIGRATION: &str = "DROP TABLE ElevationPath;
    CREATE TABLE ElevationPath (
        Lat1 REAL NOT NULL, Lon1 REAL NOT NULL, Lat2 REAL NOT NULL, Lon2 REAL NOT NULL,
        StepWidth REAL NOT NULL, Count INTEGER NOT NULL, Path BLOB NOT NULL, LastUpdated INTEGER NOT NULL,
        AlgorithmVersion INTEGER NOT NULL,
        PRIMARY KEY (Lat1, Lon1, Lat2, Lon2, StepWidth))";

fn cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|c| c.load(Ordering::Relaxed))
}

impl ElevationStore {
    /// `ElevationPathCreateFromLatLon`. `Ok(None)` if cancelled. Stores the path if valid and `save`.
    /// Validity follows the store's `COMPLETE` bit and bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn create_path(
        &self,
        model: ElevationModel,
        lat1: f64,
        lon1: f64,
        lat2: f64,
        lon2: f64,
        step_width_m: f64,
        save: bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<Option<ElevationPath>, ElevationError> {
        self.create_path_in(
            model,
            lat1,
            lon1,
            lat2,
            lon2,
            step_width_m,
            save,
            cancel,
            None,
        )
    }

    /// [`create_path`](Self::create_path) with the validity decided by `coverage` instead of the
    /// store's shared status (`None`: the store's), for callers that checked the data themselves.
    #[allow(clippy::too_many_arguments)]
    pub fn create_path_in(
        &self,
        model: ElevationModel,
        lat1: f64,
        lon1: f64,
        lat2: f64,
        lon2: f64,
        step_width_m: f64,
        save: bool,
        cancel: Option<&AtomicBool>,
        coverage: Option<Coverage>,
    ) -> Result<Option<ElevationPath>, ElevationError> {
        let current = || {
            coverage.unwrap_or_else(|| Coverage {
                complete: self.is_complete(model),
                bounds: self.bounds(),
            })
        };
        for (lat, lon) in [(lat1, lon1), (lat2, lon2)] {
            if !check_lat_lon(lat, lon) {
                return Err(ElevationError::InvalidCoordinate { lat, lon });
            }
        }
        // C# throws (overflow / out of memory) on such inputs; reject them before allocating.
        let points = latlon::distance(lat1, lon1, lat2, lon2) * 1000.0 / step_width_m;
        if !step_width_m.is_finite()
            || step_width_m <= 0.0
            || points.is_nan()
            || points > MAX_PATH_POINTS
        {
            return Err(ElevationError::InvalidArgument(format!(
                "step width {step_width_m} m is not usable for this path"
            )));
        }
        let mut ep = ElevationPath::new(lat1, lon1, lat2, lon2, step_width_m);
        let mut tile_missing = false;
        let step = step_width_m / 1000.0;
        let mut complete = current().complete;
        let avperiod = model.average_period();
        let bearing = ep.bearing12();
        let sample = |d: f64, tile_missing: &mut bool| -> Result<i16, ElevationError> {
            let gp = latlon::destination_point(lat1, lon1, bearing, d);
            let e = self.elevation(model, gp.lat, gp.lon, false)?;
            if e > TILE_MISSING {
                Ok(e)
            } else {
                if e == TILE_MISSING {
                    *tile_missing = true;
                }
                Ok(0)
            }
        };
        if avperiod == 0 {
            let mut d = 0.0;
            for i in 0..ep.count {
                ep.path[i] = sample(d, &mut tile_missing)?;
                d += step;
                if cancelled(cancel) {
                    return Ok(None);
                }
            }
        } else {
            // At the halved ASTER default, interleave two copies of the
            // original moving average, centred half a step apart. The even
            // lane samples the old positions in the same order and with the
            // same arithmetic, so every old filtered height is preserved.
            let stride = if step_width_m == model.default_step_width_m() {
                2
            } else {
                1
            };
            let filter_step = step * stride as f64;
            for phase in 0..stride.min(ep.count) {
                let output = ep.path[phase..].iter_mut().step_by(stride);
                let mut raw = vec![0i16; output.len() + avperiod - 1];
                let mut d = -((avperiod / 2) as f64) * filter_step + phase as f64 * step;
                for v in &mut raw {
                    *v = sample(d, &mut tile_missing)?;
                    d += filter_step;
                    if cancelled(cancel) {
                        return Ok(None);
                    }
                }
                for (v, average) in output.zip(moving_average(&raw, avperiod).unwrap_or_default()) {
                    *v = average;
                }
            }
        }
        if complete {
            complete = current().complete;
        }
        ep.valid = complete;
        if tile_missing && complete {
            let b = current().bounds;
            if !b.contains(lat1, lon1) || !b.contains(lat2, lon2) {
                ep.valid = false;
            }
        }
        if ep.valid && save {
            self.insert_path_if_newer(model, &ep)?;
        }
        Ok(Some(ep))
    }

    /// `ElevationPathCreateFromBearing`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_path_from_bearing(
        &self,
        model: ElevationModel,
        lat: f64,
        lon: f64,
        bearing: f64,
        distance_km: f64,
        step_width_m: f64,
        save: bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<Option<ElevationPath>, ElevationError> {
        let gp = latlon::destination_point(lat, lon, bearing, distance_km);
        self.create_path(model, lat, lon, gp.lat, gp.lon, step_width_m, save, cancel)
    }

    /// `ElevationPathFindOrCreateFromLatLon`.
    #[allow(clippy::too_many_arguments)]
    pub fn find_or_create_path(
        &self,
        model: ElevationModel,
        lat1: f64,
        lon1: f64,
        lat2: f64,
        lon2: f64,
        step_width_m: f64,
        save: bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<Option<ElevationPath>, ElevationError> {
        if let Some(p) = self.find_path(model, lat1, lon1, lat2, lon2, step_width_m)? {
            return Ok(Some(p));
        }
        self.create_path(model, lat1, lon1, lat2, lon2, step_width_m, save, cancel)
    }

    /// `ElevationPathFindOrCreateFromBearing`.
    #[allow(clippy::too_many_arguments)]
    pub fn find_or_create_path_from_bearing(
        &self,
        model: ElevationModel,
        lat: f64,
        lon: f64,
        bearing: f64,
        distance_km: f64,
        step_width_m: f64,
        save: bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<Option<ElevationPath>, ElevationError> {
        let gp = latlon::destination_point(lat, lon, bearing, distance_km);
        self.find_or_create_path(model, lat, lon, gp.lat, gp.lon, step_width_m, save, cancel)
    }

    /// `ElevationPathFind` (exact key match). Found paths are valid, like the C# default.
    pub fn find_path(
        &self,
        model: ElevationModel,
        lat1: f64,
        lon1: f64,
        lat2: f64,
        lon2: f64,
        step_width_m: f64,
    ) -> Result<Option<ElevationPath>, ElevationError> {
        self.with_conn(model, |conn| {
            let row = conn
                .query_row(
                    "SELECT Count, Path, LastUpdated FROM ElevationPath
                     WHERE Lat1 = ?1 AND Lon1 = ?2 AND Lat2 = ?3 AND Lon2 = ?4 AND StepWidth = ?5
                       AND AlgorithmVersion = ?6",
                    params![lat1, lon1, lat2, lon2, step_width_m, PATH_ALGORITHM_VERSION],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, Vec<u8>>(1)?,
                            r.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()?;
            let Some((count, blob, lu)) = row else {
                return Ok(None);
            };
            if blob.len() % 2 != 0 {
                return Err(ElevationError::Corrupt(
                    "elevation path blob has odd length".into(),
                ));
            }
            let path: Vec<i16> = blob
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b))
                .collect();
            Ok(Some(ElevationPath {
                lat1,
                lon1,
                lat2,
                lon2,
                step_width_m,
                count: count.max(0) as usize,
                path,
                valid: true,
                last_updated: DateTime::<Utc>::from_timestamp_nanos(lu),
            }))
        })
    }

    /// `ElevationPathDeleteAll` (`ElevationDatabase.cs:1320-1331`): returns the rows removed.
    pub fn elevation_path_delete_all(
        &self,
        model: ElevationModel,
    ) -> Result<usize, ElevationError> {
        self.with_conn(model, |conn| {
            Ok(conn.execute("DELETE FROM ElevationPath", [])?)
        })
    }

    fn insert_path_if_newer(
        &self,
        model: ElevationModel,
        ep: &ElevationPath,
    ) -> Result<(), ElevationError> {
        self.with_conn(model, |conn| {
            let lu = ep.last_updated.timestamp_nanos_opt().unwrap_or(i64::MAX);
            conn.execute(
                "INSERT INTO ElevationPath (Lat1, Lon1, Lat2, Lon2, StepWidth, Count, Path, LastUpdated, AlgorithmVersion)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (Lat1, Lon1, Lat2, Lon2, StepWidth) DO UPDATE SET
                   Count = excluded.Count, Path = excluded.Path, LastUpdated = excluded.LastUpdated,
                   AlgorithmVersion = excluded.AlgorithmVersion
                 WHERE excluded.AlgorithmVersion != ElevationPath.AlgorithmVersion
                    OR excluded.LastUpdated > ElevationPath.LastUpdated",
                params![ep.lat1, ep.lon1, ep.lat2, ep.lon2, ep.step_width_m, ep.count as i64,
                    ep.to_blob(), lu, PATH_ALGORITHM_VERSION],
            )?;
            Ok(())
        })
    }
}
