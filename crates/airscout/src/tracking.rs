//! Tracking values of `bw_Track` (`MapDlg.cs:8054-8155`, `:8251-8281`, `TrackValues.cs`).
//!
//! Only the maths and the status live here; the UDP and file outputs are in `track_outputs` and
//! `track_service`, the rotator output follows in stage 4c; the rig output is `cat_service`. The C# computes the values on
//! a worker thread with a 1000 ms inner loop; the app calls [`track_values`] once a second instead
//! and hands each step to the Track worker.

use crate::tracker::TrackParams;
use as_aircraft::enums::TrackMode;
use as_aircraft::PlaneInfo;
use as_geo::latlon::{bearing, distance};
use as_geo::propagation::{doppler_shift, epsilon_from_heights, slant_range_from_heights};
use as_settings::DopplerStrategy;
use chrono::{DateTime, Utc};

/// One set of tracking values (`TrackValues`). Azimuth and elevation in degrees, distance and
/// slant range in km, Doppler in Hz.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackValues {
    pub timestamp: DateTime<Utc>,
    pub hex: String,
    pub my_azimuth: f64,
    pub my_elevation: f64,
    pub my_distance: f64,
    pub my_slant_range: f64,
    pub my_doppler: f64,
    pub dx_azimuth: f64,
    pub dx_elevation: f64,
    pub dx_distance: f64,
    pub dx_slant_range: f64,
    pub dx_doppler: f64,
    pub rx_hz: i64,
    pub tx_hz: i64,
}

impl TrackValues {
    /// NaN for azimuth, elevation, distance and slant range, 0 for the rest.
    pub fn new(timestamp: DateTime<Utc>) -> Self {
        Self {
            timestamp,
            hex: String::new(),
            my_azimuth: f64::NAN,
            my_elevation: f64::NAN,
            my_distance: f64::NAN,
            my_slant_range: f64::NAN,
            my_doppler: 0.0,
            dx_azimuth: f64::NAN,
            dx_elevation: f64::NAN,
            dx_distance: f64::NAN,
            dx_slant_range: f64::NAN,
            dx_doppler: 0.0,
            rx_hz: 0,
            tx_hz: 0,
        }
    }
}

/// Values of one tracking step (`bw_Track` inner loop, `MapDlg.cs:8059-8139`). `my_h` and `dx_h`
/// are the station heights above sea level [m] (ground elevation plus antenna height), `old` the
/// last reported values. Doppler is computed only when `old` is of the same plane, as the C#
/// invalidates `oldvalues` on a plane change.
pub fn track_values(
    at: DateTime<Utc>,
    plane: Option<&PlaneInfo>,
    mode: TrackMode,
    s: &TrackParams,
    my_h: f64,
    dx_h: f64,
    old: Option<&TrackValues>,
) -> TrackValues {
    let mut v = TrackValues::new(at);
    match mode {
        TrackMode::Track => {
            let Some(p) = plane else { return v };
            // 6371 km times the K factor of the band (:8084-8087); the distance is not scaled
            let radius = s.radius_km;
            let alt = p.alt_m();
            v.hex = p.hex.clone();
            v.my_azimuth = bearing(s.my_lat, s.my_lon, p.lat, p.lon);
            v.dx_azimuth = bearing(s.dx_lat, s.dx_lon, p.lat, p.lon);
            v.my_distance = distance(s.my_lat, s.my_lon, p.lat, p.lon);
            v.my_slant_range = slant_range_from_heights(my_h, v.my_distance, alt, radius);
            v.my_elevation = epsilon_from_heights(my_h, v.my_distance, alt, radius).to_degrees();
            v.dx_distance = distance(s.dx_lat, s.dx_lon, p.lat, p.lon);
            v.dx_slant_range = slant_range_from_heights(dx_h, v.dx_distance, alt, radius);
            v.dx_elevation = epsilon_from_heights(dx_h, v.dx_distance, alt, radius).to_degrees();
            if let Some(o) = old.filter(|o| o.hex == v.hex) {
                // `(old - new) * 1000 / dt` [m/s]: positive when the slant range shrinks (the plane
                // approaches); 0 without elapsed time
                let dt = (v.timestamp - o.timestamp).as_seconds_f64();
                let speed = |old: f64, new: f64| {
                    if dt > 0.0 {
                        (old - new) * 1000.0 / dt
                    } else {
                        0.0
                    }
                };
                let hz = s.band_hz;
                v.my_doppler = doppler_shift(hz, speed(o.my_slant_range, v.my_slant_range));
                v.dx_doppler = doppler_shift(hz, speed(o.dx_slant_range, v.dx_slant_range));
            }
        }
        TrackMode::Single => {
            v.my_azimuth = s.set_az;
            v.my_elevation = s.set_el;
        }
        TrackMode::None | TrackMode::Error => {}
    }
    v
}

