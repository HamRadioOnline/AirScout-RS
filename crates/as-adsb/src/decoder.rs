//! Port of `LibADSB/ADSBDecoder.cs`: per-aircraft state built from a stream of messages.
//!
//! Stage-6 fixes:
//! - pair by comparable message times with a strict 10 s window (#42, #359);
//! - expire stale aircraft and pending pair state by TTL (#43);
//! - emit only finite, in-range coordinates (#44);
//! - emit once per accepted position revision, including delayed identity (#45);
//! - track per-field freshness and altitude provenance (#35, #363, #366, #364).

use crate::cpr;
use crate::msg::{self, AirbornePosition, Msg};
use crate::{AdsbError, ModeSReply};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

/// UTC clock used for polling and receipt-time expiry.
pub type Clock = Box<dyn Fn() -> DateTime<Utc> + Send>;

/// The full per-aircraft state (`ADSBInfo`). Integer fields are `None` where C# holds
/// `int.MinValue`; unknown lat/lon are `NaN`.
#[derive(Debug, Clone, PartialEq)]
pub struct AdsbState {
    pub timestamp: DateTime<Utc>,
    pub icao24: String,
    pub call: String,
    pub lat: f64,
    pub lon: f64,
    pub baro_alt_ft: Option<i32>,
    pub geometric_alt_ft: Option<i32>,
    pub geo_minus_baro_ft: Option<i32>,
    pub heading: Option<i32>,
    pub speed_kts: Option<i32>,
    pub last_even: Option<DateTime<Utc>>,
    pub last_odd: Option<DateTime<Utc>>,
    pub last_pos: Option<DateTime<Utc>>,
    pub num_pos: i32,
    last_pos_odd: Option<bool>,
    baro_alt_at: Option<DateTime<Utc>>,
    geometric_alt_at: Option<DateTime<Utc>>,
    geo_minus_baro_at: Option<DateTime<Utc>>,
    heading_at: Option<DateTime<Utc>>,
    speed_at: Option<DateTime<Utc>>,
    position_revision: u64,
    emitted_revision: u64,
    last_seen: DateTime<Utc>,
    last_even_received: Option<DateTime<Utc>>,
    last_odd_received: Option<DateTime<Utc>>,
    last_even_msg: Option<AirbornePosition>,
    last_odd_msg: Option<AirbornePosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AltitudeProvenance {
    Geometric,
    PressureEstimate,
    PressurePlusCorrectionEstimate,
}

/// An aircraft as handed to the feed (`GetPlanes`). Unknown values are `NaN`.
#[derive(Debug, Clone, PartialEq)]
pub struct AdsbInfo {
    pub timestamp: DateTime<Utc>,
    pub icao24: String,
    pub call: String,
    pub lat: f64,
    pub lon: f64,
    pub baro_alt_ft: f64,
    pub geometric_alt_ft: f64,
    pub geo_minus_baro_ft: f64,
    pub heading: f64,
    pub speed_kts: f64,
    pub altitude_ft: f64,
    pub altitude_provenance: AltitudeProvenance,
    /// Inclusive expiry of the weakest required observation.
    pub components_valid_until: DateTime<Utc>,
}

fn nan_or(v: Option<i32>) -> f64 {
    v.map_or(f64::NAN, f64::from)
}

impl From<&AdsbState> for AdsbInfo {
    fn from(s: &AdsbState) -> Self {
        Self {
            timestamp: s.timestamp,
            icao24: s.icao24.clone(),
            call: s.call.clone(),
            lat: s.lat,
            lon: s.lon,
            baro_alt_ft: nan_or(s.baro_alt_ft),
            geometric_alt_ft: nan_or(s.geometric_alt_ft),
            geo_minus_baro_ft: nan_or(s.geo_minus_baro_ft),
            heading: nan_or(s.heading),
            speed_kts: nan_or(s.speed_kts),
            altitude_ft: f64::NAN,
            altitude_provenance: AltitudeProvenance::PressureEstimate,
            components_valid_until: s.timestamp,
        }
    }
}

/// The stateful decoder (`ADSBDecoder`).
pub struct Decoder {
    ttl_min: i64,
    clock: Clock,
    event_timed: Option<bool>,
    receipt_watermark: DateTime<Utc>,
    index: HashMap<String, usize>,
    infos: Vec<AdsbState>,
}

impl Decoder {
    /// Decoder on the system clock.
    pub fn new(ttl_min: i64) -> Self {
        Self::with_clock(ttl_min, Box::new(Utc::now))
    }

