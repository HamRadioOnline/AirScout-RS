//! `airscout-cli`: smoke tool for the core: propagation path and horizon (stage 1), aircraft
//! database update, plane feeds and crossing prediction (stage 2).
#![forbid(unsafe_code)]

use as_aircraft::crossing::{get_nearest_planes, minutes_to_crossing};
use as_aircraft::db::AircraftDb;
use as_aircraft::updater::{update_aircraft_db, DEFAULT_UPDATE_URL};
use as_aircraft::{PlaneCategory, PlaneInfo, PlaneInfoCache, PlaneJson};
use as_elevation::{prepare_area, ElevationModel, ElevationStore};
use as_feeds::pipeline::PipelineConfig;
use as_feeds::registry::{all_feeds, feed_by_name};
use as_feeds::worker::{FeedEvent, FeedWorker, WorkerConfig};
use as_feeds::FeedArgs;
use as_geo::band::Band;
use as_geo::latlon::{self, GPoint, GRect};
use as_geo::maidenhead::{self, PositionInRectangle};
use as_propagation::{HorizonParams, PathParams, PropagationEngine, PropagationPath};
use as_settings::Settings;
use clap::{Args, Parser, Subcommand};
use serde_json::json;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::Arc;

mod elevation_coverage;
mod horizon_gen;
mod wintest_sim;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Parser)]
#[command(
    name = "airscout-cli",
    version,
    about = "AirScout-RS core: propagation path, horizon, aircraft data and plane feeds"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Propagation path between two stations (JSON on stdout).
    Path {
        /// Station 1: locator (e.g. JO50AA) or "lat,lon".
        #[arg(long)]
        from: String,
        /// Station 2: locator or "lat,lon".
        #[arg(long)]
        to: String,
        /// Antenna height above ground at station 1 [m].
        #[arg(long, default_value_t = 10.0)]
        h1: f64,
        /// Antenna height above ground at station 2 [m].
        #[arg(long, default_value_t = 10.0)]
        h2: f64,
        #[command(flatten)]
        common: Common,
    },
    /// Radio horizon around a station, 360 azimuths (JSON on stdout).
    Horizon {
        /// Station: locator or "lat,lon".
        #[arg(long)]
        loc: String,
        /// Antenna height above ground [m].
        #[arg(long, default_value_t = 10.0)]
        h: f64,
        /// Horizon radius [km].
        #[arg(long, default_value_t = 100.0)]
        dist: f64,
        #[command(flatten)]
        common: Common,
    },
    /// Download the aircraft database files and merge them (counts as JSON).
    AircraftUpdate {
        /// Data directory (aircrafts.db3).
        #[arg(long)]
        data: PathBuf,
        /// Update server (ending in '/').
        #[arg(long, default_value = DEFAULT_UPDATE_URL)]
        base_url: String,
    },
    /// Run a plane feed for some cycles and print the processed planes (newest per hex).
    Planes {
        /// Feed name (see `feeds`).
        #[arg(long)]
        feed: String,
        /// Covered area: minlat,minlon,maxlat,maxlon.
        #[arg(long)]
        area: String,
        /// Data directory (aircrafts.db3).
        #[arg(long)]
        data: PathBuf,
        /// Number of feed requests.
        #[arg(long, default_value_t = 1)]
        cycles: u32,
        /// Feed setting `key=value` (TOML value; plain text is taken as a string).
        #[arg(long = "feed-setting")]
        feed_setting: Vec<String>,
    },
    /// Crossing prediction for a list of planes (PlaneJSON) on a propagation path.
    Nearest {
        /// Station 1: locator or "lat,lon".
        #[arg(long)]
        from: String,
        /// Station 2: locator or "lat,lon".
        #[arg(long)]
        to: String,
        /// JSON file with a list of PlaneJSON objects (as AirScout serves planes.json).
        #[arg(long)]
        planes: PathBuf,
        /// Antenna height above ground at station 1 [m].
        #[arg(long, default_value_t = 10.0)]
        h1: f64,
        /// Antenna height above ground at station 2 [m].
        #[arg(long, default_value_t = 10.0)]
        h2: f64,
        /// `Planes_Filter_Max_Circumcircle` [km]: 0 = half the path length, < 0 = no limit.
        #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
        max_radius: f64,
        #[command(flatten)]
        common: Common,
    },
    /// List the plane feeds.
    Feeds,
    /// Send one Win-Test UDP message, as the C# WinTestSimulator does (JSON on stdout).
    WintestSim(wintest_sim::WintestSimArgs),
    /// Write one horizon CSV per station of stations.db3, as the C# HorizonGenerator does.
    HorizonGen(horizon_gen::HorizonGenArgs),
    /// Tiles of an elevation catalogue in an area, against the local store (JSON, GeoJSON).
    ElevationCoverage(elevation_coverage::ElevationCoverageArgs),
}

