//! HISTORY mode of the app: the loaded positions, the player and the snapshot tick
//! (`MapDlg.cs:5370-5395`, `6547-6609`, `6908-6922`, `9442-9497`).

use super::*;
use crate::analysis::AnalysisJob;
use crate::history::{HistoryData, HistoryIndex};
use crate::jobs::JobEvent;
use crate::player::{self, PlayerButton};
use as_aircraft::enums::LifeMode;
use chrono::TimeZone;

/// The state between ON and OFF: the loaded positions and the player.
pub struct HistoryState {
    /// Id of the load job; its events are the only ones that fill this state.
    pub job_id: u64,
    /// The load runs; Load/Save are disabled meanwhile.
    pub loading: bool,
    /// A job other than the load runs; the other job buttons are disabled meanwhile.
    pub busy: bool,
    /// The positions changed (files loaded, positions cleared): reload them when the job is done.
    reload: bool,
    pub data: Arc<HistoryData>,
    pub index: HistoryIndex,
    /// Range of the player, inside `[data.oldest, data.youngest]`.
    pub min: DateTime<Utc>,
    pub max: DateTime<Utc>,
    /// Step width in s per tick; 0 is paused, negative plays backwards.
    pub interval_s: i64,
    /// The plane cache of the snapshot: refilled on every tick.
    pub cache: PlaneInfoCache,
    /// Distinct report times for the markers of the time slider.
    pub markers: Vec<DateTime<Utc>>,
    pub status: String,
    /// `Time_Mode_Online` before ON; OFF restores it.
    prev_online: bool,
}

impl HistoryState {
    fn new(job_id: u64, prev_online: bool) -> Self {
        let epoch = Utc.timestamp_opt(0, 0).unwrap();
        Self {
            job_id,
            loading: true,
            busy: false,
            reload: false,
            data: Arc::default(),
            index: HistoryIndex::default(),
            min: epoch,
            max: epoch,
            interval_s: 0,
            cache: PlaneInfoCache::new(),
            markers: Vec::new(),
            status: String::new(),
            prev_online,
        }
    }
}

impl AppState {
    /// ON (MapDlg.cs:6547): refused while playing. Switches to HISTORY, stops the feed and the
    /// background workers, the Win-Test receiver and the Track worker (`StopAllBackgroundWorkers`,
    /// `MapDlg.cs:6558`, `:2484-2486`) and loads the positions off the UI thread.
    ///
    /// The original keeps the position maintainer running in HISTORY; the port stops it so it
    /// cannot delete rows under the analysis.
    pub fn enter_history(&mut self) -> Result<(), String> {
        if self.is_playing() {
            return Err("pause before entering HISTORY".into());
        }
        if let Some(why) = self.analysis_blocked() {
            return Err(why.into());
        }
        self.life_mode = LifeMode::History;
        self.pause();
        self.stop_background();
        self.stop_integrations();
        self.start_history_load();
        Ok(())
    }

