//! The tracking step as a pure function of a snapshot (`bw_Track` inner loop, `MapDlg.cs:8054-8155`,
//! `:8251-8281`). [`Tracker`] holds `oldvalues` and nothing else: the caller hands it a
//! [`TrackInput`] and the plane cache, so the step can run on any thread.

use crate::tracking::{rx_tx, track_values, valid, TrackStatus, TrackStep, TrackValues};
use as_aircraft::enums::TrackMode;
use as_aircraft::PlaneInfoCache;
use as_settings::{DopplerStrategy, Settings};
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

/// The settings one step reads, copied so a step needs no `Settings`.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackParams {
    pub my_lat: f64,
    pub my_lon: f64,
    pub dx_lat: f64,
    pub dx_lon: f64,
    /// Earth radius times the K factor of the band [km].
    pub radius_km: f64,
    /// Band frequency for the Doppler shift [Hz].
    pub band_hz: f64,
    pub set_az: f64,
    pub set_el: f64,
    pub offset_s: i64,
    pub ttl_min: i64,
    pub dial_freq_hz: i64,
    pub strategy: DopplerStrategy,
}

impl TrackParams {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            my_lat: s.my_lat,
            my_lon: s.my_lon,
            dx_lat: s.dx_lat,
            dx_lon: s.dx_lon,
            radius_km: s.radius_km(),
            band_hz: s.band.to_hz(),
            set_az: s.track.set_az,
            set_el: s.track.set_el,
            offset_s: s.track.offset_s,
            ttl_min: s.planes_position_ttl_min,
            dial_freq_hz: s.doppler.dial_freq_hz,
            strategy: s.doppler.strategy,
        }
    }
}

/// What the UI hands to the step. `params == None` means no step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackInput {
    pub rev: u64,
    /// LIFE mode (the C# stops tracking in HISTORY).
    pub life: bool,
    pub mode: TrackMode,
    /// The selected plane.
    pub hex: Option<String>,
    /// Heights above sea level [m] of My and DX, antenna height included.
    pub heights: Option<(f64, f64)>,
    pub params: Option<TrackParams>,
}

/// What the UI shows after a step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackShown {
    pub rev: u64,
    pub values: Option<TrackValues>,
    pub status: TrackStatus,
}

/// The input the UI writes and the Track worker copies at each step.
pub type TrackInputCell = Arc<Mutex<TrackInput>>;

/// What the Track worker showed last; the UI reads it.
pub type TrackShownCell = Arc<Mutex<TrackShown>>;

/// `oldvalues` of the C# loop: the last valid step. Survives Pause and Play.
#[derive(Default)]
pub struct Tracker {
    old: Option<TrackValues>,
}

impl Tracker {
    /// One step. `None` outside LIFE or without params; otherwise the step for the Track worker
    /// (`values` only when [`valid`]) and what the UI shows.
    pub fn step(
        &mut self,
        input: &TrackInput,
        now: DateTime<Utc>,
        cache: &PlaneInfoCache,
    ) -> Option<(TrackStep, TrackShown)> {
        if !input.life {
            return None;
        }
        let p = input.params.as_ref()?;
        let mode = input.mode;
        let hex = input.hex.as_deref();
        // `oldvalues` is dropped as soon as a step runs for another plane, valid or not (:8064-8067)
        if let (TrackMode::Track, Some(hex)) = (mode, hex) {
            if self.old.as_ref().is_some_and(|o| o.hex != hex) {
                self.old = None;
            }
        }
        let shown = |values: Option<TrackValues>, status| TrackShown {
            rev: input.rev,
            values,
            status,
        };
        let time = now + chrono::Duration::seconds(p.offset_s);
        let plane = match (mode, hex) {
            (TrackMode::Track, Some(hex)) => cache.get(hex, time, p.ttl_min),
            _ => None,
        };
        // only TRACK reads the station heights (:8080-8083); SINGLE and NONE do without
        let (my_h, dx_h) = if mode == TrackMode::Track {
            match input.heights {
                Some(h) => h,
                // the worker has not answered yet: no values, nothing blocks
                None => {
                    return Some((
                        TrackStep { values: None, mode },
                        shown(None, TrackStatus::Stopped),
                    ));
                }
            }
        } else {
            (0.0, 0.0)
        };
        let mut v = track_values(time, plane.as_ref(), mode, p, my_h, dx_h, self.old.as_ref());
        if !valid(&v) {
            return Some((
                TrackStep { values: None, mode },
                shown(None, TrackStatus::Stopped),
            ));
        }
        (v.rx_hz, v.tx_hz) = rx_tx(p.strategy, p.dial_freq_hz, v.my_doppler, v.dx_doppler);
        // the C# reports SINGLE and then STOPPED in the same step (`:8148-8153`, `:8293-8297`)
        let status = match mode {
            TrackMode::Track => TrackStatus::Tracking,
            _ => TrackStatus::Stopped,
        };
        self.old = Some(v.clone());
        Some((
            TrackStep {
                values: Some(v.clone()),
                mode,
            },
            shown(Some(v), status),
        ))
    }
}