#[derive(Args)]
struct Common {
    /// Band in MHz (144, 432, 1296, ...) or name (144M, 1.2G, ...).
    #[arg(long, default_value = "144")]
    band: String,
    /// Elevation model: globe, srtm3, srtm1, aster3, aster1.
    #[arg(long, default_value = "globe")]
    model: String,
    /// Data directory (elevation and propagation databases).
    #[arg(long)]
    data: PathBuf,
    /// Download missing 4-character squares from the update server first.
    #[arg(long)]
    download: bool,
    /// Elevation update server.
    #[arg(long, default_value = as_elevation::DEFAULT_BASE_URL)]
    base_url: String,
    /// Near-field suppression [m].
    #[arg(long, default_value_t = 100.0)]
    near_field_m: f64,
}

fn parse_position(s: &str) -> Res<GPoint> {
    if let Some((a, b)) = s.split_once(',') {
        let (lat, lon) = (a.trim().parse::<f64>()?, b.trim().parse::<f64>()?);
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(format!("position out of range: {s}").into());
        }
        return Ok(GPoint::new(lat, lon));
    }
    Ok(maidenhead::gpoint_from_loc(
        s,
        PositionInRectangle::MiddleMiddle,
    )?)
}

fn parse_band(s: &str) -> Res<Band> {
    let b = s
        .trim()
        .parse::<i32>()
        .ok()
        .and_then(Band::from_i32)
        .unwrap_or_else(|| Band::parse(s));
    if matches!(b, Band::None | Band::All) {
        return Err(format!("unknown band: {s}").into());
    }
    Ok(b)
}

fn parse_model(s: &str) -> Res<ElevationModel> {
    ElevationModel::ALL
        .into_iter()
        .find(|m| m.dir_name().eq_ignore_ascii_case(s))
        .ok_or_else(|| format!("unknown elevation model: {s}").into())
}

/// Bounding box of `pts`, padded by 0.01°.
fn bbox(pts: impl IntoIterator<Item = GPoint>) -> Res<GRect> {
    let (mut a, mut b, mut c, mut d) = (90.0f64, 180.0f64, -90.0f64, -180.0f64);
    for p in pts {
        a = a.min(p.lat);
        b = b.min(p.lon);
        c = c.max(p.lat);
        d = d.max(p.lon);
    }
    // DestinationPoint does not wrap longitude (logic-findings #15): points beyond ±180° cannot be
    // looked up, and a box spanning more than half the globe means the area wraps around.
    if b < -180.0 || d > 180.0 || d - b > 180.0 {
        return Err(
            "area crosses the antimeridian (±180°), not supported yet (logic-findings #15)".into(),
        );
    }
    Ok(GRect::new(
        (a - 0.01).max(-90.0),
        (b - 0.01).max(-180.0),
        (c + 0.01).min(90.0),
        (d + 0.01).min(180.0),
    ))
}

/// File listing the 4-character squares the server has no data for (HTTP 404: open sea).
fn sea_file(c: &Common, model: ElevationModel) -> PathBuf {
    c.data.join(format!(
        "{}-sea-squares.txt",
        model.dir_name().to_lowercase()
    ))
}

/// Make sure the elevation squares covering `rect` are in the store (see
/// `as_elevation::prepare_area`); prints progress and a hint if the data is incomplete.
fn prepare(store: &ElevationStore, model: ElevationModel, rect: GRect, c: &Common) -> Res<()> {
    let client = reqwest::blocking::Client::new();
    let status = prepare_area(
        store,
        &client,
        &c.base_url,
        model,
        rect,
        &sea_file(c, model),
        c.download,
        &mut |m| eprintln!("{m}"),
    )?;
    if !status.errors.is_empty() {
        return Err(format!("elevation download failed: {}", status.errors.join("; ")).into());
    }
    if !status.complete {
        eprintln!("elevation data incomplete for this area (use --download)");
    }
    Ok(())
}

