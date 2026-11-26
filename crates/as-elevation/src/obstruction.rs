//! Port of `LocalObstruction.cs`: per-degree obstruction (distance, height) around a station,
//! plus its per-model SQLite storage (`ElevationDatabase.LocalObstruction*`).

use crate::{ElevationError, ElevationModel, ElevationStore};
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

/// Table of the original (`LocalObstructionDesignator.TableName`, columns 1:1).
pub(crate) const OBSTRUCTION_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS LocalObstruction (
    Lat DOUBLE NOT NULL DEFAULT 0, Lon DOUBLE NOT NULL DEFAULT 0, Distance BLOB, Height BLOB,
    LastUpdated INT32, PRIMARY KEY(Lat, Lon))";

/// Local obstruction: for each integer bearing, distance [m] and height [m] of an obstacle.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalObstruction {
    pub lat: f64,
    pub lon: f64,
    pub distance: [f64; 360],
    pub height: [f64; 360],
}

impl LocalObstruction {
    /// Missing heights and distances default independently to zero.
    pub fn new(
        lat: f64,
        lon: f64,
        distance: Option<[f64; 360]>,
        height: Option<[f64; 360]>,
    ) -> Self {
        Self {
            lat,
            lon,
            distance: distance.unwrap_or([0.0; 360]),
            height: height.unwrap_or([0.0; 360]),
        }
    }

    /// `GetObstruction`: elevation angle [rad] of the obstacle at `bearing`, seen from `my_height`.
    /// Each sector is centred on its integer bearing. A missing distance has no obstruction.
    pub fn obstruction(&self, my_height: f64, bearing: f64) -> f64 {
        let i = bearing.rem_euclid(360.0).round() as usize % 360;
        if self.distance[i] <= 0.0 {
            return f64::MIN;
        }
        ((self.height[i] - my_height) / self.distance[i]).atan()
    }
}

fn to_blob(a: &[f64; 360]) -> Vec<u8> {
    a.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn from_blob(b: &[u8], what: &str) -> Result<[f64; 360], ElevationError> {
    if b.len() != 360 * 8 {
        return Err(ElevationError::Corrupt(format!(
            "local obstruction {what}: blob has {} bytes, expected {}",
            b.len(),
            360 * 8
        )));
    }
    let mut a = [0.0; 360];
    for (v, c) in a.iter_mut().zip(b.as_chunks::<8>().0) {
        *v = f64::from_le_bytes(*c);
    }
    Ok(a)
}

impl ElevationStore {
    /// `LocalObstructionFind(lat, lon, model)`.
    pub fn local_obstruction_find(
        &self,
        model: ElevationModel,
        lat: f64,
        lon: f64,
    ) -> Result<Option<LocalObstruction>, ElevationError> {
        self.with_conn(model, |conn| {
            let row = conn
                .query_row(
                    "SELECT Distance, Height FROM LocalObstruction WHERE Lat = ?1 AND Lon = ?2",
                    params![lat, lon],
                    |r| {
                        Ok((
                            r.get::<_, Option<Vec<u8>>>(0)?,
                            r.get::<_, Option<Vec<u8>>>(1)?,
                        ))
                    },
                )
                .optional()?;
            let Some((d, h)) = row else { return Ok(None) };
            let d = from_blob(&d.unwrap_or_default(), "distance")?;
            let h = from_blob(&h.unwrap_or_default(), "height")?;
            Ok(Some(LocalObstruction::new(lat, lon, Some(d), Some(h))))
        })
    }

    /// `LocalObstructionFindOrCreateDefault`: the stored entry, or the all-zero default
    /// (`LocalObstruction::new(lat, lon, None, None)`), stored only if `save`.
    pub fn local_obstruction_find_or_create_default(
        &self,
        model: ElevationModel,
        lat: f64,
        lon: f64,
        save: bool,
    ) -> Result<LocalObstruction, ElevationError> {
        if let Some(o) = self.local_obstruction_find(model, lat, lon)? {
            return Ok(o);
        }
        let o = LocalObstruction::new(lat, lon, None, None);
        if save {
            self.local_obstruction_insert_or_update_if_newer(model, &o, Utc::now())?;
        }
        Ok(o)
    }

    /// `LocalObstructionInsertOrUpdateIfNewer`: insert if absent, replace if the stored
    /// `LastUpdated` (unix seconds) is older than `at`. Returns `true` if a row was written.
    pub fn local_obstruction_insert_or_update_if_newer(
        &self,
        model: ElevationModel,
        o: &LocalObstruction,
        at: DateTime<Utc>,
    ) -> Result<bool, ElevationError> {
        let secs = at.timestamp();
        self.with_conn(model, |conn| {
            let existing: Option<Option<i64>> = conn
                .query_row(
                    "SELECT LastUpdated FROM LocalObstruction WHERE Lat = ?1 AND Lon = ?2",
                    params![o.lat, o.lon],
                    |r| r.get(0),
                )
                .optional()?;
            let n = match existing {
                None => conn.execute(
                    "INSERT INTO LocalObstruction (Lat, Lon, Distance, Height, LastUpdated)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![o.lat, o.lon, to_blob(&o.distance), to_blob(&o.height), secs],
                )?,
                Some(lu) if lu.unwrap_or(i64::MIN) < secs => conn.execute(
                    "UPDATE LocalObstruction SET Distance = ?3, Height = ?4, LastUpdated = ?5
                     WHERE Lat = ?1 AND Lon = ?2",
                    params![o.lat, o.lon, to_blob(&o.distance), to_blob(&o.height), secs],
                )?,
                Some(_) => 0,
            };
            Ok(n > 0)
        })
    }

    /// `LocalObstructionDelete`. Returns `true` if a row was removed.
    pub fn local_obstruction_delete(
        &self,
        model: ElevationModel,
        lat: f64,
        lon: f64,
    ) -> Result<bool, ElevationError> {
        self.with_conn(model, |conn| {
            Ok(conn.execute(
                "DELETE FROM LocalObstruction WHERE Lat = ?1 AND Lon = ?2",
                params![lat, lon],
            )? > 0)
        })
    }
}
