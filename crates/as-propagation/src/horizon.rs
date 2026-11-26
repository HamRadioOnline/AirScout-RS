//! Port of `PropagationHorizon.cs`, `HorizonPoint.cs` and `PropagationHorizonCreate` /
//! `PropagationHorizonFind` from `PropagationDatabase.cs`.

use crate::engine::{cancelled, PropagationError};
use crate::PropagationEngine;
use as_elevation::{ElevationModel, LocalObstruction};
use as_geo::propagation::{epsilon_from_heights, f1_radius};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use rusqlite::{params, OptionalExtension};
use std::sync::atomic::AtomicBool;

pub(crate) const HORIZON_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS PropagationHorizon (
    Lat REAL NOT NULL, Lon REAL NOT NULL, h REAL NOT NULL, Dist REAL NOT NULL,
    QRG REAL NOT NULL, Radius REAL NOT NULL, F1_Clearance REAL NOT NULL, StepWidth REAL NOT NULL,
    Horizon BLOB NOT NULL, LastUpdated INTEGER NOT NULL,
    PRIMARY KEY (Lat, Lon, h, Dist, QRG, Radius, F1_Clearance, StepWidth))";

/// Horizon of one azimuth: the obstacle with the highest elevation angle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizonPoint {
    /// Distance of the obstacle [km].
    pub dist: f64,
    /// Elevation angle [rad] of the obstacle (incl. Fresnel clearance).
    pub eps_min: f64,
    /// Ground elevation of the obstacle [m].
    pub elv: i16,
}

impl Default for HorizonPoint {
    /// Like the C# parameterless constructor.
    fn default() -> Self {
        Self {
            dist: f64::MIN,
            eps_min: f64::MIN,
            elv: i16::MIN,
        }
    }
}

/// Inputs of a horizon calculation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizonParams {
    pub lat: f64,
    pub lon: f64,
    /// Station height [m] above sea level (ground elevation + antenna height).
    pub h: f64,
    /// Horizon radius [km].
    pub dist_km: f64,
    pub qrg_ghz: f64,
    /// Equivalent earth radius [km].
    pub radius_km: f64,
    pub f1_clearance: f64,
    pub step_width_m: f64,
}

/// 360 horizon points, index = azimuth in degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct PropagationHorizon {
    pub params: HorizonParams,
    pub horizon: [HorizonPoint; 360],
    pub valid: bool,
    /// Always `false`: reproduces `logic-findings.md` #11 (the C# check loop never runs).
    pub local_obstructed: bool,
    pub local_obstruction: Option<LocalObstruction>,
    pub last_updated: DateTime<Utc>,
}

impl PropagationHorizon {
    fn new(
        params: HorizonParams,
        horizon: [HorizonPoint; 360],
        valid: bool,
        local_obstruction: Option<&LocalObstruction>,
    ) -> Self {
        Self {
            params,
            horizon,
            valid,
            // C#: `for (int i = 0; i > 360; i++)` -- the loop body never executes.
            local_obstructed: false,
            local_obstruction: local_obstruction.cloned(),
            last_updated: Utc::now(),
        }
    }
}

const POINT_BYTES: usize = 8 + 8 + 2;

fn horizon_to_blob(h: &[HorizonPoint; 360]) -> Vec<u8> {
    let mut b = Vec::with_capacity(360 * POINT_BYTES);
    for p in h {
        b.extend_from_slice(&p.dist.to_le_bytes());
        b.extend_from_slice(&p.eps_min.to_le_bytes());
        b.extend_from_slice(&p.elv.to_le_bytes());
    }
    b
}

fn horizon_from_blob(b: &[u8]) -> Option<[HorizonPoint; 360]> {
    if b.len() != 360 * POINT_BYTES {
        return None;
    }
    let mut h = [HorizonPoint::default(); 360];
    let (chunks, _) = b.as_chunks::<POINT_BYTES>();
    for (p, c) in h.iter_mut().zip(chunks) {
        p.dist = f64::from_le_bytes(c[0..8].try_into().ok()?);
        p.eps_min = f64::from_le_bytes(c[8..16].try_into().ok()?);
        p.elv = i16::from_le_bytes(c[16..18].try_into().ok()?);
    }
    Some(h)
}