    /// Why ON and the analysis jobs are refused now.
    ///
    /// Port deviation (finding #190): a running maintenance job blocks ON and the jobs of the
    /// Analysis tab, as an analysis job already blocks maintenance. The C# needs no check: its
    /// maintenance runs inside the modal Options dialog.
    pub fn analysis_blocked(&self) -> Option<&'static str> {
        self.maint
            .running
            .is_some()
            .then_some("a maintenance job is running")
    }

    /// Back to `Time_Mode_Online` from before ON (OFF and exit in HISTORY).
    pub(super) fn restore_time_mode(&mut self) {
        if let Some(h) = &self.history {
            self.settings.time.online = h.prev_online;
        }
    }

    /// OFF (MapDlg.cs:6563): cancel the job, drop the positions, restart the workers
    /// (`StartAllBackgroundWorkers`, `:6585`), the integrations included.
    pub fn leave_history(&mut self) {
        if self.life_mode != LifeMode::History {
            return;
        }
        self.restore_time_mode();
        self.stop_history();
        self.start_background();
        self.start_integrations();
        self.life_mode = LifeMode::Life;
        self.mode = RunMode::Paused;
    }

    /// Load all positions off the UI thread; `HistoryLoaded` follows.
    pub fn start_history_load(&mut self) {
        self.job_id += 1;
        let prev_online = self
            .history
            .as_ref()
            .map_or(self.settings.time.online, |h| h.prev_online);
        self.history = Some(HistoryState::new(self.job_id, prev_online));
        let ttl_min = self.settings.planes_position_ttl_min;
        self.services
            .run_analysis(self.job_id, AnalysisJob::LoadHistory { ttl_min });
    }

    /// Cancel the job and drop what it delivered. Events of the job that are still in the queue
    /// carry an old id and are dropped.
    pub fn stop_history(&mut self) {
        self.job_id += 1;
        self.history = None;
        self.services.cancel_analysis();
    }

    /// Id of the job started last.
    pub fn current_job_id(&self) -> u64 {
        self.job_id
    }

    /// Cancel the job `id` (closing a dialog that waits for it), if it is still the running one.
    pub fn cancel_job(&mut self, id: u64) {
        if id != self.job_id {
            return;
        }
        self.job_id += 1;
        if let Some(h) = &mut self.history {
            h.busy = false;
        }
        self.services.cancel_analysis();
    }

    /// Run a job other than the history load; it supersedes any running one.
    pub fn run_analysis_job(&mut self, job: AnalysisJob) {
        self.job_id += 1;
        if let Some(h) = &mut self.history {
            h.loading = false;
            h.busy = true;
            h.reload = matches!(
                job,
                AnalysisJob::LoadFiles(_) | AnalysisJob::ClearPositions | AnalysisJob::Download(_)
            );
        }
        self.services.run_analysis(self.job_id, job);
    }

    pub(super) fn on_job_event(&mut self, id: u64, event: JobEvent) {
        if id >= MAINT_ID_BASE {
            return self.on_maint_event(id, event);
        }
        if id != self.job_id {
            return;
        }
        match event {
            JobEvent::Progress(s) => self.status = s,
            JobEvent::Done(s) => {
                self.status = s;
                if self.history.as_ref().is_some_and(|h| h.reload) {
                    self.start_history_load();
                } else if let Some(h) = &mut self.history {
                    h.busy = false;
                }
            }
            JobEvent::Failed(s) => {
                if let Some(d) = self
                    .analysis_ui
                    .plane_dlg
                    .as_mut()
                    .filter(|d| d.job_id == id)
                {
                    d.fail(s.clone());
                }
                if let Some(d) = self
                    .analysis_ui
                    .crossing_dlg
                    .as_mut()
                    .filter(|d| d.job_id == id)
                {
                    d.fail(s.clone());
                }
                crate::ui::dialogs::traffic::on_failed(self, id, &s);
                if let Some(h) = &mut self.history {
                    h.loading = false;
                    h.busy = false;
                }
                self.status = format!("error: {s}");
            }
            JobEvent::HistoryLoaded(d) => self.on_history_loaded(id, *d),
            JobEvent::PlaneLists { hexes, calls } => {
                if let Some(d) = &mut self.analysis_ui.plane_dlg {
                    d.set_lists(hexes, calls);
                }
            }
            JobEvent::Routes(r) => crate::ui::dialogs::traffic::on_routes(self, id, r),
            JobEvent::Sizes(_)
            | JobEvent::Tables(..)
            | JobEvent::View { .. }
            | JobEvent::Locations(_)
            | JobEvent::Coverage { .. }
            | JobEvent::ElevationGrid(_) => {}
            JobEvent::Crossings(r) => {
                if let Some(d) = &mut self.analysis_ui.crossing_dlg {
                    d.set_result(*r);
                }
            }
        }
    }

    /// `bw_Analysis_DataGetter_RunWorkerCompleted` (MapDlg.cs:9442-9497).
    fn on_history_loaded(&mut self, id: u64, d: HistoryData) {
        let Some(h) = self.history.as_mut().filter(|h| h.job_id == id) else {
            return;
        };
        h.loading = false;
        let (Some(oldest), Some(youngest)) = (d.oldest, d.youngest) else {
            h.data = Arc::new(d);
            h.status = "No positions in the database".into();
            self.status = h.status.clone();
            return;
        };
        h.index = HistoryIndex::new(&d.positions);
        h.markers = h.index.markers(&d.positions);
        h.min = oldest;
        h.max = youngest;
        h.status = format!("{} positions loaded", d.positions.len());
        h.data = Arc::new(d);
        let status = h.status.clone();
        self.settings.time.offline = oldest;
        self.settings.time.online = false;
        self.status = match self.request_paths_for_mode() {
            Ok(()) => status,
            Err(e) => e,
        };
    }

    /// A player click sets the step width (MapDlg.cs:6865-6906).
    pub fn player(&mut self, b: PlayerButton) {
        if let Some(h) = &mut self.history {
            h.interval_s = player::step(h.interval_s, b);
        }
    }

    /// The Min/Max pickers: clamped to the loaded range, `min >= max` is refused.
    ///
    /// Not reproduced (GUI): the C# Max picker clamps the Min picker (copy-paste,
    /// MapDlg.cs:6934-6942) and every change resets the scrollbar.
    pub fn set_history_range(
        &mut self,
        min: DateTime<Utc>,
        max: DateTime<Utc>,
    ) -> Result<(), String> {
        let Some(h) = self.history.as_mut() else {
            return Err("not in HISTORY".into());
        };
        let (Some(oldest), Some(youngest)) = (h.data.oldest, h.data.youngest) else {
            return Err("No positions in the database".into());
        };
        let (min, max) = (min.clamp(oldest, youngest), max.clamp(oldest, youngest));
        if min >= max {
            return Err("the start of the range must lie before its end".into());
        }
        (h.min, h.max) = (min, max);
        let at = self.settings.time.offline.clamp(min, max);
        self.settings.time.offline = at;
        self.history_tick(false);
        Ok(())
    }

    /// The scrollbar (MapDlg.cs:6908-6922): the offline time becomes `min + secs`, clamped; the
    /// snapshot is refilled and one tick runs. (The original does not refill the cache and
    /// shows stale planes; not reproduced.)
    pub fn scroll_to(&mut self, secs: i64) {
        let Some(h) = &self.history else { return };
        self.settings.time.offline = player::advance(h.min, secs, h.min, h.max);
        self.history_tick(false);
    }

    /// True if the `ti_Progress` timer runs: playing in LIFE, a non-zero step width in HISTORY.
    pub(super) fn ticking(&self) -> bool {
        match self.life_mode {
            LifeMode::History => self
                .history
                .as_ref()
                .is_some_and(|h| !h.loading && h.interval_s != 0 && h.data.oldest.is_some()),
            _ => self.is_playing(),
        }
    }

    /// `ti_Progress_Tick`, HISTORY branch (MapDlg.cs:5370-5395): advance the time, refill the
    /// snapshot (the last report of every hex in `[min, offline]`) and update the planes.
    pub(super) fn history_tick(&mut self, advance: bool) {
        let ttl = self.settings.planes_position_ttl_min;
        if !self
            .history
            .as_ref()
            .is_some_and(|h| !h.loading && h.data.oldest.is_some())
        {
            return;
        }
        self.resend_watch_paths();
        let Some(h) = self.history.as_ref() else {
            return;
        };
        let offline = if advance {
            player::advance(self.settings.time.offline, h.interval_s, h.min, h.max)
        } else {
            self.settings.time.offline
        };
        h.cache.clear();
        for p in h.index.latest_per_hex(&h.data.positions, h.min, offline) {
            h.cache.set(p);
        }
        let planes = h.cache.get_all(offline, ttl);
        self.settings.time.offline = offline;
        self.run_tick(planes, offline, false);
    }

    /// `PlayMode == FORWARD` (MapDlg.cs:6094): playing in LIFE, a positive step width in HISTORY
    /// (the signed width cannot tell Forward from Fast-forward, which C# excludes).
    pub(super) fn playing_forward(&self) -> bool {
        match (self.life_mode, &self.history) {
            (LifeMode::History, Some(h)) => h.interval_s > 0,
            (LifeMode::History, None) => false,
            _ => self.is_playing(),
        }
    }

    /// `WatchListUpdated` (MapDlg.cs:5101): the checked set changed, recompute the paths.
    pub(super) fn resend_watch_paths(&mut self) {
        if !std::mem::take(&mut self.watchlist_dirty) {
            return;
        }
        let targets = self.multi_targets();
        if targets.is_empty() {
            self.paths.clear();
            self.path_gen += 1;
            self.status = "no stations checked in the watchlist".into();
        } else {
            self.send_paths(targets);
            self.status = "calculating path".into();
        }
        self.path_note = None;
    }

    /// Validate the inputs of the current path mode and list the targets (the first part of
    /// `Play`, MapDlg.cs:3998).
    pub(super) fn path_targets(&mut self) -> Result<Vec<PathTarget>, String> {
        self.sync_my();
        // The settings hold the positions (they may be more precise than the 6-character text).
        let my_ok = self.my_locator_valid();
        let err = |st: &mut Self, e: String| {
            st.status = e.clone();
            Err(e)
        };
        if self.path_mode() == PathMode::Multi {
            if !my_ok {
                return err(self, "invalid locator for My".into());
            }
            let t = self.multi_targets();
            if t.is_empty() {
                return err(self, "no stations checked in the watchlist".into());
            }
            return Ok(t);
        }
        if !self.locators_valid() {
            return err(self, "invalid locator for My or DX".into());
        }
        if let Some(d) = self.path_too_long() {
            let e = format!(
                "path too long: {d:.0} km, limit {:.0} km (Path_MaxLength)",
                self.settings.path_max_length_km
            );
            return err(self, e);
        }
        let s = &self.settings;
        Ok(vec![PathTarget {
            call: s.dx_call.clone(),
            pos: GPoint::new(s.dx_lat, s.dx_lon),
        }])
    }

    /// Reset the per-run results and send the path request (`ClearAllPathsInMap`, `UpdatePaths`).
    pub(super) fn send_targets(&mut self, targets: Vec<PathTarget>, mode: PathMode) {
        self.path_note = None;
        self.tick = TickResult::default();
        self.watch_potential.clear();
        self.watchlist_dirty = false;
        let my_call = self.settings.my_call.clone();
        self.settings.my_calls_push(&my_call);
        if mode == PathMode::Single {
            // Put the DX station on the watchlist if it is not in yet; flag it when the path is
            // longer than `Path_MaxLength` (`UpdatePaths`, SINGLE).
            if let Some(item) = dx_watch_item(&self.settings) {
                let max = self.settings.watchlist_max_count;
                self.settings.watchlist.insert_capped(item, max);
            }
        }
        self.send_paths(targets);
    }

    /// Request the paths of the current mode without starting a feed (what `Play` does first).
    pub fn request_paths_for_mode(&mut self) -> Result<(), String> {
        let targets = self.path_targets()?;
        let mode = self.path_mode();
        self.send_targets(targets, mode);
        Ok(())
    }
}