fn open(
    c: &Common,
) -> Res<(
    Arc<ElevationStore>,
    PropagationEngine,
    ElevationModel,
    Band,
    Settings,
)> {
    let model = parse_model(&c.model)?;
    let band = parse_band(&c.band)?;
    let store = Arc::new(ElevationStore::open(&c.data)?);
    let engine = PropagationEngine::open(store.clone(), &c.data)?;
    engine.set_near_field_suppression_m(c.near_field_m);
    let settings = Settings {
        band,
        elevation_model: model,
        ..Settings::default()
    };
    Ok((store, engine, model, band, settings))
}

/// Prepare the elevation data and compute the propagation path between two stations.
fn build_path(
    from: &str,
    to: &str,
    h1: f64,
    h2: f64,
    c: &Common,
) -> Res<(PropagationPath, PathParams, Settings)> {
    let (a, b) = (parse_position(from)?, parse_position(to)?);
    let (store, engine, model, band, settings) = open(c)?;
    let d = latlon::distance(a.lat, a.lon, b.lat, b.lon);
    let brg = latlon::bearing(a.lat, a.lon, b.lat, b.lon);
    let samples = (0..=d as usize).map(|k| latlon::destination_point(a.lat, a.lon, brg, k as f64));
    prepare(&store, model, bbox(samples.chain([a, b]))?, c)?;
    let bs = settings.current_band_settings();
    let p = PathParams {
        lat1: a.lat,
        lon1: a.lon,
        h1: store.elevation(model, a.lat, a.lon, true)? as f64 + h1,
        lat2: b.lat,
        lon2: b.lon,
        h2: store.elevation(model, b.lat, b.lon, true)? as f64 + h2,
        qrg_ghz: band.to_ghz(),
        radius_km: settings.radius_km(),
        f1_clearance: bs.f1_clearance,
        step_width_m: model.default_step_width_m(),
        local_obstruction: f64::MIN,
    };
    let pp = engine
        .create_path(model, &p, false, None)?
        .ok_or("cancelled")?;
    Ok((pp, p, settings))
}

fn path_cmd(from: &str, to: &str, h1: f64, h2: f64, c: &Common) -> Res<serde_json::Value> {
    let (pp, p, _) = build_path(from, to, h1, h2, c)?;
    let info: Vec<_> = pp
        .info_points()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, x)| json!({"dist_km": i, "lat": x.lat, "lon": x.lon, "h1": x.h1, "h2": x.h2, "f1": x.f1}))
        .collect();
    Ok(json!({
        "distance_km": pp.distance(),
        "bearing12": pp.bearing12(),
        "bearing21": pp.bearing21(),
        "h1": p.h1,
        "h2": p.h2,
        "eps1_min_deg": pp.eps1_min.to_degrees(),
        "eps2_min_deg": pp.eps2_min.to_degrees(),
        "valid": pp.valid,
        "info_points": info,
    }))
}

fn horizon_cmd(loc: &str, h: f64, dist: f64, c: &Common) -> Res<serde_json::Value> {
    let a = parse_position(loc)?;
    let (store, engine, model, band, settings) = open(c)?;
    let ring = (0..360).map(|az| latlon::destination_point(a.lat, a.lon, az as f64, dist));
    prepare(&store, model, bbox(ring.chain([a]))?, c)?;
    let p = HorizonParams {
        lat: a.lat,
        lon: a.lon,
        h: store.elevation(model, a.lat, a.lon, true)? as f64 + h,
        dist_km: dist,
        qrg_ghz: band.to_ghz(),
        radius_km: settings.radius_km(),
        f1_clearance: settings.current_band_settings().f1_clearance,
        step_width_m: model.default_step_width_m(),
    };
    let hor = engine
        .create_horizon(model, &p, None, false, None, &|_, _| {})?
        .ok_or("cancelled")?;
    let points: Vec<_> = hor
        .horizon
        .iter()
        .enumerate()
        .map(|(az, x)| json!({"azimuth": az, "dist_km": x.dist, "eps_min_deg": x.eps_min.to_degrees(), "elv": x.elv}))
        .collect();
    Ok(json!({"lat": a.lat, "lon": a.lon, "h": p.h, "valid": hor.valid, "points": points}))
}

