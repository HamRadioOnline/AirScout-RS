//! The [`Services`] trait: what the app needs from the background services.

use super::*;
use crate::analysis::AnalysisJob;
use crate::tracker::{TrackInput, TrackShown};
use crate::wintest_nearest::NearestSettings;
use std::net::SocketAddr;

/// What the app needs from the background services.
pub trait Services {
    /// Queue a path computation (`UpdatePaths`).
    fn request_path(&mut self, req: PathRequest);
    /// Start the plane feed named in the settings. The workers fill `cache` and tag their events
    /// with `feed_id`.
    fn start_feed(
        &mut self,
        feed_id: u64,
        settings: &Settings,
        cache: Arc<PlaneInfoCache>,
    ) -> Result<(), String>;
    /// Ask the running feed to stop; returns at once.
    fn stop_feed(&mut self);
    /// Stored position of a station (`StationData.Database.LocationFind(call, loc)`), if the
    /// station database has one. The default knows none.
    fn station_position(&self, _call: &str, _loc: &str) -> Option<GPoint> {
        None
    }
    /// Load all stored stations (`LocationGetAll`, inside `area` if given) off the UI thread and
    /// answer with [`ServiceEvent::Stations`]. The default delivers nothing.
    fn load_stations(&mut self, _area: Option<GRect>) {}
    /// Restart the background workers (database updates, calculations) with new settings, after
    /// the Options dialog was confirmed. The default does nothing.
    /// `generation` tags the events of the new workers; the app drops those of older ones.
    fn restart_background(&mut self, _settings: &Settings, _generation: u64) {}
    /// Start the background workers once (after the settings load). The default does nothing.
    fn start_background(&mut self, _settings: &Settings, _generation: u64) {}
    /// Cancel the background workers (exit); returns at once. The default does nothing.
    fn stop_background(&mut self) {}
    /// Number of background worker threads still running (the calculators, updaters and
    /// preloader). A delete or clear is refused while this is above zero (#155). The default is
    /// idle (0).
    fn background_jobs(&self) -> usize {
        0
    }
    /// `PropagationData.Database.NearFieldSuppression` [m] (MapDlg.cs:1306): set only from here,
    /// when the services start and when Options is applied, never by a worker thread.
    fn set_near_field_suppression(&mut self, _m: f64) {}
    /// My station as it is now; the background calculator reads it per path. The default
    /// ignores it.
    fn set_my(&mut self, _call: &str, _lat: f64, _lon: f64) {}
    /// Stored QRV entries of a station (`QRVFind(call, loc)`), for the Options dialog. A small
    /// query that runs when the dialog opens or a station changes, never per frame. The
    /// default knows none.
    fn qrv_get_all(&self, _call: &str, _loc: &str) -> Vec<Qrv> {
        Vec::new()
    }
    /// Write QRV entries (`QRVInsertOrUpdateIfNewer`); returns how many were written. The
    /// default writes nothing.
    fn qrv_write(&mut self, _qrvs: &[Qrv]) -> Result<usize, String> {
        Ok(0)
    }
    /// The stored local obstruction at a position, or the all-zero default
    /// (`LocalObstructionFindOrCreateDefault`). A single small query. The default has none.
    fn obstruction_load(
        &self,
        _model: ElevationModel,
        _lat: f64,
        _lon: f64,
    ) -> Result<LocalObstruction, String> {
        Err("no elevation database".into())
    }
    /// Store a local obstruction (`LocalObstructionInsertOrUpdateIfNewer`, now); returns whether
    /// a row was written.
    fn obstruction_store(
        &mut self,
        _model: ElevationModel,
        _o: &LocalObstruction,
    ) -> Result<bool, String> {
        Err("no elevation database".into())
    }
    /// Start a horizon calculation; its progress arrives as `ServiceEvent::Horizon*` with
    /// `req.id`. A running one is cancelled.
    fn start_horizon(&mut self, _req: HorizonRequest) -> Result<(), String> {
        Err("no elevation database".into())
    }
    /// Cancel the running horizon calculation, if any.
    fn cancel_horizon(&mut self) {}
    /// Start the Spectrum Lab poller; it sets nothing but reads `record` (true while a plane is
    /// nearest) and answers with `ServiceEvent::Spectrum*`. A running one is cancelled.
    fn start_speclab(&mut self, _settings: &Settings, _record: Arc<AtomicBool>) {}
    /// Cancel the Spectrum Lab poller, if any.
    fn stop_speclab(&mut self) {}
    /// Run an analysis job on a worker thread; its events arrive as `ServiceEvent::Job { id, .. }`.
    /// A running job is cancelled. The default does nothing.
    fn run_analysis(&mut self, _id: u64, _job: AnalysisJob) {}
    /// Cancel the running analysis job, if any; returns at once.
    fn cancel_analysis(&mut self) {}
    /// Run a database maintenance job on a worker thread; its events arrive as
    /// `ServiceEvent::Job { id, .. }`. It does not touch the analysis job. The default does
    /// nothing.
    fn run_maintenance(&mut self, _id: u64, _job: crate::maintenance::MaintenanceJob) {}
    /// Ask the maintenance job `id` to stop; returns at once. A finished or unknown id is
    /// ignored. The default does nothing.
    fn cancel_maintenance(&mut self, _id: u64) {}
    /// Look up the ground elevation [m] of My and DX (`GetElevation`, MapDlg.cs:3649) on a
    /// worker thread and answer with `ServiceEvent::TrackHeights { id, .. }`. Never blocks. The
    /// default answers nothing, so tracking stays without values.
    fn request_ground_elevations(&mut self, _id: u64, _my: GPoint, _dx: GPoint) {}
    /// (Re)start the integrations the settings switch on: the Win-Test receiver
    /// (`server.activate`), the Track worker (`track.activate`) and the CAT worker
    /// (`cat.activate`). Running ones are stopped first. Their events carry `generation`.
    /// The web server (`server.activate`) restarts only when its config changed.
    /// Never does socket I/O on the caller's thread. The default does nothing.
    fn start_integrations(&mut self, _s: &Settings, _generation: u64, _cache: Arc<PlaneInfoCache>) {
    }
    /// Stop the integrations except the web server; returns at once. The default does
    /// nothing.
    fn stop_integrations(&mut self) {}
    /// Stop the integrations and wait at most `max` for the CAT worker to restore the rig
    /// (exit). The default only stops.
    fn stop_integrations_and_wait(&mut self, _max: Duration) {
        self.stop_integrations()
    }
    /// Store the input the Track worker steps from (its next step sees it); never blocks. The
    /// default drops it.
    fn track_input(&mut self, _i: TrackInput) {}
    /// What the running Track worker showed last. The default has none.
    fn track_shown(&self) -> Option<TrackShown> {
        None
    }
    /// The settings the Win-Test receiver answers ASSETPATH with, changed while it runs. The
    /// default ignores them.
    fn set_nearest_settings(&mut self, _n: NearestSettings) {}
    /// Address the Win-Test receiver is bound to, once it is. The default has none.
    fn wintest_local_addr(&self) -> Option<SocketAddr> {
        None
    }
    /// The settings the web routes read, changed while the server runs; never blocks. The
    /// default ignores them.
    fn set_web_settings(&mut self, _s: &Settings) {}
    /// Address the web server is bound to, once it is. The default has none.
    fn web_local_addr(&self) -> Option<SocketAddr> {
        None
    }
    /// Whether the web server listens; cheap, read every frame. The default is off.
    fn web_state(&self) -> crate::web_service::WebState {
        crate::web_service::WebState::Off
    }
}
