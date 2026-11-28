//! ASSETPATH answer of the Win-Test server (`MapDlg.cs:7189-7310`): parse the request, build the
//! path, find the nearest planes and reply with ASNEAREST.

use crate::best_case::ui_position;
use crate::services::{ground_elevation, local_obstruction, Paths};
use as_aircraft::crossing::{get_nearest_planes, minutes_to_crossing};
use as_aircraft::{PlaneInfo, PlaneInfoCache};
use as_geo::band::Band;
use as_geo::latlon::{GPoint, EARTH_RADIUS_KM};
use as_geo::maidenhead;
use as_propagation::{PathParams, PropagationPath};
use as_settings::{BandSettings, ElevationEnabled, Settings};
use as_stations::{callsign, default_antenna_height, GeoSource};
use as_wintest::encode;
use as_wintest::track::as_nearest;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::AtomicBool;

/// The settings the answer reads, copied so the receiver thread needs no lock.
#[derive(Clone, Debug, PartialEq)]
pub struct NearestSettings {
    pub band: Band,
    pub elevation_enabled: ElevationEnabled,
    pub band_settings: BTreeMap<Band, BandSettings>,
    pub best_case: bool,
    pub ttl_min: i64,
    pub max_circumcircle_km: f64,
    pub max_alt_m: f64,
    pub max_delta_m: f64,
}

impl NearestSettings {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            band: s.band,
            elevation_enabled: s.elevation_enabled,
            band_settings: s.band_settings.clone(),
            best_case: s.path_best_case_elevation,
            ttl_min: s.planes_position_ttl_min,
            max_circumcircle_km: s.gui.planes_filter_max_circumcircle_km,
            max_alt_m: f64::from(s.planes_max_alt_m),
            max_delta_m: s.planes_max_delta_m,
        }
    }
}

/// A valid ASSETPATH request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetPathRequest {
    /// Name of the sender; the reply is addressed to it.
    pub src: String,
    pub band: Band,
    pub my_call: String,
    pub my_loc: String,
    pub dx_call: String,
    pub dx_loc: String,
}

/// `MapDlg.cs:7193-7220`. `None` drops the request silently: no reply, no DB writes.
pub fn parse_set_path(src: &str, data: &str) -> Option<SetPathRequest> {
    let a: Vec<String> = data.split(',').map(|f| f.replace('"', "")).collect();
    if a.len() < 5 {
        return None;
    }
    // `int.TryParse`: 0 on failure
    let qrg = a[0].trim().parse::<i32>().unwrap_or(0);
    let band = Band::from_i32(qrg / 10000)?;
    if !Band::all_except_none_and_all().contains(&band) {
        return None;
    }
    let loc6 = |l: &str| -> Option<String> {
        let c: Vec<char> = l.chars().collect();
        (c.len() >= 6).then(|| c[..6].iter().collect())
    };
    let my_loc = loc6(&a[2])?;
    let dx_loc = loc6(&a[4])?;
    if !callsign::check(&a[1])
        || !callsign::check(&a[3])
        || !maidenhead::check(&my_loc)
        || !maidenhead::check(&dx_loc)
    {
        return None;
    }
    Some(SetPathRequest {
        src: src.to_string(),
        band,
        my_call: a[1].clone(),
        my_loc,
        dx_call: a[3].clone(),
        dx_loc,
    })
}

/// `MapDlg.LocationFindOrCreate`: the station position, moved to the best case if enabled.
/// Read-only (#265): an un-stored station resolves to its in-memory locator centre and is not
/// written to the `Location` table.
fn station(
    paths: &Paths,
    n: &NearestSettings,
    call: &str,
    loc: &str,
    req_band: Band,
) -> Result<(GPoint, GPoint, f64), String> {
    let l = paths
        .stations
        .location_find_or_locator(call, loc)
        .map_err(|e| format!("station database: {e}"))?
        .ok_or_else(|| format!("no station {call} {loc}"))?;
    // a stored station position (not a `FromLoc` centre) is trusted and may derive the
    // six-character QRV key from a coarse locator (#13); capture it before the best-case move.
    let raw = GPoint::new(l.lat, l.lon);
    let trusted = (l.source != GeoSource::FromLoc).then_some(raw);
    let mut pos = raw;
    if n.best_case {
        let store = &paths.store;
        pos = ui_position(&n.elevation_enabled, pos, |m, l| {
            store.max_elv_loc(m, l).ok().flatten()
        })?;
    }
    // the QRV row of the requested band, the default height of the requested band (#200)
    let q = paths
        .stations
        .qrv_find_or_create_default(call, loc, req_band, trusted)
        .map_err(|e| format!("station database: {e}"))?;
    let h = if q.antenna_height == 0.0 {
        default_antenna_height(req_band)
    } else {
        q.antenna_height
    };
    Ok((raw, pos, h))
}

