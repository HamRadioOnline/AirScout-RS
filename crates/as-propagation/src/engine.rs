//! Port of `PropagationDatabase.cs`: calculation and SQLite cache of propagation paths.

use crate::{PathParams, PropagationPath};
use as_elevation::{Coverage, ElevationError, ElevationModel, ElevationStore};
use as_geo::propagation::{epsilon_from_heights, f1_radius};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// Errors of the propagation crate.
#[derive(thiserror::Error, Debug)]
pub enum PropagationError {
    #[error(transparent)]
    Elevation(#[from] ElevationError),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database schema version {actual} is newer than supported version {supported}")]
    SchemaAhead { actual: i64, supported: i64 },
}

impl From<as_db::DbError> for PropagationError {
    fn from(err: as_db::DbError) -> Self {
        match err {
            as_db::DbError::SchemaAhead { actual, supported } => {
                Self::SchemaAhead { actual, supported }
            }
            as_db::DbError::Sqlite(err) => Self::Db(err),
        }
    }
}

const PATH_ALGORITHM_VERSION: i64 = 3;

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS PropagationPath (
    Lat1 REAL NOT NULL, Lon1 REAL NOT NULL, h1 REAL NOT NULL,
    Lat2 REAL NOT NULL, Lon2 REAL NOT NULL, h2 REAL NOT NULL,
    QRG REAL NOT NULL, Radius REAL NOT NULL, F1_Clearance REAL NOT NULL, StepWidth REAL NOT NULL,
    Eps1_Min REAL, Eps2_Min REAL, LastUpdated INTEGER NOT NULL,
    PRIMARY KEY (Lat1, Lon1, h1, Lat2, Lon2, h2, QRG, Radius, F1_Clearance, StepWidth))",
    crate::horizon::LEGACY_HORIZON_SCHEMA,
    // Stage 6: new Fresnel geometry and an expanded primary key (#354).
    "DROP TABLE PropagationPath;
     CREATE TABLE PropagationPath (
        Lat1 REAL NOT NULL, Lon1 REAL NOT NULL, h1 REAL NOT NULL,
        Lat2 REAL NOT NULL, Lon2 REAL NOT NULL, h2 REAL NOT NULL,
        QRG REAL NOT NULL, Radius REAL NOT NULL, F1_Clearance REAL NOT NULL, StepWidth REAL NOT NULL,
        NearField REAL NOT NULL, Eps1_Min REAL, Eps2_Min REAL, LastUpdated INTEGER NOT NULL,
        AlgorithmVersion INTEGER NOT NULL,
        PRIMARY KEY (Lat1, Lon1, h1, Lat2, Lon2, h2, QRG, Radius, F1_Clearance, StepWidth, NearField))",
    crate::horizon::HORIZON_CACHE_MIGRATION,
];

/// Propagation calculator with per-model SQLite caches (`propagation_<model>.db3`).
pub struct PropagationEngine {
    pub(crate) elevation: Arc<ElevationStore>,
    pub(crate) conns: HashMap<ElevationModel, Mutex<Connection>>,
    near_field_bits: AtomicU64,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|c| c.load(Ordering::Relaxed))
}

impl PropagationEngine {
    /// Open the caches in `dir`. Near-field suppression defaults to 0 like the C# property.
    pub fn open(elevation: Arc<ElevationStore>, dir: &Path) -> Result<Self, PropagationError> {
        let mut conns = HashMap::new();
        for model in ElevationModel::ALL {
            let conn = as_db::open_versioned(
                &dir.join(format!("propagation_{}", model.db_file_name())),
                SCHEMA,
            )?;
            conns.insert(model, Mutex::new(conn));
        }
        Ok(Self {
            elevation,
            conns,
            near_field_bits: AtomicU64::new(0f64.to_bits()),
        })
    }

    /// Run `f` with the connection of the propagation database of `model`.
    pub fn with_conn<R>(
        &self,
        model: ElevationModel,
        f: impl FnOnce(&Connection) -> R,
    ) -> Result<R, PropagationError> {
        let conn = self.conn(model)?;
        Ok(f(&conn))
    }

