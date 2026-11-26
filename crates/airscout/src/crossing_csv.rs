//! The export of `CrossingHistoryDlg` (CrossingHistoryDlg.cs:302-402): the ambiguity pass, the
//! CSV and its file name. See `crossing_history.rs` for what is ported 1:1.
//!
//! Port choices: the CSV is culture independent and its text fields have `;`, CR and LF replaced
//! by a blank. `signal = f64::MIN` is written as `-255.00` (`F2` of -255), `Ambiguous` as
//! `True`/`False` and `Category` as the C# enum name (`MEDIUM`). The C# header has no
//! separators (not reproduced, finding #126).

use crate::crossing_history::CrossingResult;
use crate::exports::{f2, f8, flat, write_tmp};
use crate::jobs::{JobCtx, JobEvent};
use crate::ui::dialogs::horizon::CSV_SEPARATOR;
use as_aircraft::PlaneInfo;
use as_geo::latlon;
use as_propagation::PropagationPath;
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::Path;

/// The 21 columns of the CSV.
pub const CROSSING_HEADER: &str = "time[utc];call;lat[deg];lon[deg];type;category;potential;track[deg];alt[m];altdiff[m];dist[km];crossangle[deg];squint[deg];dist1[km];dist2[km];eps1[deg];eps2[deg];theta1[deg];theta2[deg];signal_strength[dB];ambiguous";

/// The ambiguity pass of the export: split into groups where a gap to the previous sample is
/// strictly greater than `gap_s`, and mark each group ambiguous iff it contains at least two
/// unique aircraft hexes.
pub fn mark_ambiguous(c: &mut [PlaneInfo], gap_s: f64) {
    let n = c.len();
    if n == 0 {
        return;
    }
    c.iter_mut().for_each(|p| p.ambiguous = false);
    let mut start = 0usize;
    for i in 1..=n {
        let split = i == n || {
            let diff = (c[i].time - c[i - 1].time).num_milliseconds() as f64 / 1000.0;
            diff > gap_s
        };
        if split {
            let mut hexes: HashSet<&str> = HashSet::new();
            for p in &c[start..i] {
                hexes.insert(p.hex.as_str());
            }
            if hexes.len() > 1 {
                for p in &mut c[start..i] {
                    p.ambiguous = true;
                }
            }
            start = i;
        }
    }
}

/// A text field of the CSV without separators and line breaks.
fn clean(s: &str) -> String {
    s.replace([';', '\r', '\n'], " ")
}

/// The CSV (:349-397): header and one row per crossing. `path` gives the end points for the
/// distances to station 1 and 2.
pub fn write_crossing_csv(
    w: &mut dyn Write,
    crossings: &[PlaneInfo],
    path: &PropagationPath,
) -> io::Result<()> {
    let sep = CSV_SEPARATOR;
    writeln!(w, "{CROSSING_HEADER}")?;
    let pp = &path.params;
    let deg = |rad: f64| f8(rad / std::f64::consts::PI * 180.0);
    for c in crossings {
        let a1 = latlon::distance(pp.lat1, pp.lon1, c.lat, c.lon);
        let a2 = latlon::distance(pp.lat2, pp.lon2, c.lat, c.lon);
        let signal = if c.signal_strength == f64::MIN {
            -255.0
        } else {
            c.signal_strength
        };
        let fields = [
            c.time.format("%Y-%m-%d %H:%M:%S").to_string(),
            clean(&c.call),
            f8(c.lat),
            f8(c.lon),
            clean(&c.type_code),
            // C# `Category.ToString()`: the enum name (PLANECATEGORY: NONE, LIGHT, MEDIUM, ...)
            c.category.as_str().to_uppercase(),
            c.potential.to_string(),
            f8(c.track),
            f8(c.alt_m()),
            f8(c.alt_diff),
            f8(c.int_qrb),
            deg(c.angle),
            deg(c.squint),
            f8(a1),
            f8(a2),
            deg(c.eps1),
            deg(c.eps2),
            deg(c.theta1),
            deg(c.theta2),
            f2(signal),
            // C# `bool.ToString()`
            if c.ambiguous { "True" } else { "False" }.to_string(),
        ];
        writeln!(w, "{}", fields.join(sep))?;
    }
    Ok(())
}

/// `Path Crossing History_<my>_<dx>_<from>_to_<to>.csv` (:327).
pub fn crossing_file_name(
    my_call: &str,
    dx_call: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> String {
    let f = "%Y%m%d_%H%M%S";
    format!(
        "Path Crossing History_{}_{}_{}_to_{}.csv",
        flat(my_call),
        flat(dx_call),
        from.format(f),
        to.format(f)
    )
}

/// Job `ExportCrossings` (`btn_History_Export_Click`, :302-402): the ambiguity pass, then the CSV
/// in `<data>/Tmp/`.
pub fn export_crossings(
    ctx: &JobCtx,
    dir: &Path,
    result: &CrossingResult,
    gap_s: f64,
    range: (DateTime<Utc>, DateTime<Utc>),
    calls: (&str, &str),
) -> Result<(), String> {
    if result.crossings.is_empty() {
        ctx.send(JobEvent::Done("No crossings to export.".into()));
        return Ok(());
    }
    ctx.progress("Checking ambiguous crossings...");
    let mut crossings = result.crossings.clone();
    mark_ambiguous(&mut crossings, gap_s);
    let name = crossing_file_name(calls.0, calls.1, range.0, range.1);
    let written = write_tmp(dir, &name, |w| {
        write_crossing_csv(w, &crossings, &result.path).map(|()| true)
    })?;
    if let Some(path) = written {
        ctx.send(JobEvent::Done(format!(
            "{} crossings written to {}",
            crossings.len(),
            path.display()
        )));
    }
    Ok(())
}