// ------------------------------------------------------------------ stage 2

fn aircraft_update_cmd(data: &std::path::Path, base_url: &str) -> Res<serde_json::Value> {
    let db = AircraftDb::open(data)?;
    let client = reqwest::blocking::Client::builder()
        .user_agent(as_net::user_agent())
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    let s = update_aircraft_db(&client, base_url, &data.join("tmp"), &db, None, &mut |m| {
        eprintln!("{m}")
    })?;
    Ok(json!({
        "aircrafts": s.aircrafts,
        "aircraft_types": s.aircraft_types,
        "airlines": s.airlines,
        "airports": s.airports,
        "registrations": s.registrations,
    }))
}

fn feeds_cmd() -> serde_json::Value {
    let feeds: Vec<_> = all_feeds()
        .iter()
        .map(|f| {
            json!({
                "name": f.name(),
                "info": f.info(),
                "enabled_by_default": f.enabled_by_default(),
                "disclaimer": f.disclaimer(),
            })
        })
        .collect();
    json!(feeds)
}

/// `minlat,minlon,maxlat,maxlon`.
fn parse_area(s: &str) -> Res<[f64; 4]> {
    let v = s
        .split(',')
        .map(|x| x.trim().parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("bad area {s}: {e}"))?;
    <[f64; 4]>::try_from(v)
        .map_err(|_| format!("area needs minlat,minlon,maxlat,maxlon: {s}").into())
}

/// `key=value`: the value as TOML if it parses (numbers, booleans, quoted strings), else as a
/// plain string.
fn parse_feed_setting(s: &str) -> Res<(String, toml::Value)> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("feed setting needs key=value: {s}"))?;
    let k = k.trim().to_string();
    let value = format!("v = {v}")
        .parse::<toml::Table>()
        .ok()
        .and_then(|mut t| t.remove("v"))
        .unwrap_or_else(|| toml::Value::String(v.to_string()));
    Ok((k, value))
}

fn plane_json(p: &PlaneInfo) -> serde_json::Value {
    json!({
        "time": p.time.timestamp(),
        "hex": p.hex,
        "call": p.call,
        "reg": p.reg,
        "lat": p.lat,
        "lon": p.lon,
        "alt_ft": p.alt_ft,
        "track": p.track,
        "speed_kts": p.speed_kts,
        "type": p.type_code,
        "manufacturer": p.manufacturer,
        "model": p.model,
        "category": p.category.as_str(),
    })
}

fn planes_cmd(
    feed: &str,
    area: &str,
    data: &std::path::Path,
    cycles: u32,
    feed_settings: &[String],
) -> Res<serde_json::Value> {
    let [min_lat, min_lon, max_lat, max_lon] = parse_area(area)?;
    let mut f = feed_by_name(feed).ok_or_else(|| format!("unknown feed: {feed}"))?;
    let mut t = toml::Table::new();
    for s in feed_settings {
        let (k, v) = parse_feed_setting(s)?;
        t.insert(k, v);
    }
    f.load_settings(&t)?;
    let settings = Settings::default();
    let args = FeedArgs {
        app_dir: data.to_path_buf(),
        data_dir: data.to_path_buf(),
        tmp_dir: data.join("tmp"),
        log_dir: data.to_path_buf(),
        database_dir: data.to_path_buf(),
        min_lat,
        max_lat,
        min_lon,
        max_lon,
        min_alt_m: settings.planes_min_alt_m,
        max_alt_m: settings.planes_max_alt_m,
        // the centre of the area, not the user's location
        my_lat: (min_lat + max_lat) / 2.0,
        my_lon: (min_lon + max_lon) / 2.0,
        ..FeedArgs::default()
    };
    let cfg = WorkerConfig {
        // one second between requests: the worker sees the stop request before the next one
        interval_s: 1,
        pipeline: PipelineConfig {
            extended_plausibility: settings.planes_extended_plausibility,
            max_error_dist: settings.planes_max_error_dist,
            log_errors: false,
        },
    };
    let db = Arc::new(AircraftDb::open(data)?);
    let (tx, rx) = channel();
    let h = FeedWorker::spawn(f, args, cfg, Arc::new(PlaneInfoCache::new()), db, None, tx);
    // a cycle reports only planes with a newer position, so collect all cycles (newest per hex)
    let mut seen: std::collections::BTreeMap<String, PlaneInfo> = Default::default();
    let (mut done, mut any, mut error) = (0, false, None);
    while done < cycles {
        match rx.recv() {
            Ok(FeedEvent::Planes(p)) => {
                done += 1;
                any = true;
                for x in p {
                    seen.insert(x.hex.clone(), x);
                }
            }
            Ok(FeedEvent::Error(e)) => {
                eprintln!("{e}");
                // a failed request is a cycle too; database errors follow a cycle's planes
                if e.starts_with("Plane Feed Exception") {
                    done += 1;
                }
                error = Some(e);
            }
            Ok(FeedEvent::Started) => {}
            Ok(FeedEvent::Stopped) | Err(_) => break,
        }
    }
    h.stop();
    h.join();
    if !any {
        return Err(error.unwrap_or_else(|| "the feed stopped".into()).into());
    }
    Ok(json!(seen.values().map(plane_json).collect::<Vec<_>>()))
}

