//! The plane feed worker wrapper.

use crate::services::{AppError, Paths, ServiceEvent};
use as_aircraft::db::AircraftDb;
use as_aircraft::positions::PositionDb;
use as_aircraft::PlaneInfoCache;
use as_feeds::pipeline::PipelineConfig;
use as_feeds::registry::feed_by_name;
use as_feeds::worker::{FeedHandle, FeedWorker, WorkerConfig};
use as_feeds::FeedArgs;
use as_settings::Settings;
use std::net::SocketAddr;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;

/// The plane feed worker plus a thread forwarding its events.
pub struct FeedService {
    handle: FeedHandle,
    _forwarder: Option<JoinHandle<()>>,
}

impl FeedService {
    /// Start the feed named in `settings.feed`. `FeedArgs.my_lat/my_lon` are the centre of
    /// `settings.area`, never the station position: they are sent to some feeds' servers.
    pub fn start(
        paths: &Paths,
        settings: &Settings,
        cache: Arc<PlaneInfoCache>,
        tx: Sender<ServiceEvent>,
        feed_id: u64,
    ) -> Result<Self, AppError> {
        let listener =
            crate::web_service::web_config(settings, Default::default()).map(|web| web.bind);
        Self::start_with_listener(paths, settings, cache, tx, feed_id, listener)
    }

    pub(crate) fn start_with_listener(
        paths: &Paths,
        settings: &Settings,
        cache: Arc<PlaneInfoCache>,
        tx: Sender<ServiceEvent>,
        feed_id: u64,
        airscout_listener: Option<SocketAddr>,
    ) -> Result<Self, AppError> {
        let mut feed = feed_by_name(&settings.feed)
            .ok_or_else(|| AppError::Other(format!("unknown feed: {}", settings.feed)))?;
        if let Some(t) = settings
            .feed_settings
            .get(feed.name())
            .and_then(|v| v.as_table())
        {
            feed.load_settings(t)
                .map_err(|e| AppError::Other(format!("feed settings: {e}")))?;
        }
        let a = &settings.area;
        let dir = &paths.data_dir;
        let args = FeedArgs {
            app_dir: dir.clone(),
            data_dir: dir.clone(),
            tmp_dir: dir.join("tmp"),
            log_dir: dir.clone(),
            database_dir: dir.clone(),
            min_lat: a.min_lat,
            max_lat: a.max_lat,
            min_lon: a.min_lon,
            max_lon: a.max_lon,
            min_alt_m: settings.planes_min_alt_m,
            max_alt_m: settings.planes_max_alt_m,
            my_lat: (a.min_lat + a.max_lat) / 2.0,
            my_lon: (a.min_lon + a.max_lon) / 2.0,
            keep_history: settings.planes_keep_history,
            airscout_listener,
            ..FeedArgs::default()
        };
        if feed.name() == "AirScoutServer" {
            feed.start(&args)
                .map_err(|error| AppError::Other(error.to_string()))?;
        }
        let cfg = WorkerConfig {
            // 0 would make the worker loop without pause
            interval_s: settings.planes_interval_s.max(1),
            pipeline: PipelineConfig {
                extended_plausibility: settings.planes_extended_plausibility,
                max_error_dist: settings.planes_max_error_dist,
                log_errors: false,
            },
        };
        let db = Arc::new(AircraftDb::open(dir)?);
        let positions = if settings.planes_keep_history {
            Some(Arc::new(PositionDb::open(dir)?))
        } else {
            None
        };
        let (ftx, frx) = std::sync::mpsc::channel();
        let handle = FeedWorker::spawn(feed, args, cfg, cache, db, positions, ftx);
        let forwarder = std::thread::Builder::new()
            .name("feed-forward".into())
            .spawn(move || {
                // ends when the worker has finished and dropped its sender
                for e in frx {
                    if tx.send(ServiceEvent::Feed(feed_id, e)).is_err() {
                        break;
                    }
                }
            })
            .ok();
        Ok(Self {
            handle,
            _forwarder: forwarder,
        })
    }

    /// Ask the worker to stop. Returns at once; `Feed(Stopped)` follows within about a second
    /// (after a request in flight).
    pub fn stop(self) {
        self.handle.stop();
    }
}
