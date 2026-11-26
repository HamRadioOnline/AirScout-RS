//! Compact Position Reporting: `NL`, `getGlobalPosition` and `getLocalPosition` of
//! `AirbornePositionMsg` / `SurfacePositionMsg`.

use crate::{AdsbError, AirbornePosition, SurfacePosition};

const CPR_SCALE: f64 = (1u32 << 17) as f64;

/// A position message that carries CPR coordinates.
pub trait CprMsg {
    /// 360° for airborne, 90° for surface positions.
    const ZONE: f64;
    fn icao24(&self) -> u32;
    fn odd(&self) -> bool;
    fn cpr_lat(&self) -> u32;
    fn cpr_lon(&self) -> u32;
    fn has_position(&self) -> bool;
}

macro_rules! cpr_msg {
    ($t:ty, $zone:expr) => {
        impl CprMsg for $t {
            const ZONE: f64 = $zone;
            fn icao24(&self) -> u32 {
                self.icao24
            }
            fn odd(&self) -> bool {
                self.odd
            }
            fn cpr_lat(&self) -> u32 {
                self.cpr_lat
            }
            fn cpr_lon(&self) -> u32 {
                self.cpr_lon
            }
            fn has_position(&self) -> bool {
                self.has_position
            }
        }
    };
}
cpr_msg!(AirbornePosition, 360.0);
cpr_msg!(SurfacePosition, 90.0);

fn nl_f(rlat: f64) -> f64 {
    if rlat == 0.0 {
        return 59.0;
    } else if rlat.abs() == 87.0 {
        return 2.0;
    } else if rlat.abs() > 87.0 {
        return 1.0;
    }
    let tmp = 1.0
        - (1.0 - (std::f64::consts::PI / (2.0 * 15.0)).cos())
            / (std::f64::consts::PI / 180.0 * rlat.abs()).cos().powi(2);
    (2.0 * std::f64::consts::PI / tmp.acos()).floor()
}

/// Number of longitude zones at `lat` (`NL`).
pub fn nl(lat: f64) -> i32 {
    nl_f(lat) as i32
}

fn modulo(a: f64, b: f64) -> f64 {
    ((a % b) + b) % b
}

/// `getGlobalPosition` from an even and an odd message; `newer_is_odd` says which one is
/// `this` in C# (its zone count picks the longitude). Returns (lat, lon). The result is not
/// range-checked: a bad pair can give a latitude above 90° (logic-findings #36).
pub fn global<T: CprMsg>(even: &T, odd: &T, newer_is_odd: bool) -> Result<(f64, f64), AdsbError> {
    let (newer, other) = if newer_is_odd {
        (odd, even)
    } else {
        (even, odd)
    };
    if other.icao24() != newer.icao24() {
        return Err(AdsbError::IllegalArgument(format!(
            "Transmitter of other message ({:06X}) not equal to this ({:06X})",
            other.icao24(),
            newer.icao24()
        )));
    }
    if other.odd() == newer.odd() {
        return Err(AdsbError::BadFormat(format!(
            "Expected {} message format",
            if newer.odd() { "even" } else { "odd" }
        )));
    }
    if !newer.has_position() {
        return Err(AdsbError::MissingInformation(
            "No position information available!".into(),
        ));
    }
    if !other.has_position() {
        return Err(AdsbError::MissingInformation(
            "Other message has no position information.".into(),
        ));
    }
    let z = T::ZONE;
    let (elat, elon) = (f64::from(even.cpr_lat()), f64::from(even.cpr_lon()));
    let (olat, olon) = (f64::from(odd.cpr_lat()), f64::from(odd.cpr_lon()));
    let dlat0 = z / 60.0;
    let dlat1 = z / 59.0;
    let j = ((59.0 * elat - 60.0 * olat) / CPR_SCALE + 0.5).floor();
    let mut rlat0 = dlat0 * (modulo(j, 60.0) + elat / CPR_SCALE);
    let mut rlat1 = dlat1 * (modulo(j, 59.0) + olat / CPR_SCALE);
    if (270.0..=360.0).contains(&rlat0) {
        rlat0 -= 360.0;
    }
    if (270.0..=360.0).contains(&rlat1) {
        rlat1 -= 360.0;
    }
    if (-360.0..=-270.0).contains(&rlat0) {
        rlat0 += 360.0;
    }
    if (-360.0..=-270.0).contains(&rlat1) {
        rlat1 += 360.0;
    }
    if nl_f(rlat0) != nl_f(rlat1) {
        return Err(AdsbError::PositionStraddle);
    }
    let dlon0 = z / nl_f(rlat0).max(1.0);
    let dlon1 = z / (nl_f(rlat1) - 1.0).max(1.0);
    let nl_helper = nl_f(if newer_is_odd { rlat1 } else { rlat0 });
    let m = ((elon * (nl_helper - 1.0) - olon * nl_helper) / CPR_SCALE + 0.5).floor();
    let mut rlon0 = dlon0 * (modulo(m, nl_f(rlat0).max(1.0)) + elon / CPR_SCALE);
    let mut rlon1 = dlon1 * (modulo(m, (nl_f(rlat1) - 1.0).max(1.0)) + olon / CPR_SCALE);
    let wrap = |r: &mut f64| {
        if *r < -180.0 && *r > -360.0 {
            *r += 360.0;
        }
    };
    wrap(&mut rlon0);
    wrap(&mut rlon1);
    if rlon0 > 180.0 && rlon0 < 360.0 {
        rlon0 -= 360.0;
    }
    if rlon1 > 180.0 && rlon1 < 360.0 {
        rlon1 -= 360.0;
    }
    Ok(if newer_is_odd {
        (rlat1, rlon1)
    } else {
        (rlat0, rlon0)
    })
}

/// `getLocalPosition` relative to a reference position. C# casts the CPR longitude to `float`
/// in one term; a 17-bit integer is exact in `float`, so the cast changes nothing
/// (logic-findings #37).
pub fn local<T: CprMsg>(msg: &T, ref_lat: f64, ref_lon: f64) -> Result<(f64, f64), AdsbError> {
    if !msg.has_position() {
        return Err(AdsbError::MissingInformation(
            "No position information available!".into(),
        ));
    }
    let z = T::ZONE;
    let (clat, clon) = (f64::from(msg.cpr_lat()), f64::from(msg.cpr_lon()));
    let dlat = if msg.odd() { z / 59.0 } else { z / 60.0 };
    let j =
        (ref_lat / dlat).floor() + (0.5 + modulo(ref_lat, dlat) / dlat - clat / CPR_SCALE).floor();
    let rlat = dlat * (j + clat / CPR_SCALE);
    let dlon = z / (nl_f(rlat) - if msg.odd() { 1.0 } else { 0.0 }).max(1.0);
    let m = (ref_lon / dlon).floor()
        + (0.5 + modulo(ref_lon, dlon) / dlon - f64::from(clon as f32) / CPR_SCALE).floor();
    let rlon = dlon * (m + clon / CPR_SCALE);
    Ok((rlat, rlon))
}