/// `EpsilonMaxFromPath`: highest elevation angle on `elv` (ignoring the first 100 m). With
/// `max_elv > 0` the path is truncated where an object of `max_elv` could no longer clear it.
#[allow(clippy::too_many_arguments)]
pub fn epsilon_max_from_path(
    h1: f64,
    elv: &mut Vec<i16>,
    dist: f64,
    step_width_m: f64,
    freq: f64,
    f1_clearance: f64,
    max_elv: f64,
    re: f64,
) -> HorizonPoint {
    let mut hp = HorizonPoint::default();
    let step = step_width_m / 1000.0;
    let mut d = 0.0;
    for i in 0..elv.len() {
        let f1 = f1_radius(freq, d, dist - d);
        let h2 = elv[i] as f64 + f1 * f1_clearance;
        let eps = epsilon_from_heights(h1, d, h2, re);
        if eps > hp.eps_min && d > 0.1 {
            hp.eps_min = eps;
            hp.dist = d;
            hp.elv = elv[i];
        }
        if max_elv > 0.0 {
            let epsmax = epsilon_from_heights(h1, d, max_elv, re);
            if epsmax < hp.eps_min {
                elv.truncate(i);
                return hp;
            }
        }
        d += step;
    }
    hp
}

impl PropagationEngine {
    /// One azimuth of `PropagationHorizonCreate`. Reproduces `logic-findings.md` #3
    /// (F1 radius uses the full horizon distance as the second leg).
    fn horizon_azimuth(
        &self,
        model: ElevationModel,
        p: &HorizonParams,
        azimuth: usize,
        cancel: Option<&AtomicBool>,
    ) -> Result<Option<(HorizonPoint, bool)>, PropagationError> {
        let Some(ep) = self.elevation().find_or_create_path_from_bearing(
            model,
            p.lat,
            p.lon,
            azimuth as f64,
            p.dist_km,
            p.step_width_m,
            false,
            cancel,
        )?
        else {
            return Ok(None);
        };
        let nf = self.near_field_suppression_m() / 1000.0;
        let mut eps_min = f64::MIN;
        let mut eps_dist = 0.0;
        let mut eps_elv: i16 = 0;
        for i in 0..ep.count {
            let d = i as f64 * p.step_width_m / 1000.0;
            let elv = ep.path.get(i).copied().unwrap_or(0);
            let dd = if d > nf { d } else { nf };
            let f1c = f1_radius(p.qrg_ghz, dd, p.dist_km) * p.f1_clearance;
            let eps = epsilon_from_heights(p.h, dd, elv as f64 + f1c, p.radius_km);
            if eps > eps_min {
                eps_min = eps;
                eps_dist = d;
                eps_elv = elv;
            }
        }
        Ok(Some((
            HorizonPoint {
                dist: eps_dist,
                eps_min,
                elv: eps_elv,
            },
            ep.valid,
        )))
    }

    /// `PropagationHorizonCreate`. Azimuths run in parallel; `progress(azimuth, point)` is called
    /// once per azimuth (in completion order). `Ok(None)` if cancelled.
    pub fn create_horizon(
        &self,
        model: ElevationModel,
        p: &HorizonParams,
        local_obstruction: Option<&LocalObstruction>,
        save: bool,
        cancel: Option<&AtomicBool>,
        progress: &(dyn Fn(usize, &HorizonPoint) + Sync),
    ) -> Result<Option<PropagationHorizon>, PropagationError> {
        let results: Vec<Option<(HorizonPoint, bool)>> = (0..360usize)
            .into_par_iter()
            .map(|az| {
                if cancelled(cancel) {
                    return Ok(None);
                }
                let r = self.horizon_azimuth(model, p, az, cancel)?;
                if let Some((hp, _)) = &r {
                    progress(az, hp);
                }
                Ok(r)
            })
            .collect::<Result<_, PropagationError>>()?;
        if cancelled(cancel) {
            return Ok(None);
        }
        let mut horizon = [HorizonPoint::default(); 360];
        let mut valid = true;
        for (slot, r) in horizon.iter_mut().zip(results) {
            let Some((hp, v)) = r else {
                return Ok(None);
            };
            *slot = hp;
            valid &= v;
        }
        let hor = PropagationHorizon::new(*p, horizon, valid, local_obstruction);
        if hor.valid && save {
            self.insert_horizon_if_newer(model, &hor)?;
        }
        Ok(Some(hor))
    }

