//! Port of `LibADSB/ADSBDecoder.cs`: per-aircraft state built from a stream of messages.
//!
//! Kept from C# (see `docs/logic-findings.md`):
//! - even/odd pairing uses the wall clock, not the message time, with a 10 s window (#42);
//! - the TTL is stored but never used (#43);
//! - `GetPlanes` compares lat/lon with `NaN` using `!=`, so the check never rejects (#44);
//! - a global position fix does not update `Timestamp`; an aircraft whose first fix arrives
//!   after a `GetPlanes` call stays hidden until its next local fix (#45);
//! - once a position is known, every later message is decoded locally against it, forever.

use crate::cpr;
use crate::msg::{self, AirbornePosition, Msg};
use crate::{AdsbError, ModeSReply};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Clock used for pairing and `GetPlanes` (C# `DateTime.UtcNow`).
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
    pub geo_minus_baro_ft: Option<i32>,
    pub heading: Option<i32>,
    pub speed_kts: Option<i32>,
    pub last_even: Option<DateTime<Utc>>,
    pub last_odd: Option<DateTime<Utc>>,
    pub last_pos: Option<DateTime<Utc>>,
    pub num_pos: i32,
    last_even_msg: Option<AirbornePosition>,
    last_odd_msg: Option<AirbornePosition>,
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
    pub geo_minus_baro_ft: f64,
    pub heading: f64,
    pub speed_kts: f64,
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
            geo_minus_baro_ft: nan_or(s.geo_minus_baro_ft),
            heading: nan_or(s.heading),
            speed_kts: nan_or(s.speed_kts),
        }
    }
}

/// The stateful decoder (`ADSBDecoder`).
pub struct Decoder {
    ttl_min: i64,
    clock: Clock,
    last_send: DateTime<Utc>,
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
        let last_send = clock();
        Self {
            ttl_min,
            clock,
            last_send,
            index: HashMap::new(),
            infos: Vec::new(),
        }
    }

    /// The configured TTL [min]. Unused, as in C# (#43).
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

    fn info(&mut self, icao24: u32) -> &mut AdsbState {
        let key = format!("{icao24:06X}");
        let idx = match self.index.get(&key) {
            Some(&i) => i,
            None => {
                let now = (self.clock)();
                self.infos.push(AdsbState {
                    timestamp: now,
                    icao24: key.clone(),
                    call: String::new(),
                    lat: f64::NAN,
                    lon: f64::NAN,
                    baro_alt_ft: None,
                    geo_minus_baro_ft: None,
                    heading: None,
                    speed_kts: None,
                    last_even: None,
                    last_odd: None,
                    last_pos: None,
                    num_pos: 0,
                    last_even_msg: None,
                    last_odd_msg: None,
                });
                self.index.insert(key, self.infos.len() - 1);
                self.infos.len() - 1
            }
        };
        &mut self.infos[idx]
    }

    /// `DecodeMessage`: `raw` is `*HEX;` (first and last character are dropped unseen).
    /// Parity failures and unsupported messages are ignored (`Ok`); malformed input is an
    /// error, where C# throws.
    pub fn decode_message(
        &mut self,
        raw: &str,
        ts: DateTime<Utc>,
        _use_geo_alt_only: bool,
    ) -> Result<(), AdsbError> {
        let n = raw.chars().count();
        if n < 2 {
            return Err(AdsbError::BadFormat(format!("message too short: {raw}")));
        }
        let inner: String = raw.chars().skip(1).take(n - 2).collect();
        let m = msg::decode(&inner)?;
        if !ModeSReply::from_hex(&inner)?.check_parity() {
            return Ok(());
        }
        match m {
            Msg::Identification(id) => self.info(id.icao24).call = id.call,
            Msg::AirbornePosition(p) => self.airborne(p, ts),
            Msg::VelocityOverGround(v) => {
                let info = self.info(v.icao24);
                if v.velocity_valid {
                    info.speed_kts = Some(v.speed_kts);
                    info.heading = Some(v.heading_deg);
                }
                info.geo_minus_baro_ft = v.geo_minus_baro_valid.then_some(v.geo_minus_baro_ft);
            }
            Msg::AirspeedHeading(a) => {
                let info = self.info(a.icao24);
                if a.airspeed_valid {
                    info.speed_kts = Some(a.airspeed_kts);
                }
                if a.heading_valid {
                    info.heading = Some(a.heading_deg);
                }
                info.geo_minus_baro_ft = a.geo_minus_baro_valid.then_some(a.geo_minus_baro_ft);
            }
            Msg::SurfacePosition(_) | Msg::Other { .. } => {}
        }
        Ok(())
    }

    fn airborne(&mut self, pos: AirbornePosition, ts: DateTime<Utc>) {
        let now = (self.clock)();
        let info = self.info(pos.icao24);
        if !pos.has_position {
            return;
        }
        if !info.lat.is_nan() && !info.lon.is_nan() {
            if let Ok((lat, lon)) = cpr::local(&pos, info.lat, info.lon) {
                info.lat = lat;
                info.lon = lon;
                if let Some(a) = pos.alt_ft {
                    info.baro_alt_ft = Some(a);
                }
                info.timestamp = ts;
                info.num_pos += 1;
                return;
            }
        }
        info.last_pos = None;
        info.num_pos = 0;
        let paired = if pos.odd {
            info.last_odd = Some(now);
            let r = match (&info.last_even_msg, info.last_even) {
                (Some(even), Some(te)) if (now - te).num_milliseconds() as f64 / 1000.0 <= 10.0 => {
                    Some(cpr::global(even, &pos, true))
                }
                _ => None,
            };
            info.last_odd_msg = Some(pos.clone());
            r
        } else {
            info.last_even = Some(now);
            let r = match (&info.last_odd_msg, info.last_odd) {
                (Some(odd), Some(to)) if (now - to).num_milliseconds() as f64 / 1000.0 <= 10.0 => {
                    Some(cpr::global(&pos, odd, false))
                }
                _ => None,
            };
            info.last_even_msg = Some(pos.clone());
            r
        };
        match paired {
            Some(Ok((lat, lon))) => {
                info.lat = lat;
                info.lon = lon;
                if let Some(a) = pos.alt_ft {
                    info.baro_alt_ft = Some(a);
                }
                info.last_pos = Some(ts);
                info.num_pos += 1;
            }
            Some(Err(_)) => {
                info.last_pos = None;
                info.num_pos = 0;
            }
            None => {}
        }
    }

    /// `GetPlanes`: aircraft updated since the previous call that have ICAO, call, altitude,
    /// speed and heading. Lat/lon are not checked (#44).
    pub fn planes(&mut self) -> Vec<AdsbInfo> {
        let out = self
            .infos
            .iter()
            .filter(|i| i.timestamp > self.last_send)
            .filter(|i| {
                !i.icao24.is_empty()
                    && !i.call.is_empty()
                    && i.baro_alt_ft.is_some()
                    && i.speed_kts.is_some()
                    && i.heading.is_some()
            })
            .map(AdsbInfo::from)
            .collect();
        self.last_send = (self.clock)();
        out
    }
}