    /// Decoder on a given clock (tests).
    pub fn with_clock(ttl_min: i64, clock: Clock) -> Self {
        Self {
            ttl_min,
            clock,
            event_timed: None,
            receipt_watermark: DateTime::<Utc>::MIN_UTC,
            index: HashMap::new(),
            infos: Vec::new(),
        }
    }

    /// The configured TTL [min].
    pub fn ttl_min(&self) -> i64 {
        self.ttl_min
    }

    /// Number of aircraft known (`Count`).
    pub fn len(&self) -> usize {
        self.infos.len()
    }

    /// True when no aircraft is known.
    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }

    /// Every aircraft state, in first-seen order.
    pub fn states(&self) -> Vec<AdsbState> {
        self.infos.clone()
    }

    fn ttl_duration(&self) -> Duration {
        Duration::minutes(self.ttl_min)
    }

    fn info(
        &mut self,
        icao24: u32,
        ts: DateTime<Utc>,
        received_at: DateTime<Utc>,
    ) -> &mut AdsbState {
        let key = format!("{icao24:06X}");
        let idx = match self.index.get(&key) {
            Some(&i) => i,
            None => {
                self.infos.push(AdsbState {
                    timestamp: ts,
                    icao24: key.clone(),
                    call: String::new(),
                    lat: f64::NAN,
                    lon: f64::NAN,
                    baro_alt_ft: None,
                    geometric_alt_ft: None,
                    geo_minus_baro_ft: None,
                    heading: None,
                    speed_kts: None,
                    last_even: None,
                    last_odd: None,
                    last_pos: None,
                    num_pos: 0,
                    last_pos_odd: None,
                    baro_alt_at: None,
                    geometric_alt_at: None,
                    geo_minus_baro_at: None,
                    heading_at: None,
                    speed_at: None,
                    position_revision: 0,
                    emitted_revision: 0,
                    last_seen: received_at,
                    last_even_received: None,
                    last_odd_received: None,
                    last_even_msg: None,
                    last_odd_msg: None,
                });
                self.index.insert(key, self.infos.len() - 1);
                self.infos.len() - 1
            }
        };
        self.infos[idx].last_seen = self.infos[idx].last_seen.max(received_at);
        &mut self.infos[idx]
    }

    fn prune_expired(&mut self, now: DateTime<Utc>) {
        self.receipt_watermark = self.receipt_watermark.max(now);
        let now = self.receipt_watermark;
        let ttl = self.ttl_duration();
        self.infos.retain(|info| now - info.last_seen <= ttl);
        self.index.clear();
        for (i, info) in self.infos.iter_mut().enumerate() {
            self.index.insert(info.icao24.clone(), i);
            if info.last_even_received.is_some_and(|t| now - t > ttl) {
                info.last_even = None;
                info.last_even_msg = None;
                info.last_even_received = None;
            }
            if info.last_odd_received.is_some_and(|t| now - t > ttl) {
                info.last_odd = None;
                info.last_odd_msg = None;
                info.last_odd_received = None;
            }
        }
    }

