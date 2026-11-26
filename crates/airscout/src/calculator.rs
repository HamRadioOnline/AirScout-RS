//! Background path calculator (`PathCalculator`, MapDlg.cs:34-304): for each enabled elevation
//! model, one thread fills the propagation path database with the paths from My to every known
//! station in range, for every band, so the live view finds them stored.
//!
//! A calculator starts when the updaters of its model and of the stations have finished a run
//! ([`IdleSet`]), and runs when the station table changed since its last complete run (the
//! stamp file `calculator-<model>.stamp`). It never writes the stamp for a cancelled run, nor
//! for a run that left paths unstored.
//!
//! Before it calculates, it checks that the elevation data of the area is complete (every square
//! has tiles or is recorded as sea), as the original's updater does before it sets the model's
//! `COMPLETE` bit and the bounds to the area (ElevationDatabaseUpdater.cs:474-491). The result is
//! passed to each path as its [`Coverage`] instead of being written into the shared store state,
//! which the path service sets for the UI's paths. With incomplete data the run stops: every path
//! would come out invalid and none would be stored (`logic-findings.md` #98).
//!
//! Deviations from the original, none of which changes a calculated path:
//! - the stamp is per model; the original keeps one `StationsDatabase_TimeStamp` that the first
//!   calculator to finish sets, so the others see "unchanged" and skip (`logic-findings.md` #92);
//! - a cancelled run does not save the stamp (the original does, #92);
//! - the throttle waits before every path while the UI computes one (`ui_busy`), instead of the
//!   original's sleep after a path that was found stored (#93);
//! - ASTER models get their name and step width (the original has none for ASTER3/ASTER1, #91).

use crate::background::{sleep_cancellable, Led};
use crate::best_case::calculator_shift;
use crate::services::{local_obstruction, sea_file, Paths};
use as_elevation::{check_area, Coverage, ElevationError, ElevationModel};
use as_geo::band::Band;
use as_geo::latlon::{self, GPoint, GRect, EARTH_RADIUS_KM};
use as_geo::maidenhead;
use as_propagation::PathParams;
use as_settings::Settings;
use as_stations::default_antenna_height;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// The background jobs that have finished a run; the calculators wait for these.
#[derive(Default)]
pub(crate) struct IdleSet {
    set: Mutex<BTreeSet<Led>>,
    changed: Condvar,
}

impl IdleSet {
    pub(crate) fn mark(&self, led: Led) {
        self.set
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(led);
        self.changed.notify_all();
    }

    /// Wait until all `leds` are marked; false if `cancel` was set first.
    pub(crate) fn wait_for(&self, leds: &[Led], cancel: &AtomicBool) -> bool {
        let mut set = self.set.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            if leds.iter().all(|l| set.contains(l)) {
                return true;
            }
            set = self
                .changed
                .wait_timeout(set, Duration::from_millis(200))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

/// Pauses of the calculator.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// After every station ("keep cpu load low"): 100 ms.
    pub per_location: Duration,
    /// Poll interval while the UI computes a path (`Background_Calculations_ThreadWait`): 1000 ms.
    pub busy_wait: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            per_location: Duration::from_millis(100),
            busy_wait: Duration::from_millis(1000),
        }
    }
}

/// Everything one calculator needs.
pub struct Calculator {
    pub paths: Paths,
    pub settings: Settings,
    pub model: ElevationModel,
    pub cancel: Arc<AtomicBool>,
    pub timing: Timing,
}

/// Result of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// The station table has not changed since the last complete run.
    Skipped,
    Cancelled,
    /// The elevation data of the area lacks `missing` squares: nothing was calculated.
    Incomplete {
        missing: usize,
    },
    /// `found` paths were stored already, `created` were calculated, `unsaved` of those came out
    /// invalid (incomplete elevation data) and were not stored.
    Finished {
        found: usize,
        created: usize,
        unsaved: usize,
    },
}

/// One end of a path: position after the best-case shift and the ground elevation used [m].
/// The QRV key is the locator of the stored position, not of the shifted one.
struct End {
    pos: GPoint,
    elevation: f64,
}

/// The stamp file of `model` in the data directory (also a `--clean` target).
pub(crate) fn stamp_file_in(data_dir: &Path, model: ElevationModel) -> PathBuf {
    data_dir.join(format!(
        "calculator-{}.stamp",
        model.dir_name().to_lowercase()
    ))
}

impl Calculator {
    fn stamp_file(&self) -> PathBuf {
        stamp_file_in(&self.paths.data_dir, self.model)
    }

