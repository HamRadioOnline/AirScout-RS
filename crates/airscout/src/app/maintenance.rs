//! Database maintenance of the app (Options Database tab, maintenance dialog, single station
//! delete): the state, the confirmation flow and the job events.
//!
//! Maintenance jobs have ids of their own (from [`MAINT_ID_BASE`] up), so they never disturb the
//! analysis job and its events. Every job that removes data waits for an explicit Yes, is refused
//! while playing, in HISTORY with a job running, or while another maintenance job runs.

use super::*;
use crate::coverage::GridRequest;
use crate::history_download::{DiskSpace, Fs4Disk};
use crate::jobs::JobEvent;
use crate::maintenance::{DbId, MaintenanceJob};
use crate::map_preloader::map_mirror_ok;
use crate::ui::dialogs::db_maintenance::DbMaintDlg;
use crate::ui::dialogs::delete_station::DeleteStationDlg;
use crate::ui::dialogs::download_area::DownloadAreaDlg;
use crate::ui::options::{MaintAction, ViewRequest};

/// First id of the maintenance jobs; analysis job ids stay far below.
pub const MAINT_ID_BASE: u64 = 1 << 62;

/// The state of the maintenance dialogs and the running job.
#[derive(Default)]
pub struct MaintState {
    seq: u64,
    /// Id of the running job.
    pub running: Option<u64>,
    /// The running job removes data (`MaintenanceJob::destructive`): the Options OK/Cancel stay
    /// disabled until it ends, so the restarted workers cannot race the delete/clear (#155).
    pub destructive: bool,
    /// File path and size [MB] of every database, as of the last `Sizes` job.
    pub sizes: Vec<(DbId, String, f64)>,
    /// The job that waits for Yes or No.
    pub confirm: Option<MaintenanceJob>,
    pub db_dlg: Option<DbMaintDlg>,
    pub station_dlg: Option<DeleteStationDlg>,
    /// Id of the coverage job of the Options Elevation tab. Coverage and grid jobs only read, so
    /// they are not "running" in the sense of [`Self::running`] and block nothing.
    pub coverage_job: Option<u64>,
    /// Id of the newest elevation grid job of the station map.
    pub grid_job: Option<u64>,
}

impl AppState {
    /// True if a job may start now (the maintenance buttons are enabled).
    pub fn maintenance_allowed(&self) -> bool {
        self.maintenance_blocked(true).is_none()
    }

    /// Give the Options dialog what its Database tab shows.
    pub fn feed_options_dlg(&self, dlg: &mut OptionsDlg) {
        dlg.sizes.clone_from(&self.maint.sizes);
        dlg.maint_enabled = self.maintenance_allowed();
        dlg.maint_running = self.maint.running.is_some() && self.maint.destructive;
        dlg.maint_waiting = self.background_workers_waiting();
        dlg.web_state = self.services.web_state();
    }

    /// The number of background workers still stopping, if any; the Database tab shows it while
    /// the destructive maintenance buttons stay disabled (#155).
    fn background_workers_waiting(&self) -> Option<usize> {
        let n = self.services.background_jobs();
        (n > 0).then_some(n)
    }

    /// A button of the Database tab: the delete buttons ask first.
    pub fn on_maint_action(&mut self, a: MaintAction) {
        let job = match a {
            MaintAction::DeleteElevationPaths => MaintenanceJob::DeleteElevationPaths,
            MaintAction::DeletePropagationPaths => MaintenanceJob::DeletePropagationPaths,
            MaintAction::DeleteMapTiles => MaintenanceJob::DeleteMapTiles,
            MaintAction::DeleteStation => return self.open_delete_station(),
            MaintAction::Maintenance(db) => return self.open_db_maintenance(db),
        };
        if let Err(e) = self.request_maintenance(job) {
            self.status = e;
        }
    }