/// Values worth tracking (`:8142-8145`): azimuth and elevation are numbers and `0 <= az < 360`.
pub fn valid(v: &TrackValues) -> bool {
    !v.my_azimuth.is_nan()
        && !v.my_elevation.is_nan()
        && v.my_azimuth >= 0.0
        && v.my_azimuth < 360.0
}

/// RX and TX frequency [Hz] of a Doppler strategy (`:8251-8281`). Every Doppler is cast to
/// `i64` on its own (truncation towards zero), as the C# casts to `long`.
pub fn rx_tx(strategy: DopplerStrategy, dial: i64, my: f64, dx: f64) -> (i64, i64) {
    let (my, dx) = (my as i64, dx as i64);
    match strategy {
        DopplerStrategy::A => (dial + my + dx, dial),
        DopplerStrategy::B => (dial, dial - my - dx),
        DopplerStrategy::C => (dial + my + dx, dial - my - dx),
        DopplerStrategy::D => (dial + my, dial - my),
        DopplerStrategy::None => (0, 0),
    }
}

/// `TRACKSTATUS`, the state of the TRK field of the status bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrackStatus {
    #[default]
    None,
    Stopped,
    Single,
    Tracking,
    Error,
}

pub(crate) const DARK_GRAY: [u8; 3] = [169, 169, 169];
pub(crate) const CONTROL: [u8; 3] = [240, 240, 240];

/// `SayTrack("TRK", fore, back)` colours (`:8361-8378`) as RGB: `(text, background)`.
/// `Control` is the system colour of a control, taken as light gray.
pub fn status_colors(s: TrackStatus) -> ([u8; 3], [u8; 3]) {
    match s {
        TrackStatus::None => (DARK_GRAY, CONTROL),
        TrackStatus::Stopped => (CONTROL, DARK_GRAY),
        TrackStatus::Single | TrackStatus::Tracking => ([255, 255, 255], [0, 100, 0]),
        TrackStatus::Error => ([255, 255, 0], [255, 0, 0]),
    }
}

/// State of the ROT field of the status bar (`SayRot`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RotStatus {
    #[default]
    None,
    Stopped,
    Tracking,
    Error,
}

/// `SayRot("ROT", fore, back)` colours (`MapDlg.cs:8403-8417`) as RGB: `(text, background)`.
/// The constants are those of [`status_colors`].
pub fn rot_colors(s: RotStatus) -> ([u8; 3], [u8; 3]) {
    match s {
        RotStatus::None => (DARK_GRAY, CONTROL),
        RotStatus::Stopped => (CONTROL, DARK_GRAY),
        RotStatus::Tracking => ([255, 255, 255], [0, 100, 0]),
        RotStatus::Error => ([255, 255, 0], [255, 0, 0]),
    }
}

/// One step for the Track worker: the values of the tracking step, `Some` only when they are
/// [`valid`], and the mode of the step.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackStep {
    pub values: Option<TrackValues>,
    pub mode: TrackMode,
}