fn nearest_cmd(
    from: &str,
    to: &str,
    file: &std::path::Path,
    h1: f64,
    h2: f64,
    max_radius: f64,
    c: &Common,
) -> Res<serde_json::Value> {
    let list: Vec<PlaneJson> = serde_json::from_str(&std::fs::read_to_string(file)?)?;
    let now = chrono::Utc::now();
    let mut planes: Vec<PlaneInfo> = list
        .iter()
        .map(|j| {
            let s = |x: &Option<String>| x.clone().unwrap_or_default();
            PlaneInfo::new(
                j.time
                    .and_then(|t| chrono::DateTime::from_timestamp(i64::from(t), 0))
                    .unwrap_or(now),
                &s(&j.call),
                &s(&j.reg),
                &s(&j.hex),
                j.lat.unwrap_or_default(),
                j.lon.unwrap_or_default(),
                j.track.unwrap_or_default(),
                j.alt.unwrap_or_default(),
                j.speed.unwrap_or_default(),
                &s(&j.type_code),
                &s(&j.manufacturer),
                &s(&j.model),
                j.category
                    .and_then(PlaneCategory::from_i32)
                    .unwrap_or_default(),
            )
        })
        .collect();
    let (pp, _, settings) = build_path(from, to, h1, h2, c)?;
    let found = get_nearest_planes(
        now,
        &pp,
        &mut planes,
        max_radius,
        settings.current_band_settings().max_distance,
        f64::from(settings.planes_max_alt_m),
        settings.planes_max_delta_m,
    );
    let out: Vec<_> = found
        .iter()
        .map(|p| {
            json!({
                "hex": p.hex,
                "call": p.call,
                "potential": p.potential,
                "int_qrb": p.int_qrb,
                "minutes": minutes_to_crossing(p),
                "alt_diff": p.alt_diff,
                "eps1_deg": p.eps1.to_degrees(),
                "eps2_deg": p.eps2.to_degrees(),
                "squint": p.squint,
            })
        })
        .collect();
    Ok(json!({"distance_km": pp.distance(), "valid": pp.valid, "planes": out}))
}

fn main() {
    let cli = Cli::parse();
    let r = match &cli.cmd {
        Cmd::Path {
            from,
            to,
            h1,
            h2,
            common,
        } => path_cmd(from, to, *h1, *h2, common),
        Cmd::Horizon {
            loc,
            h,
            dist,
            common,
        } => horizon_cmd(loc, *h, *dist, common),
        Cmd::AircraftUpdate { data, base_url } => aircraft_update_cmd(data, base_url),
        Cmd::Planes {
            feed,
            area,
            data,
            cycles,
            feed_setting,
        } => planes_cmd(feed, area, data, *cycles, feed_setting),
        Cmd::Nearest {
            from,
            to,
            planes,
            h1,
            h2,
            max_radius,
            common,
        } => nearest_cmd(from, to, planes, *h1, *h2, *max_radius, common),
        Cmd::Feeds => Ok(feeds_cmd()),
        Cmd::WintestSim(a) => wintest_sim::run(a),
        Cmd::HorizonGen(a) => horizon_gen::run(a),
        Cmd::ElevationCoverage(a) => elevation_coverage::run(a),
    };
    match r {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default()),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