    /// `PropagationHorizonFindOrCreate`.
    pub fn find_or_create_horizon(
        &self,
        model: ElevationModel,
        p: &HorizonParams,
        local_obstruction: Option<&LocalObstruction>,
        save: bool,
        cancel: Option<&AtomicBool>,
        progress: &(dyn Fn(usize, &HorizonPoint) + Sync),
    ) -> Result<Option<PropagationHorizon>, PropagationError> {
        if let Some(h) = self.find_horizon(model, p, local_obstruction)? {
            return Ok(Some(h));
        }
        self.create_horizon(model, p, local_obstruction, save, cancel, progress)
    }

    /// `PropagationHorizonFind`; loaded horizons are valid.
    pub fn find_horizon(
        &self,
        model: ElevationModel,
        p: &HorizonParams,
        local_obstruction: Option<&LocalObstruction>,
    ) -> Result<Option<PropagationHorizon>, PropagationError> {
        let conn = self.conn(model)?;
        let row = conn
            .query_row(
                "SELECT Horizon, LastUpdated FROM PropagationHorizon
                 WHERE Lat = ?1 AND Lon = ?2 AND h = ?3 AND Dist = ?4 AND QRG = ?5 AND Radius = ?6
                   AND F1_Clearance = ?7 AND StepWidth = ?8",
                params![
                    p.lat,
                    p.lon,
                    p.h,
                    p.dist_km,
                    p.qrg_ghz,
                    p.radius_km,
                    p.f1_clearance,
                    p.step_width_m
                ],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((blob, lu)) = row else {
            return Ok(None);
        };
        // a corrupt blob is treated as a cache miss
        let Some(horizon) = horizon_from_blob(&blob) else {
            return Ok(None);
        };
        let mut h = PropagationHorizon::new(*p, horizon, true, local_obstruction);
        h.last_updated = DateTime::<Utc>::from_timestamp_nanos(lu);
        Ok(Some(h))
    }

    /// `PropagationHorizonInsertOrUpdateIfNewer`. Does not reproduce `logic-findings.md` #6.
    fn insert_horizon_if_newer(
        &self,
        model: ElevationModel,
        h: &PropagationHorizon,
    ) -> Result<(), PropagationError> {
        let p = &h.params;
        let lu = h.last_updated.timestamp_nanos_opt().unwrap_or(i64::MAX);
        self.conn(model)?.execute(
            "INSERT INTO PropagationHorizon (Lat, Lon, h, Dist, QRG, Radius, F1_Clearance, StepWidth, Horizon, LastUpdated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (Lat, Lon, h, Dist, QRG, Radius, F1_Clearance, StepWidth) DO UPDATE SET
               Horizon = excluded.Horizon, LastUpdated = excluded.LastUpdated
             WHERE excluded.LastUpdated > PropagationHorizon.LastUpdated",
            params![p.lat, p.lon, p.h, p.dist_km, p.qrg_ghz, p.radius_km, p.f1_clearance, p.step_width_m, horizon_to_blob(&h.horizon), lu],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip() {
        let mut h = [HorizonPoint::default(); 360];
        h[17] = HorizonPoint {
            dist: 1.5,
            eps_min: -0.002,
            elv: 812,
        };
        assert_eq!(horizon_from_blob(&horizon_to_blob(&h)), Some(h));
        assert_eq!(horizon_from_blob(&[0u8; 10]), None);
    }
}