    fn try_local_decode(
        info: &AdsbState,
        pos: &AirbornePosition,
        ts: DateTime<Utc>,
    ) -> Option<(f64, f64)> {
        let ref_ts = info.last_pos?;
        let ref_odd = info.last_pos_odd?;
        let age_ns = (ts - ref_ts).num_nanoseconds()?;
        if !(0..=10_000_000_000).contains(&age_ns) {
            return None;
        }
        if !info.lat.is_finite()
            || !info.lon.is_finite()
            || !(-90.0..=90.0).contains(&info.lat)
            || !(-180.0..=180.0).contains(&info.lon)
        {
            return None;
        }
        const CPR_SCALE: f64 = (1u32 << 17) as f64;
        const EARTH_RADIUS_KM: f64 = 6371.0;
        let age_seconds = age_ns as f64 / 1_000_000_000.0;
        let max_km = 6000.0 * 1.852 * age_seconds / 3600.0;
        let travel_delta = max_km / EARTH_RADIUS_KM;
        let ref_cell_lat = 360.0 / if ref_odd { 59.0 } else { 60.0 };
        let pos_cell_lat = 360.0 / if pos.odd { 59.0 } else { 60.0 };
        let nl = cpr::nl(info.lat);
        if nl == 0 {
            return None;
        }
        let ref_cell_lon = 360.0 / (nl - i32::from(ref_odd)).max(1) as f64;
        let pos_cell_lon = 360.0 / (nl - i32::from(pos.odd)).max(1) as f64;
        let ref_lat_quantum = ref_cell_lat / CPR_SCALE;
        let pos_lat_quantum = pos_cell_lat / CPR_SCALE;
        let ref_lon_quantum = ref_cell_lon / CPR_SCALE;
        let pos_lon_quantum = pos_cell_lon / CPR_SCALE;

        let phi = info.lat.to_radians();
        let latitude_half_width = travel_delta.to_degrees() + ref_lat_quantum + pos_lat_quantum;
        if phi.abs() + latitude_half_width.to_radians() >= std::f64::consts::FRAC_PI_2 {
            return None;
        }
        let longitude_argument = travel_delta.sin() / phi.cos();
        if longitude_argument >= 1.0 || longitude_argument <= -1.0 {
            return None;
        }
        let longitude_half_width =
            longitude_argument.asin().to_degrees() + ref_lon_quantum + pos_lon_quantum;
        let lo = info.lat - latitude_half_width;
        let hi = info.lat + latitude_half_width;
        if cpr::nl(lo) != nl
            || cpr::nl(hi) != nl
            || latitude_half_width >= ref_cell_lat / 2.0
            || latitude_half_width >= pos_cell_lat / 2.0
            || longitude_half_width >= ref_cell_lon / 2.0
            || longitude_half_width >= pos_cell_lon / 2.0
        {
            return None;
        }

        let (lat, lon) = cpr::local(pos, info.lat, info.lon).ok()?;
        if !in_domain(lat, lon) {
            return None;
        }
        let lat_quantum_rad = (ref_lat_quantum + pos_lat_quantum).to_radians();
        let lon_quantum_rad = (ref_lon_quantum + pos_lon_quantum).to_radians();
        let tol = EARTH_RADIUS_KM * lat_quantum_rad.hypot(lon_quantum_rad * phi.cos().abs());
        let d = great_circle_km(info.lat, info.lon, lat, lon);
        if d <= max_km + tol {
            Some((lat, lon))
        } else {
            None
        }
    }

    fn accept_position(info: &mut AdsbState, lat: f64, lon: f64, ts: DateTime<Utc>, odd: bool) {
        info.lat = lat;
        info.lon = lon;
        info.timestamp = ts;
        info.last_pos = Some(ts);
        info.last_pos_odd = Some(odd);
        info.num_pos += 1;
        info.position_revision = info.position_revision.saturating_add(1);
    }

    fn apply_altitude(info: &mut AdsbState, pos: &AirbornePosition, ts: DateTime<Utc>) {
        let (alt, at) = if pos.tc >= 20 {
            (&mut info.geometric_alt_ft, &mut info.geometric_alt_at)
        } else {
            (&mut info.baro_alt_ft, &mut info.baro_alt_at)
        };
        update_component(alt, at, pos.alt_ft, ts);
    }

    fn airborne(&mut self, pos: AirbornePosition, ts: DateTime<Utc>, received_at: DateTime<Utc>) {
        let info = self.info(pos.icao24, ts, received_at);
        if !pos.has_position {
            return;
        }
        if pos.odd {
            if info.last_odd.is_some_and(|last| ts < last) {
                return;
            }
            info.last_odd = Some(ts);
            info.last_odd_msg = Some(pos.clone());
            info.last_odd_received = Some(received_at);
        } else {
            if info.last_even.is_some_and(|last| ts < last) {
                return;
            }
            info.last_even = Some(ts);
            info.last_even_msg = Some(pos.clone());
            info.last_even_received = Some(received_at);
        }

        let mut accepted = false;
        if let (Some(even), Some(te), Some(odd), Some(to)) = (
            info.last_even_msg.as_ref(),
            info.last_even,
            info.last_odd_msg.as_ref(),
            info.last_odd,
        ) {
            let sep_ok = (te - to)
                .num_nanoseconds()
                .map(|n| n.unsigned_abs() <= 10_000_000_000)
                .unwrap_or(false);
            if sep_ok {
                let newer_ts = if te >= to { te } else { to };
                if info.last_pos.is_none_or(|last| newer_ts >= last) {
                    if let Ok((lat, lon)) = cpr::global(even, odd, to > te) {
                        if in_domain(lat, lon) {
                            let newer = if te >= to { even.clone() } else { odd.clone() };
                            Self::accept_position(info, lat, lon, newer_ts, newer.odd);
                            Self::apply_altitude(info, &newer, newer_ts);
                            accepted = true;
                        }
                    }
                }
            }
        }
        if !accepted {
            if let Some((lat, lon)) = Self::try_local_decode(info, &pos, ts) {
                Self::accept_position(info, lat, lon, ts, pos.odd);
                Self::apply_altitude(info, &pos, ts);
            }
        }
    }

