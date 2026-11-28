//! The plane feed worker wrapper: one [`FeedWorker`] per distinct non-empty feed slot, all
//! writing into the same [`PlaneInfoCache`] (the planeserver approach, `service.rs`).

use crate::services::{AppError, Paths, ServiceEvent};
use as_aircraft::db::AircraftDb;
use as_aircraft::positions::PositionDb;
use as_aircraft::PlaneInfoCache;
use as_feeds::pipeline::PipelineConfig;
use as_feeds::registry::feed_by_name;
use as_feeds::worker::{FeedEvent, FeedHandle, FeedWorker, WorkerConfig};
use as_feeds::{FeedArgs, PlaneFeed};
use as_settings::Settings;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;

/// One running feed: its worker and the thread forwarding its events, tagged with the feed
/// name so the app can show each feed's state.
struct Running {
    handle: FeedHandle,
    _forwarder: Option<JoinHandle<()>>,
}

/// The plane feed workers (one per distinct feed slot) plus their forwarding threads.
pub struct FeedService {
    feeds: Vec<Running>,
}

/// Build one feed of the settings, or the status text of why it does not start.
fn make_feed(name: &str, settings: &Settings) -> Result<Box<dyn PlaneFeed>, String> {
    let mut feed = feed_by_name(name).ok_or_else(|| format!("unknown feed ({name})"))?;
    if feed.needs_disclaimer() && !settings.feeds_disclaimer_accepted.iter().any(|a| a == name) {
        return Err(format!("disclaimer not accepted ({name})"));
    }
    if let Some(v) = settings.feed_settings.get(name) {
        let t = v
            .as_table()
            .ok_or_else(|| format!("feed settings of {name} are not a table"))?;
        // `FeedError` texts name the key, never a value.
        feed.load_settings(t).map_err(|e| e.to_string())?;
    }
    if name == "AirScoutServer"
        && settings
            .feed_settings
            .get(name)
            .and_then(|settings| settings.get("url"))
            .and_then(toml::Value::as_str)
            .is_none_or(str::is_empty)
    {
        return Err("AirScoutServer feed URL is unconfigured".into());
    }
    Ok(feed)
}

fn worker_config(settings: &Settings, tmp_dir: &Path) -> WorkerConfig {
    WorkerConfig {
        // 0 would make the worker loop without pause.
        interval_s: settings.planes_interval_s.max(1),
        pipeline: PipelineConfig {
            extended_plausibility: settings.planes_extended_plausibility,
            max_error_dist: settings.planes_max_error_dist,
            log_errors: settings.planes_log_errors,
        },
        trace_positions: settings
            .planes_trace_positions
            .then(|| tmp_dir.join("PlanePositions")),
    }
}

impl FeedService {
    /// Start every distinct feed named in `settings` (`feed`, `feed2`, `feed3`).
    /// `FeedArgs.my_lat/my_lon` are the centre of `settings.area`, never the station position:
    /// they are sent to some feeds' servers.
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
        let cfg = worker_config(settings, &args.tmp_dir);
        // One aircraft and position database shared by every worker, as the single feed does.
        let db = Arc::new(AircraftDb::open(dir)?);
        let positions = if settings.planes_keep_history {
            Some(Arc::new(PositionDb::open(dir)?))
        } else {
            None
        };

        let mut feeds = Vec::new();
        for name in settings.distinct_feeds() {
            let mut feed = match make_feed(name, settings) {
                Ok(feed) => feed,
                Err(text) => {
                    let _ = tx.send(ServiceEvent::Feed {
                        id: feed_id,
                        name: name.to_string(),
                        event: FeedEvent::Error(text),
                    });
                    continue;
                }
            };
            // AirScoutServer validates its URL up front (also the own-listener guard).
            if feed.name() == "AirScoutServer" {
                if let Err(error) = feed.start(&args) {
                    let _ = tx.send(ServiceEvent::Feed {
                        id: feed_id,
                        name: name.to_string(),
                        event: FeedEvent::Error(error.to_string()),
                    });
                    continue;
                }
            }
            let (ftx, frx) = std::sync::mpsc::channel();
            let handle = FeedWorker::spawn(
                feed,
                args.clone(),
                cfg.clone(),
                cache.clone(),
                db.clone(),
                positions.clone(),
                ftx,
            );
            let name = name.to_string();
            let tx = tx.clone();
            let forwarder = std::thread::Builder::new()
                .name("feed-forward".into())
                .spawn(move || {
                    // ends when the worker has finished and dropped its sender
                    for e in frx {
                        if tx
                            .send(ServiceEvent::Feed {
                                id: feed_id,
                                name: name.clone(),
                                event: e,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .ok();
            feeds.push(Running {
                handle,
                _forwarder: forwarder,
            });
        }
        Ok(Self { feeds })
    }

    /// Ask every worker to stop. Returns at once; `Feed(Stopped)` follows within about a second
    /// (after a request in flight).
    pub fn stop(self) {
        for f in self.feeds {
            f.handle.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_errors_reaches_worker_config() {
        let tmp = Path::new("tmp");
        for (log_errors, trace_positions) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let settings = Settings {
                planes_log_errors: log_errors,
                planes_trace_positions: trace_positions,
                ..Settings::default()
            };
            let cfg = worker_config(&settings, tmp);
            assert_eq!(cfg.pipeline.log_errors, log_errors);
            assert_eq!(
                cfg.trace_positions,
                trace_positions.then(|| tmp.join("PlanePositions"))
            );
        }
    }
}