/// `MapDlg.cs:7222-7262`. The path of the request, computed with the requested band's
/// propagation settings and the settings' elevation model (#200: the request's band resolves the
/// whole path, not just the QRV rows).
pub fn set_path_path(
    paths: &Paths,
    n: &NearestSettings,
    r: &SetPathRequest,
    cancel: &AtomicBool,
) -> Result<Option<PropagationPath>, String> {
    let model = n.elevation_enabled.effective_model();
    if model == as_elevation::ElevationModel::None {
        return Err("no elevation model enabled".into());
    }
    let bs = n
        .band_settings
        .get(&r.band)
        .ok_or_else(|| format!("no band settings for {}", r.band.to_ghz()))?;
    let (my_raw, my, h_my) = station(paths, n, &r.my_call, &r.my_loc, r.band)?;
    let (dx_raw, dx, h_dx) = station(paths, n, &r.dx_call, &r.dx_loc, r.band)?;
    // the obstruction is stored at the settings position, not at the shifted one (#105)
    let obstruction = local_obstruction(&paths.store, model, my_raw, dx_raw, h_my)?;
    let en = &n.elevation_enabled;
    let p = PathParams {
        lat1: my.lat,
        lon1: my.lon,
        h1: ground_elevation(&paths.store, en, my.lat, my.lon) + h_my,
        lat2: dx.lat,
        lon2: dx.lon,
        h2: ground_elevation(&paths.store, en, dx.lat, dx.lon) + h_dx,
        qrg_ghz: r.band.to_ghz(),
        radius_km: EARTH_RADIUS_KM * bs.k_factor,
        f1_clearance: bs.f1_clearance,
        step_width_m: model.default_step_width_m(),
        local_obstruction: obstruction,
    };
    paths
        .engine
        .find_or_create_path(model, &p, true, Some(cancel))
        .map_err(|e| format!("path calculation: {e}"))
}

/// `MapDlg.cs:7266-7293`: the data field of the ASNEAREST message.
pub fn nearest_data(now: DateTime<Utc>, r: &SetPathRequest, nearest: &[PlaneInfo]) -> String {
    let planes: Vec<&PlaneInfo> = nearest
        .iter()
        .filter(|p| p.potential > 0 && p.forward_qrb.is_some() && minutes_to_crossing(p).is_some())
        .collect();
    let mut s = format!(
        "{},{},{},{},{},{},",
        now.format("%Y-%m-%d %H:%M:%SZ"),
        r.my_call,
        r.my_loc,
        r.dx_call,
        r.dx_loc,
        planes.len()
    );
    for p in planes {
        let dist = p.forward_qrb.unwrap_or_default();
        let mins = minutes_to_crossing(p).unwrap_or(0);
        s.push_str(&format!(
            "{},{},{},{},{},",
            p.call,
            p.category.short(),
            dist as i32,
            p.potential,
            mins
        ));
    }
    s.trim_end_matches(',').to_string()
}

/// Answer of an ASSETPATH: the ASNEAREST data. `Ok(None)` if cancelled, or if the settings lack
/// the band tables the answer needs (a request from the network never panics).
pub fn answer(
    paths: &Paths,
    cache: &PlaneInfoCache,
    n: &NearestSettings,
    r: &SetPathRequest,
    now: DateTime<Utc>,
    cancel: &AtomicBool,
) -> Result<Option<String>, String> {
    // before any DB write: a requested band the settings do not define drops the request
    // silently (#200). Only the requested band is looked up; the GUI band is irrelevant here.
    let Some(req_bs) = n.band_settings.get(&r.band) else {
        return Ok(None);
    };
    let Some(path) = set_path_path(paths, n, r, cancel)? else {
        return Ok(None);
    };
    let planes = cache.get_all(now, n.ttl_min);
    let nearest = get_nearest_planes(
        now,
        &path,
        &planes,
        n.max_circumcircle_km,
        req_bs.max_distance,
        n.max_alt_m,
        n.max_delta_m,
    );
    Ok(Some(nearest_data(now, r, &nearest)))
}

/// `MapDlg.cs:7294-7301`: one ASNEAREST datagram from a new socket (ephemeral port, broadcast
/// allowed), which is closed again.
pub fn send_reply(
    server_name: &str,
    r: &SetPathRequest,
    data: String,
    bind_ip: Ipv4Addr,
    to: SocketAddr,
) -> io::Result<()> {
    let bytes = encode(&as_nearest(server_name, &r.src, data))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "message not encodable"))?;
    let sock = UdpSocket::bind((bind_ip, 0))?;
    sock.set_broadcast(true)?;
    sock.send_to(&bytes, to)?;
    Ok(())
}