    fn fresh_time(
        ttl: Duration,
        component_at: Option<DateTime<Utc>>,
        position_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Option<DateTime<Utc>> {
        component_at.filter(|&at| at <= now && now - at <= ttl && (at - position_at).abs() <= ttl)
    }

    fn select_altitude(
        s: &AdsbState,
        ttl: Duration,
        now: DateTime<Utc>,
        geometric_only: bool,
    ) -> Option<(f64, AltitudeProvenance, DateTime<Utc>)> {
        let pos_at = s.last_pos?;
        let fresh = |value: Option<i32>, at| value.zip(Self::fresh_time(ttl, at, pos_at, now));
        let pressure = fresh(s.baro_alt_ft, s.baro_alt_at);
        let geometric = fresh(s.geometric_alt_ft, s.geometric_alt_at);
        if !geometric_only {
            return pressure
                .map(|(ft, at)| (f64::from(ft), AltitudeProvenance::PressureEstimate, at))
                .or_else(|| {
                    geometric.map(|(ft, at)| (f64::from(ft), AltitudeProvenance::Geometric, at))
                });
        }
        if let Some((ft, at)) = geometric {
            return Some((f64::from(ft), AltitudeProvenance::Geometric, at));
        }
        let (baro, baro_at) = pressure?;
        let (correction, correction_at) = fresh(s.geo_minus_baro_ft, s.geo_minus_baro_at)?;
        ((baro_at - correction_at).abs() <= ttl).then_some((
            f64::from(baro + correction),
            AltitudeProvenance::PressurePlusCorrectionEstimate,
            baro_at.min(correction_at),
        ))
    }

    /// `DecodeMessage`: `raw` is `*HEX;` (first and last character are dropped unseen).
    /// Parity failures and unsupported messages are ignored (`Ok`); malformed input is an
    /// error, where C# throws. A decoder belongs to one source. Pass UTC receipt time
    /// and, when available, an event UTC from that source's trusted clock. Switching
    /// between event and receipt timing starts a fresh set of position and field observations.
    pub fn decode_message(
        &mut self,
        raw: &str,
        received_at: DateTime<Utc>,
        event_at: Option<DateTime<Utc>>,
    ) -> Result<(), AdsbError> {
        self.prune_expired(received_at);
        let n = raw.chars().count();
        if n < 2 {
            return Err(AdsbError::BadFormat(format!("message too short: {raw}")));
        }
        let inner: String = raw.chars().skip(1).take(n - 2).collect();
        let m = msg::decode(&inner)?;
        if !ModeSReply::from_hex(&inner)?.check_parity() {
            return Ok(());
        }
        if matches!(m, Msg::SurfacePosition(_) | Msg::Other { .. }) {
            return Ok(());
        }
        let event_timed = event_at.is_some();
        if self
            .event_timed
            .is_some_and(|previous| previous != event_timed)
        {
            // The last fix and the component observations share the previous clock.
            self.infos.clear();
            self.index.clear();
        }
        self.event_timed = Some(event_timed);
        let ts = event_at.unwrap_or(received_at);
        let received_at = self.receipt_watermark;
        match m {
            Msg::Identification(id) => self.info(id.icao24, ts, received_at).call = id.call,
            Msg::AirbornePosition(p) => self.airborne(p, ts, received_at),
            Msg::VelocityOverGround(v) => {
                let info = self.info(v.icao24, ts, received_at);
                update_component(
                    &mut info.speed_kts,
                    &mut info.speed_at,
                    v.velocity_valid.then_some(v.speed_kts),
                    ts,
                );
                update_component(
                    &mut info.heading,
                    &mut info.heading_at,
                    v.velocity_valid.then_some(v.heading_deg),
                    ts,
                );
                update_component(
                    &mut info.geo_minus_baro_ft,
                    &mut info.geo_minus_baro_at,
                    v.geo_minus_baro_valid.then_some(v.geo_minus_baro_ft),
                    ts,
                );
            }
            Msg::AirspeedHeading(a) => {
                let info = self.info(a.icao24, ts, received_at);
                update_component(
                    &mut info.geo_minus_baro_ft,
                    &mut info.geo_minus_baro_at,
                    a.geo_minus_baro_valid.then_some(a.geo_minus_baro_ft),
                    ts,
                );
            }
            Msg::SurfacePosition(_) | Msg::Other { .. } => {}
        }
        Ok(())
    }

    /// `GetPlanes`: aircraft with one unsent accepted position revision and fresh required
    /// components.
    pub fn planes(&mut self) -> Vec<AdsbInfo> {
        self.planes_with_altitude_policy(false)
    }

    /// Take reports qualified for the adapter's requested height policy.
    pub fn planes_with_altitude_policy(&mut self, geometric_only: bool) -> Vec<AdsbInfo> {
        let now = (self.clock)();
        self.prune_expired(now);
        let ttl = self.ttl_duration();
        let mut out = Vec::new();
        for i in &mut self.infos {
            if i.position_revision <= i.emitted_revision {
                continue;
            }
            if i.icao24.is_empty() || i.call.is_empty() || !in_domain(i.lat, i.lon) {
                continue;
            }
            let Some(pos_at) = Self::fresh_time(ttl, i.last_pos, i.timestamp, now) else {
                continue;
            };
            let Some(heading_at) = Self::fresh_time(ttl, i.heading_at, pos_at, now) else {
                continue;
            };
            let Some(speed_at) = Self::fresh_time(ttl, i.speed_at, pos_at, now) else {
                continue;
            };
            if i.heading.is_none() || i.speed_kts.is_none() {
                continue;
            }
            let Some((altitude_ft, altitude_provenance, altitude_at)) =
                Self::select_altitude(i, ttl, now, geometric_only)
            else {
                continue;
            };
            i.emitted_revision = i.position_revision;
            let mut info = AdsbInfo::from(&*i);
            info.altitude_ft = altitude_ft;
            info.altitude_provenance = altitude_provenance;
            let earliest = pos_at.min(heading_at).min(speed_at).min(altitude_at);
            info.components_valid_until = earliest
                .checked_add_signed(ttl)
                .unwrap_or(DateTime::<Utc>::MAX_UTC);
            out.push(info);
        }
        out
    }
}

fn update_component(
    value: &mut Option<i32>,
    observed_at: &mut Option<DateTime<Utc>>,
    sample: Option<i32>,
    at: DateTime<Utc>,
) {
    if observed_at.is_none_or(|previous| at >= previous) {
        // An unavailable sample still advances the watermark.
        *observed_at = Some(at);
        *value = sample;
    }
}

fn in_domain(lat: f64, lon: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
}

fn great_circle_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (lat1, lon1, lat2, lon2) = (
        lat1.to_radians(),
        lon1.to_radians(),
        lat2.to_radians(),
        lon2.to_radians(),
    );
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6371.0 * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn time_zero() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap()
    }

    fn encode_airborne_position(lat: f64, lon: f64, odd: bool) -> AirbornePosition {
        const SCALE: f64 = (1u32 << 17) as f64;
        // Inverse airborne CPR: floor(frac(lat / dLat) * 2^17) and
        // floor(frac(lon / dLon) * 2^17), with dLat=360/(59 or 60) and
        // dLon=360/(NL-lsb), bounded below by one zone.
        let dlat = if odd { 360.0 / 59.0 } else { 6.0 };
        let modulo = |value: f64, divisor: f64| (value % divisor + divisor) % divisor;
        let cpr_lat = (modulo(lat, dlat) / dlat * SCALE).floor() as u32;
        let zones = (cpr::nl(lat) - i32::from(odd)).max(1) as f64;
        let dlon = 360.0 / zones;
        let cpr_lon = (modulo(lon, dlon) / dlon * SCALE).floor() as u32;
        AirbornePosition {
            icao24: 0x3C0001,
            tc: 9,
            odd,
            cpr_lat,
            cpr_lon,
            has_position: true,
            alt_ft: None,
            surveillance_status: 0,
            nic_suppl_b: false,
            utc: false,
        }
    }

    fn reference(lat: f64, lon: f64, at: DateTime<Utc>) -> AdsbState {
        let mut decoder = Decoder::with_clock(5, Box::new(Utc::now));
        let info = decoder.info(0x3C0001, at, at);
        info.lat = lat;
        info.lon = lon;
        info.last_pos = Some(at);
        info.last_pos_odd = Some(false);
        decoder.infos.remove(0)
    }

    #[test]
    fn finding_42_older_same_parity_does_not_replace_newer_word() {
        let at = time_zero();
        let newer_at = at + Duration::seconds(12);
        let mut decoder = Decoder::with_clock(5, Box::new(move || newer_at));
        decoder.airborne(
            encode_airborne_position(10.0, 0.0, false),
            newer_at,
            newer_at,
        );
        decoder.airborne(encode_airborne_position(10.0, 0.0, false), at, newer_at);
        decoder.airborne(
            encode_airborne_position(10.0, 0.0, true),
            at + Duration::seconds(10),
            newer_at,
        );
        let state = &decoder.infos[0];
        assert_eq!(state.last_even, Some(newer_at));
        assert_eq!(state.last_pos, Some(newer_at));
    }

    #[test]
    fn finding_44_emits_only_finite_coordinates_in_domain() {
        let at = time_zero();
        for ((lat, lon), expected) in [
            ((0.0, 0.0), true),
            ((90.0, -180.0), true),
            ((f64::NAN, 0.0), false),
            ((0.0, f64::NAN), false),
            ((f64::INFINITY, 0.0), false),
            ((0.0, f64::NEG_INFINITY), false),
            ((-90.1, 0.0), false),
            ((0.0, 180.1), false),
        ] {
            let mut decoder = Decoder::with_clock(5, Box::new(move || at));
            let info = decoder.info(0x3C0001, at, at);
            info.call = "TE1ST".into();
            info.heading = Some(90);
            info.heading_at = Some(at);
            info.speed_kts = Some(360);
            info.speed_at = Some(at);
            info.baro_alt_ft = Some(35_000);
            info.baro_alt_at = Some(at);
            Decoder::accept_position(info, lat, lon, at, false);
            assert_eq!(
                decoder.planes().len(),
                usize::from(expected),
                "{lat}, {lon}"
            );
        }
    }

    #[test]
    fn finding_359_rejects_local_alias_whose_envelope_touches_pole() {
        let at = time_zero();
        let info = reference(89.8, 0.0, at);
        // Inverse even-CPR encoding of (89.8°, 70°); at NL=1 this longitude is
        // a valid cell alias, but a 10 s travel envelope around 89.8° reaches the pole.
        let pos = encode_airborne_position(89.8, 70.0, false);

        assert!(Decoder::try_local_decode(&info, &pos, at + Duration::seconds(10)).is_none());
    }

    #[test]
    fn finding_359_accepts_midlatitude_local_fix_at_ten_seconds() {
        let at = time_zero();
        let info = reference(45.0, 5.0, at);
        // Inverse odd-CPR encoding of (45.1°, 5.1°), about 13.6 km from the
        // (45°, 5°) reference and within the 10 s, 6,000 kt envelope.
        let pos = encode_airborne_position(45.1, 5.1, true);

        let decoded = Decoder::try_local_decode(&info, &pos, at + Duration::seconds(10)).unwrap();
        assert!((decoded.0 - 45.1).abs() < 0.001);
        assert!((decoded.1 - 5.1).abs() < 0.001);
    }

    #[test]
    fn finding_359_rejects_candidate_beyond_cap_and_cpr_quantum() {
        const SCALE: f64 = (1u32 << 17) as f64;
        let at = time_zero();
        let info = reference(45.0, 5.0, at);
        let travel_rad: f64 = 6000.0 * 1.852 * 10.0 / 3600.0 / 6371.0;
        let travel_deg = travel_rad.to_degrees();
        let odd_lat_quantum = (360.0 / 59.0) / SCALE;
        let target_lat = 45.0 + travel_deg + 4.0 * odd_lat_quantum;
        // Inverse odd-CPR encoding places this candidate beyond the cap plus the
        // reference and candidate latitude quantization allowances.
        let pos = encode_airborne_position(target_lat, 5.0, true);

        assert!(Decoder::try_local_decode(&info, &pos, at + Duration::seconds(10)).is_none());
    }
}