    /// `PropagationPathDeleteAll` (`PropagationDatabase.cs:470-481`): returns the rows removed.
    pub fn path_delete_all(&self, model: ElevationModel) -> Result<usize, PropagationError> {
        let conn = self.conn(model)?;
        Ok(conn.execute("DELETE FROM PropagationPath", [])?)
    }

    /// The elevation store used by this engine.
    pub fn elevation(&self) -> &Arc<ElevationStore> {
        &self.elevation
    }

    /// Near-field suppression [m]: obstacles closer than this are evaluated at this distance.
    pub fn near_field_suppression_m(&self) -> f64 {
        f64::from_bits(self.near_field_bits.load(Ordering::Relaxed))
    }

    /// Set the near-field suppression [m] (`Path_NearFieldSuppression`, default 100 in the app).
    pub fn set_near_field_suppression_m(&self, v: f64) {
        self.near_field_bits.store(v.to_bits(), Ordering::Relaxed);
    }

    pub(crate) fn conn(
        &self,
        model: ElevationModel,
    ) -> Result<MutexGuard<'_, Connection>, PropagationError> {
        self.conns.get(&model).map(lock).ok_or_else(|| {
            ElevationError::InvalidArgument(format!("no database for {model:?}")).into()
        })
    }

    /// `PropagationPathCreateFromLatLon`. `Ok(None)` if cancelled.
    pub fn create_path(
        &self,
        model: ElevationModel,
        p: &PathParams,
        save: bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<Option<PropagationPath>, PropagationError> {
        self.create_path_in(model, p, save, cancel, None)
    }

    /// [`create_path`](Self::create_path) with the validity of a newly computed elevation path
    /// decided by `coverage` (`None`: the elevation store's status), see
    /// [`ElevationStore::create_path_in`].
    pub fn create_path_in(
        &self,
        model: ElevationModel,
        p: &PathParams,
        save: bool,
        cancel: Option<&AtomicBool>,
        coverage: Option<Coverage>,
    ) -> Result<Option<PropagationPath>, PropagationError> {
        let near_field_m = self.near_field_suppression_m();
        self.compute_path(model, p, save, cancel, coverage, near_field_m)
    }

    fn compute_path(
        &self,
        model: ElevationModel,
        p: &PathParams,
        save: bool,
        cancel: Option<&AtomicBool>,
        coverage: Option<Coverage>,
        near_field_m: f64,
    ) -> Result<Option<PropagationPath>, PropagationError> {
        let found =
            self.elevation
                .find_path(model, p.lat1, p.lon1, p.lat2, p.lon2, p.step_width_m)?;
        let ep = match found {
            Some(ep) => ep,
            None => match self.elevation.create_path_in(
                model,
                p.lat1,
                p.lon1,
                p.lat2,
                p.lon2,
                p.step_width_m,
                save,
                cancel,
                coverage,
            )? {
                Some(ep) => ep,
                None => return Ok(None),
            },
        };
        let mut eps1_min = f64::MIN;
        let mut eps2_min = f64::MIN;
        let nf = near_field_m / 1000.0;
        let distance = ep.distance();
        for i in 0..ep.count {
            let dist1 = i as f64 * p.step_width_m / 1000.0;
            let dist2 = (distance - dist1).max(0.0);
            let f1c1 = f1_radius(p.qrg_ghz, dist1, dist2) * p.f1_clearance;
            let f1c2 = f1_radius(p.qrg_ghz, dist2, dist1) * p.f1_clearance;
            let elv = ep.path.get(i).copied().unwrap_or(0) as f64;
            let eps1 = epsilon_from_heights(
                p.h1,
                if dist1 > nf { dist1 } else { nf },
                elv + f1c1,
                p.radius_km,
            );
            if eps1 > eps1_min {
                eps1_min = eps1;
            }
            let eps2 = epsilon_from_heights(
                p.h2,
                if dist2 > nf { dist2 } else { nf },
                elv + f1c2,
                p.radius_km,
            );
            if eps2 > eps2_min {
                eps2_min = eps2;
            }
            if cancelled(cancel) {
                return Ok(None);
            }
        }
        let pp = PropagationPath::new(*p, eps1_min, eps2_min, ep.valid);
        if pp.valid && save && !near_field_m.is_nan() {
            self.insert_path_if_newer(model, &pp, near_field_m)?;
        }
        Ok(Some(pp))
    }

    /// `PropagationPathFindOrCreateFromLatLon`.
    pub fn find_or_create_path(
        &self,
        model: ElevationModel,
        p: &PathParams,
        save: bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<Option<PropagationPath>, PropagationError> {
        let near_field_m = self.near_field_suppression_m();
        if let Some(pp) = self.find_path_with_near_field(model, p, near_field_m)? {
            return Ok(Some(pp));
        }
        self.compute_path(model, p, save, cancel, None, near_field_m)
    }

    /// `PropagationPathFind`; the local obstruction is applied on load (not part of the key).
    pub fn find_path(
        &self,
        model: ElevationModel,
        p: &PathParams,
    ) -> Result<Option<PropagationPath>, PropagationError> {
        self.find_path_with_near_field(model, p, self.near_field_suppression_m())
    }

    fn find_path_with_near_field(
        &self,
        model: ElevationModel,
        p: &PathParams,
        near_field_m: f64,
    ) -> Result<Option<PropagationPath>, PropagationError> {
        let conn = self.conn(model)?;
        let row = conn
            .query_row(
                "SELECT Eps1_Min, Eps2_Min, LastUpdated FROM PropagationPath
                 WHERE Lat1 = ?1 AND Lon1 = ?2 AND h1 = ?3 AND Lat2 = ?4 AND Lon2 = ?5 AND h2 = ?6
                   AND QRG = ?7 AND Radius = ?8 AND F1_Clearance = ?9 AND StepWidth = ?10
                   AND NearField = ?11 AND AlgorithmVersion = ?12",
                params![
                    p.lat1,
                    p.lon1,
                    p.h1,
                    p.lat2,
                    p.lon2,
                    p.h2,
                    p.qrg_ghz,
                    p.radius_km,
                    p.f1_clearance,
                    p.step_width_m,
                    near_field_m,
                    PATH_ALGORITHM_VERSION
                ],
                |r| {
                    Ok((
                        r.get::<_, f64>(0)?,
                        r.get::<_, f64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        Ok(row.map(|(e1, e2, lu)| {
            let mut pp = PropagationPath::new(*p, e1, e2, true);
            pp.last_updated = DateTime::<Utc>::from_timestamp_nanos(lu);
            pp
        }))
    }

    fn insert_path_if_newer(
        &self,
        model: ElevationModel,
        pp: &PropagationPath,
        near_field_m: f64,
    ) -> Result<(), PropagationError> {
        let p = &pp.params;
        let lu = pp.last_updated.timestamp_nanos_opt().unwrap_or(i64::MAX);
        self.conn(model)?.execute(
            "INSERT INTO PropagationPath
               (Lat1, Lon1, h1, Lat2, Lon2, h2, QRG, Radius, F1_Clearance, StepWidth,
                NearField, Eps1_Min, Eps2_Min, LastUpdated, AlgorithmVersion)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT (Lat1, Lon1, h1, Lat2, Lon2, h2, QRG, Radius, F1_Clearance, StepWidth, NearField)
             DO UPDATE SET Eps1_Min = excluded.Eps1_Min, Eps2_Min = excluded.Eps2_Min,
               LastUpdated = excluded.LastUpdated, AlgorithmVersion = excluded.AlgorithmVersion
             WHERE excluded.AlgorithmVersion != PropagationPath.AlgorithmVersion
                OR excluded.LastUpdated > PropagationPath.LastUpdated",
            params![p.lat1, p.lon1, p.h1, p.lat2, p.lon2, p.h2, p.qrg_ghz, p.radius_km,
                p.f1_clearance, p.step_width_m, near_field_m, pp.eps1_min, pp.eps2_min, lu,
                PATH_ALGORITHM_VERSION],
        )?;
        Ok(())
    }
}
