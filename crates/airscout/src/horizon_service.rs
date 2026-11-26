//! Radio horizon computation of `HorizonDlg`: one thread per request, with live progress.

use crate::services::{Paths, ServiceEvent};
use as_elevation::ElevationModel;
use as_geo::latlon::GPoint;
use as_propagation::HorizonParams;
use as_settings::Settings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;

/// `HorizonDlg.CalculationDistance` [km].
pub const CALCULATION_DISTANCE_KM: f64 = 500.0;

/// One horizon calculation.
#[derive(Debug, Clone)]
pub struct HorizonRequest {
    /// Echoed in the answers, so the dialog can drop events of superseded requests.
    pub id: u64,
    pub call: String,
    pub pos: GPoint,
    /// Antenna height above ground [m]: the stored QRV value, or the band default if that is 0.
    pub height_m: f64,
    /// Take the stored local obstruction at `pos` into account (My station only).
    pub with_obstruction: bool,
    pub settings: Settings,
}

/// Parameters of `PropagationHorizonFindOrCreate` as `HorizonDlg` passes them, with the ground
/// elevation [m] already known.
pub fn horizon_params(req: &HorizonRequest, model: ElevationModel, ground_m: f64) -> HorizonParams {
    let s = &req.settings;
    HorizonParams {
        lat: req.pos.lat,
        lon: req.pos.lon,
        h: ground_m + req.height_m,
        dist_km: CALCULATION_DISTANCE_KM,
        qrg_ghz: s.band.to_ghz(),
        radius_km: s.radius_km(),
        f1_clearance: s.current_band_settings().f1_clearance,
        step_width_m: model.default_step_width_m(),
    }
}

/// Running calculation. Dropping the handle cancels it; the thread ends after the azimuths in
/// progress and nothing is sent after that.
pub struct HorizonHandle {
    cancel: Arc<AtomicBool>,
    _thread: Option<JoinHandle<()>>,
}

impl HorizonHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for HorizonHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub struct HorizonService;

impl HorizonService {
    /// Start the calculation on its own thread. Answers: `HorizonPoint` per azimuth, then
    /// `HorizonDone` or `HorizonFailed`.
    pub fn start(paths: Paths, req: HorizonRequest, tx: Sender<ServiceEvent>) -> HorizonHandle {
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        let tx_err = tx.clone();
        let id = req.id;
        let thread = std::thread::Builder::new()
            .name("horizon".into())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&paths, &req, &c, &tx)
                }));
                let result = outcome.unwrap_or_else(|_| {
                    Err("internal error while computing the horizon".to_string())
                });
                // a cancelled calculation reports nothing: nobody listens any more
                let event = match result {
                    Ok(Some(h)) => ServiceEvent::HorizonDone {
                        id,
                        horizon: Box::new(h),
                    },
                    Ok(None) => return,
                    Err(error) => ServiceEvent::HorizonFailed { id, error },
                };
                let _ = tx.send(event);
            })
            .map_err(|e| {
                let error = format!("cannot start the horizon thread: {e}");
                let _ = tx_err.send(ServiceEvent::HorizonFailed { id, error });
            })
            .ok();
        HorizonHandle {
            cancel,
            _thread: thread,
        }
    }
}

/// `HorizonDlg`: `ElevationData.Database[lat, lon, model]`, the raw lookup in the given model only
/// (no fallback to other models); a missing value reads as 0 (`setinvalidtozero` defaults to
/// true in the indexer).
pub fn ground_elevation_raw(
    store: &as_elevation::ElevationStore,
    model: ElevationModel,
    pos: GPoint,
) -> Result<f64, String> {
    store
        .elevation(model, pos.lat, pos.lon, true)
        .map(f64::from)
        .map_err(|e| format!("elevation: {e}"))
}

fn run(
    paths: &Paths,
    req: &HorizonRequest,
    cancel: &AtomicBool,
    tx: &Sender<ServiceEvent>,
) -> Result<Option<as_propagation::PropagationHorizon>, String> {
    let s = &req.settings;
    let model = s.elevation_enabled.effective_model();
    if model == ElevationModel::None {
        return Err("no elevation model enabled".into());
    }
    let ground = ground_elevation_raw(&paths.store, model, req.pos)?;
    let p = horizon_params(req, model, ground);
    let obstruction = if req.with_obstruction {
        // the obstruction belongs to My station (`MyLat`, `MyLon`), as `OptionsDlg` loads it
        paths
            .store
            .local_obstruction_find(model, s.my_lat, s.my_lon)
            .map_err(|e| format!("local obstruction: {e}"))?
    } else {
        None
    };
    let id = req.id;
    paths
        .engine
        .find_or_create_horizon(
            model,
            &p,
            obstruction.as_ref(),
            true,
            Some(cancel),
            &|az, point| {
                let _ = tx.send(ServiceEvent::HorizonPoint {
                    id,
                    az,
                    point: *point,
                });
            },
        )
        .map_err(|e| format!("horizon calculation: {e}"))
}
