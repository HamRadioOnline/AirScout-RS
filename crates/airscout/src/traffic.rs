//! Traffic dialog (`TrafficDlg.cs`): the routes of all planes in a time range. The job reads
//! the range once and groups it by hex in memory; the C# runs one query per hex (:101-143,
//! finding). The dialog is `ui/dialogs/traffic.rs`.

use crate::jobs::{JobCtx, JobEvent};
use as_aircraft::positions::{AircraftPosition, PositionDb};
use as_geo::latlon::{self, GPoint};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::path::Path;

/// A jump of this many km or more between two reports starts a new route (:125).
pub const SPLIT_KM: f64 = 100.0;
/// Routes per `JobEvent::Routes`; thousands of hexes must not flood the event queue.
const BATCH: usize = 100;

/// Start and stop of the dialog (:61-68): the oldest and youngest position, `now` for a missing
/// one (an empty database).
pub fn traffic_range(
    oldest: Option<DateTime<Utc>>,
    youngest: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    (oldest.unwrap_or(now), youngest.unwrap_or(now))
}

/// Alpha of the route colour for an opacity of `percent` (:114, `(int)(percent * 255 / 100)`).
pub fn route_alpha(percent: u32) -> u8 {
    (percent.min(100) * 255 / 100) as u8
}

/// The routes of one plane (`points` oldest first). A new route starts when a report lies
/// [`SPLIT_KM`] or more from the last one of the current route. The report that triggers the
/// split starts the new route; the C# drops it (:127-137), not reproduced (finding). Routes with
/// fewer than two points are dropped.
pub fn split_routes(points: &[AircraftPosition]) -> Vec<Vec<GPoint>> {
    let mut out = Vec::new();
    let mut cur: Vec<GPoint> = Vec::new();
    for p in points {
        let g = GPoint::new(p.lat, p.lon);
        if let Some(last) = cur.last() {
            if latlon::distance(last.lat, last.lon, g.lat, g.lon) >= SPLIT_KM {
                out.push(std::mem::take(&mut cur));
            }
        }
        cur.push(g);
    }
    out.push(cur);
    out.retain(|r| r.len() >= 2);
    out
}

/// Job `Traffic`: the routes of every hex with a position in `from..=to`, in batches. Cancel is
/// checked per hex; a cancelled job reports nothing more.
pub fn run_traffic(
    ctx: &JobCtx,
    dir: &Path,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<(), String> {
    ctx.progress("Getting positions...");
    let rows = PositionDb::open(dir)
        .and_then(|db| db.get_all(from, to))
        .map_err(|e| e.to_string())?;
    if ctx.cancelled() {
        return Ok(());
    }
    // rows are oldest first, so each group is too
    let mut by_hex: BTreeMap<String, Vec<AircraftPosition>> = BTreeMap::new();
    for r in rows {
        by_hex.entry(r.hex.clone()).or_default().push(r);
    }
    let total = by_hex.len();
    let mut batch = Vec::new();
    let mut n = 0;
    for (i, (hex, pts)) in by_hex.iter().enumerate() {
        if ctx.cancelled() {
            return Ok(());
        }
        ctx.progress(format!("Calculating {hex} [{} of {total}]", i + 1));
        let routes = split_routes(pts);
        n += routes.len();
        batch.extend(routes);
        if batch.len() >= BATCH {
            ctx.send(JobEvent::Routes(std::mem::take(&mut batch)));
        }
    }
    if !batch.is_empty() {
        ctx.send(JobEvent::Routes(batch));
    }
    ctx.send(JobEvent::Done(format!("{n} routes of {total} planes")));
    Ok(())
}