    /// Why no maintenance job may start now. A read-only job (`destructive == false`, such as
    /// `Sizes`) is not refused just because background workers are still stopping, so reading the
    /// database sizes never fails silently (#155).
    fn maintenance_blocked(&self, destructive: bool) -> Option<&'static str> {
        if self.is_playing() {
            Some("pause before database maintenance")
        } else if self.maint.running.is_some() {
            Some("a maintenance job is running")
        } else if self.history.as_ref().is_some_and(|h| h.loading || h.busy) {
            Some("a history job is running")
        } else if destructive && self.services.background_jobs() > 0 {
            Some("wait for the background workers to stop")
        } else {
            None
        }
    }

    fn start_maintenance(&mut self, job: MaintenanceJob) {
        self.maint.seq += 1;
        let id = MAINT_ID_BASE + self.maint.seq;
        self.maint.running = Some(id);
        self.maint.destructive = job.destructive();
        self.services.run_maintenance(id, job);
    }

    /// Start a maintenance job. A job that removes data (and `View`, as the C# asks too) only
    /// asks first: [`Self::confirm_maintenance`] starts it. Refused while playing, while a job
    /// runs and in HISTORY with a job running.
    pub fn request_maintenance(&mut self, job: MaintenanceJob) -> Result<(), String> {
        if let Some(why) = self.maintenance_blocked(job.destructive()) {
            return Err(why.into());
        }
        if job.confirmation().is_some() {
            self.maint.confirm = Some(job);
        } else {
            self.start_maintenance(job);
        }
        Ok(())
    }

    /// Yes: send the job that was asked about. The state is checked again.
    pub fn confirm_maintenance(&mut self) -> Result<(), String> {
        let job = self.maint.confirm.take().ok_or("nothing to confirm")?;
        if let Some(why) = self.maintenance_blocked(job.destructive()) {
            self.status = why.into();
            return Err(why.into());
        }
        self.start_maintenance(job);
        Ok(())
    }

    /// No, or the dialog was closed: nothing is deleted.
    pub fn cancel_confirmation(&mut self) {
        self.maint.confirm = None;
    }

    /// Read the database sizes for the Options tab (a read, no question).
    pub(super) fn request_sizes(&mut self) {
        let _ = self.request_maintenance(MaintenanceJob::Sizes);
    }

    /// "Database Maintenance" of the Options tab.
    pub fn open_db_maintenance(&mut self, db: DbId) {
        if self.maint.db_dlg.is_some() {
            return;
        }
        self.maint.db_dlg = Some(DbMaintDlg::new(db));
        if let Err(e) = self.request_maintenance(MaintenanceJob::Tables(db)) {
            if let Some(d) = &mut self.maint.db_dlg {
                d.error = Some(e);
            }
        }
    }

    /// "Delete Single Station" of the Options tab.
    pub fn open_delete_station(&mut self) {
        if self.maint.station_dlg.is_none() {
            self.maint.station_dlg = Some(DeleteStationDlg::default());
        }
    }

    /// "Download data for the area" of the menu.
    pub fn open_download_area(&mut self) {
        if self.download_area_dlg.is_none() {
            self.download_area_dlg = Some(DownloadAreaDlg::new(&self.settings));
        }
    }

    /// Free space of the volume that holds the data directory, for the download dialog.
    pub fn free_disk_bytes(&self) -> Option<u64> {
        Fs4Disk.available(self.data_dir.as_deref()?).ok()
    }

    /// Start the area download: the map part from the provider's mirror (checked again; no
    /// mirror makes the map part unavailable, never a bulk fetch of a provider's own server),
    /// the elevation part for the enabled models. Refused while playing, while a job runs, and
    /// while background workers still run (#155): the download counts as one while it runs.
    pub fn start_download_area(
        &mut self,
        maps: bool,
        zoom: u8,
        elevation: bool,
    ) -> Result<(), String> {
        if let Some(why) = self.maintenance_blocked(false) {
            return Err(why.into());
        }
        if self.services.background_jobs() > 0 {
            return Err("wait for the background workers to stop".into());
        }
        let s = &self.settings;
        let maps = if maps {
            map_mirror_ok(&s.gui.map_provider, &s.background.map_mirror_url)
                .ok()
                .map(|_| (s.background.map_mirror_url.clone(), zoom))
        } else {
            None
        };
        let elevation = elevation.then(|| s.gui.elevation_base_url.clone());
        if maps.is_none() && elevation.is_none() {
            return Err("select Maps or Elevation".into());
        }
        let a = &s.area;
        let area = GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon);
        let job = MaintenanceJob::DownloadArea {
            maps,
            elevation,
            models: s.elevation_enabled,
            area,
        };
        self.start_maintenance(job);
        Ok(())
    }

    /// Cancel the running area download `id`, if any (a finished or unknown id is ignored).
    pub fn cancel_download_area(&mut self, id: Option<u64>) {
        if let Some(id) = id {
            self.services.cancel_maintenance(id);
        }
    }

    /// A request of the coverage view or the station map; a newer job of a kind supersedes the
    /// older one, and a cancel is a job-id match, so a job that finished is left alone.
    pub fn on_view_request(&mut self, request: ViewRequest) {
        match request {
            ViewRequest::Coverage {
                model,
                base_url,
                area,
            } => {
                self.cancel_coverage();
                let job = MaintenanceJob::Coverage {
                    model,
                    base_url,
                    area,
                };
                self.maint.coverage_job = Some(self.start_view_job(job));
            }
            ViewRequest::CancelCoverage => self.cancel_coverage(),
            ViewRequest::Grid(GridRequest::Run { rect, model }) => {
                self.cancel_grid();
                let job = MaintenanceJob::ElevationGrid { rect, model };
                self.maint.grid_job = Some(self.start_view_job(job));
            }
            ViewRequest::Grid(GridRequest::Cancel) => self.cancel_grid(),
        }
    }

    fn start_view_job(&mut self, job: MaintenanceJob) -> u64 {
        self.maint.seq += 1;
        let id = MAINT_ID_BASE + self.maint.seq;
        self.services.run_maintenance(id, job);
        id
    }

    fn cancel_coverage(&mut self) {
        if let Some(id) = self.maint.coverage_job.take() {
            self.services.cancel_maintenance(id);
        }
    }

    fn cancel_grid(&mut self) {
        if let Some(id) = self.maint.grid_job.take() {
            self.services.cancel_maintenance(id);
        }
    }

    /// The Options dialog closed: its views have no use for their jobs any more.
    pub fn cancel_view_jobs(&mut self) {
        self.cancel_coverage();
        self.cancel_grid();
    }

    /// The answer of a coverage or grid job (events of superseded jobs are dropped by the id).
    fn on_view_event(&mut self, id: u64, event: JobEvent) {
        let is_coverage = self.maint.coverage_job == Some(id);
        match event {
            JobEvent::Progress(s) => self.status = s,
            JobEvent::Coverage { found, missing } if is_coverage => {
                self.maint.coverage_job = None;
                if let Some(o) = &mut self.options_dlg {
                    o.set_coverage(&found, &missing);
                }
            }
            JobEvent::ElevationGrid(cells) if !is_coverage => {
                self.maint.grid_job = None;
                if let Some(o) = &mut self.options_dlg {
                    o.set_grid(cells);
                }
            }
            JobEvent::Failed(s) => {
                if is_coverage {
                    self.maint.coverage_job = None;
                    if let Some(o) = &mut self.options_dlg {
                        o.coverage_failed(&s);
                    }
                } else {
                    self.maint.grid_job = None;
                    if let Some(o) = &mut self.options_dlg {
                        o.grid_failed(&s);
                    }
                }
                self.status = format!("error: {s}");
            }
            _ => {}
        }
    }

    pub(super) fn on_maint_event(&mut self, id: u64, event: JobEvent) {
        if self.maint.coverage_job == Some(id) || self.maint.grid_job == Some(id) {
            return self.on_view_event(id, event);
        }
        if self.maint.running != Some(id) {
            return;
        }
        // The "Download data for the area" dialog follows its own job's progress.
        let is_download = self
            .download_area_dlg
            .as_ref()
            .is_some_and(|d| d.running == Some(id));
        if is_download
            && matches!(
                event,
                JobEvent::Progress(_) | JobEvent::Done(_) | JobEvent::Failed(_)
            )
        {
            let d = self.download_area_dlg.as_mut().unwrap();
            if let Some(text) = d.on_event(event) {
                self.status = text;
            }
            if d.running.is_none() {
                self.maint.running = None;
                self.maint.destructive = false;
            }
            return;
        }
        match event {
            JobEvent::Progress(s) => {
                self.status = s;
                return;
            }
            JobEvent::Done(s) => {
                self.status = s;
                if let Some(d) = &mut self.maint.station_dlg {
                    d.deleted();
                }
            }
            JobEvent::Failed(s) => {
                if let Some(d) = &mut self.maint.db_dlg {
                    d.fail(s.clone());
                }
                if let Some(d) = &mut self.maint.station_dlg {
                    d.error = Some(s.clone());
                }
                self.status = format!("error: {s}");
            }
            JobEvent::Sizes(v) => self.maint.sizes = v,
            JobEvent::Tables(db, t) => {
                if let Some(d) = self.maint.db_dlg.as_mut().filter(|d| d.db == db) {
                    d.set_tables(t);
                }
            }
            JobEvent::View { columns, rows } => {
                if let Some(d) = &mut self.maint.db_dlg {
                    d.set_view(columns, rows);
                }
            }
            JobEvent::Locations(v) => {
                if let Some(d) = &mut self.maint.station_dlg {
                    d.set_locations(v);
                }
            }
            _ => {}
        }
        self.maint.running = None;
        self.maint.destructive = false;
    }
}