    /// Timestamps of the `Location` and `QRV` tables and of the user's QRV edits as text (empty if
    /// never set), and My's call and position. The original compares the file time of the whole
    /// station database, so QRV imports and the user's QRV edits count too; a moved My needs new
    /// paths as well.
    fn stations_stamp(&self) -> Result<String, String> {
        let one = |table: &str| -> Result<String, String> {
            let t = self
                .paths
                .stations
                .table_timestamp(table)
                .map_err(|e| format!("station database: {e}"))?;
            Ok(t.and_then(|t| t.timestamp_nanos_opt())
                .map_or_else(String::new, |n| n.to_string()))
        };
        let my = self.paths.my_station();
        Ok(format!(
            "{}:{}:{}:{}:{}:{}",
            one("Location")?,
            one("QRV")?,
            one(as_stations::QRV_USER_EDIT_MARKER)?,
            my.call,
            my.lat,
            my.lon
        ))
    }

    fn changed_since_last_run(&self, current: &str) -> bool {
        std::fs::read_to_string(self.stamp_file()).map_or(true, |old| old.trim() != current)
    }

    fn end(&self, call: &str, pos: GPoint, band: Band) -> Result<(End, f64), String> {
        let model = self.model;
        let s = &self.settings;
        let store = &self.paths.store;
        let loc = maidenhead::loc_from_lat_lon(pos.lat, pos.lon, false, 3, false)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("invalid position {}, {}", pos.lat, pos.lon))?;
        let mut elevation = f64::from(
            store
                .elevation(model, pos.lat, pos.lon, true)
                .map_err(|e| format!("elevation: {e}"))?,
        );
        let mut at = pos;
        if s.path_best_case_elevation {
            let shift =
                calculator_shift(model, pos, |m, l| store.max_elv_loc(m, l).ok().flatten())?;
            if let Some(info) = shift {
                at = GPoint::new(info.max_lat, info.max_lon);
                elevation = f64::from(info.max_elv);
            }
        }
        let q = self
            .paths
            .stations
            .qrv_find(call, &loc, band)
            .map_err(|e| format!("station database: {e}"))?;
        let height = match q {
            Some(q) if q.antenna_height != 0.0 => q.antenna_height,
            _ => default_antenna_height(band),
        };
        Ok((End { pos: at, elevation }, height))
    }

    /// Wait while the UI computes a path; false if cancelled meanwhile.
    fn wait_for_ui(&self) -> bool {
        while self.paths.ui_busy.load(Ordering::SeqCst) {
            if sleep_cancellable(self.timing.busy_wait, &self.cancel) {
                return false;
            }
        }
        !self.cancel.load(Ordering::Relaxed)
    }

    /// Find or create the path of one station and band; `Ok(None)` if cancelled.
    fn one_path(
        &self,
        call: &str,
        pos: GPoint,
        band: Band,
        coverage: Coverage,
        progress: &mut dyn FnMut(String),
    ) -> Result<Option<Created>, String> {
        let s = &self.settings;
        let started = Instant::now();
        // My is read per path, as the original reads the settings
        let my_st = self.paths.my_station();
        if !maidenhead::check_lat_lon(my_st.lat, my_st.lon) {
            return Err("invalid position of My".into());
        }
        let my_pos = GPoint::new(my_st.lat, my_st.lon);
        let (my, my_height) = self.end(&my_st.call, my_pos, band)?;
        let (dx, dx_height) = self.end(call, pos, band)?;
        // the obstruction is stored at the settings position, not at the shifted one (#105)
        let obstruction = local_obstruction(&self.paths.store, self.model, my_pos, pos, my_height)?;
        let bs = s.band_settings.get(&band).copied().unwrap_or_default();
        let p = PathParams {
            lat1: my.pos.lat,
            lon1: my.pos.lon,
            h1: my.elevation + my_height,
            lat2: dx.pos.lat,
            lon2: dx.pos.lon,
            h2: dx.elevation + dx_height,
            qrg_ghz: band.to_ghz(),
            radius_km: EARTH_RADIUS_KM * bs.k_factor,
            f1_clearance: bs.f1_clearance,
            step_width_m: self.model.default_step_width_m(),
            local_obstruction: obstruction,
        };
        let engine = &self.paths.engine;
        if engine
            .find_path(self.model, &p)
            .map_err(|e| format!("path lookup: {e}"))?
            .is_some()
        {
            return Ok(Some(Created::Found));
        }
        let Some(pp) = engine
            .create_path_in(self.model, &p, true, Some(&self.cancel), Some(coverage))
            .map_err(|e| format!("path calculation: {e}"))?
        else {
            return Ok(None);
        };
        progress(format!(
            "{} calculating path[ {}]: {}<>{}, {} ms.",
            self.model.dir_name(),
            band.name(),
            my_st.call,
            call,
            started.elapsed().as_millis()
        ));
        Ok(Some(if pp.valid {
            Created::Stored
        } else {
            Created::Unsaved
        }))
    }

    /// Run once (`OnDoWork` without the wait loop and the periodic repeat).
    pub fn run_once(&self, progress: &mut dyn FnMut(String)) -> Result<RunOutcome, String> {
        let name = self.model.dir_name();
        let s = &self.settings;
        let current = self.stations_stamp()?;
        if !self.changed_since_last_run(&current) {
            return Ok(RunOutcome::Skipped);
        }
        progress(format!("{name} started."));
        let a = &s.area;
        let rect = GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon);
        let sea = sea_file(&self.paths.data_dir, self.model);
        let area = match check_area(
            &self.paths.store,
            self.model,
            rect,
            &sea,
            Some(&self.cancel),
            &mut |_| {},
        ) {
            Err(ElevationError::Cancelled) => {
                progress(format!("{name} cancelled."));
                return Ok(RunOutcome::Cancelled);
            }
            Err(e) => return Err(format!("elevation data: {e}")),
            Ok(a) => a,
        };
        if !area.complete {
            progress(format!(
                "{name} elevation data incomplete ({} squares missing), no paths calculated.",
                area.missing.len()
            ));
            return Ok(RunOutcome::Incomplete {
                missing: area.missing.len(),
            });
        }
        let coverage = Coverage {
            complete: true,
            bounds: rect,
        };
        progress(format!("{name} getting locations..."));
        let locations = self
            .paths
            .stations
            .location_get_all(Some(rect))
            .map_err(|e| format!("station database: {e}"))?;
        progress(format!("{name} checking locations..."));
        let (mut found, mut created, mut unsaved) = (0, 0, 0);
        let bands: Vec<Band> = s
            .band_settings
            .keys()
            .copied()
            .filter(|b| !matches!(b, Band::None | Band::All))
            .collect();
        for ld in &locations {
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            let my = self.paths.my_station();
            let valid = maidenhead::check_lat_lon(my.lat, my.lon)
                && maidenhead::check_lat_lon(ld.lat, ld.lon);
            if valid && latlon::distance(my.lat, my.lon, ld.lat, ld.lon) <= s.path_max_length_km {
                for &band in &bands {
                    if !self.wait_for_ui() {
                        break;
                    }
                    match self.one_path(
                        &ld.call,
                        GPoint::new(ld.lat, ld.lon),
                        band,
                        coverage,
                        progress,
                    ) {
                        Ok(Some(Created::Found)) => found += 1,
                        Ok(Some(Created::Stored)) => created += 1,
                        Ok(Some(Created::Unsaved)) => {
                            created += 1;
                            unsaved += 1;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            progress(format!("{name} error processing call [{}]: {e}", ld.call));
                            break;
                        }
                    }
                }
            }
            if sleep_cancellable(self.timing.per_location, &self.cancel) {
                break;
            }
        }
        if self.cancel.load(Ordering::Relaxed) {
            progress(format!("{name} cancelled."));
            return Ok(RunOutcome::Cancelled);
        }
        if unsaved == 0 {
            std::fs::write(self.stamp_file(), &current).map_err(|e| format!("stamp file: {e}"))?;
        } else {
            // the next run tries the unstored paths again
            progress(format!("{name} {unsaved} paths not stored."));
        }
        progress(format!("{name} finished."));
        Ok(RunOutcome::Finished {
            found,
            created,
            unsaved,
        })
    }

    /// The thread body: wait for the stations and this model's elevation updater, then run;
    /// repeat every `period` if given.
    pub(crate) fn run_loop(
        &self,
        idle: &IdleSet,
        period: Option<Duration>,
        progress: &mut dyn FnMut(String),
    ) {
        let leds = [Led::Stations, Led::Elevation(self.model)];
        if !idle.wait_for(&leds, &self.cancel) {
            return;
        }
        loop {
            match self.run_once(progress) {
                Ok(RunOutcome::Cancelled) => return,
                Ok(_) => {}
                Err(e) => progress(format!("{} error: {e}", self.model.dir_name())),
            }
            match period {
                Some(p) if !sleep_cancellable(p, &self.cancel) => {}
                _ => return,
            }
        }
    }
}

/// How `one_path` got its path.
enum Created {
    Found,
    Stored,
    Unsaved,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_set_waits_for_all_leds_and_for_cancel() {
        let idle = Arc::new(IdleSet::default());
        let cancel = AtomicBool::new(false);
        let leds = [Led::Stations, Led::Elevation(ElevationModel::Globe)];
        idle.mark(Led::Stations);
        let i2 = idle.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            i2.mark(Led::Elevation(ElevationModel::Globe));
        });
        assert!(idle.wait_for(&leds, &cancel));
        t.join().unwrap();
        // an LED that never comes: only the cancel ends the wait
        cancel.store(true, Ordering::Relaxed);
        assert!(!idle.wait_for(&[Led::Aircraft], &cancel));
    }
}
