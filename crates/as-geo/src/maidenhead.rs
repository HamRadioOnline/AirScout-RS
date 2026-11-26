//! Port of `ScoutBase.Core/MaidenheadLocator.cs` (by G. Monz, DK7IO) plus the parts of
//! `GeographicalPoint.cs` and `Utilities.cs` it depends on.

use crate::latlon::{GPoint, GRect};

/// Number of zones for 'Field' (precision step 1).
pub const ZONES_ODD_STEP1: i32 = 18;
/// Number of zones for 'Subsquare' etc. (precision steps 3, 5, ...).
pub const ZONES_ODD_STEPS_EXCEPT1: i32 = 24;
/// Number of zones for 'Square' etc. (precision steps 2, 4, ...).
pub const ZONES_EVEN_STEPS: i32 = 10;
/// Lowest allowed precision.
pub const MIN_PRECISION: i32 = 1;
/// Highest allowed precision.
pub const MAX_PRECISION: i32 = 9;

const LOWER_LAT: f64 = -90.0;
const UPPER_LAT: f64 = 90.0;
const LOWER_LON: f64 = -180.0;
const UPPER_LON: f64 = 180.0;

/// Position of a coordinate inside a locator rectangle (order as in `GeographicalPoint.cs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PositionInRectangle {
    TopLeft,
    TopMiddle,
    TopRight,
    BottomLeft,
    BottomMiddle,
    BottomRight,
    MiddleLeft,
    MiddleRight,
    MiddleMiddle,
}

/// Subgrid filter for [`get_sub_grids`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubGridsFilter {
    All,
    Top,
    Bottom,
    Left,
    Right,
}

/// Locator errors (the C# code throws `ArgumentException` in these cases).
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum LocatorError {
    #[error("invalid Maidenhead locator: {0:?}")]
    Invalid(String),
}

/// `GeographicalPoint.Check`.
pub fn check_lat_lon(lat: f64, lon: f64) -> bool {
    (LOWER_LAT..=UPPER_LAT).contains(&lat) && (LOWER_LON..=UPPER_LON).contains(&lon)
}

/// Checks if a string is a valid Maidenhead locator (`MaidenheadLocator.Check`).
pub fn check(loc: &str) -> bool {
    if loc.is_empty() {
        return false;
    }
    let up: Vec<char> = loc.to_uppercase().chars().collect();
    let len = up.len() as i32;
    if len % 2 != 0 || len / 2 < MIN_PRECISION || len / 2 > MAX_PRECISION {
        return false;
    }
    up.iter().enumerate().all(|(i, &ch)| {
        if i % 4 <= 1 {
            ('A'..='X').contains(&ch)
        } else {
            ch.is_numeric()
        }
    })
}

/// `MaidenheadLocator.IsPrecise(lat, lon, precision)`.
///
/// `Err` where C# throws: e.g. lat = 90 produces a locator character outside the valid zone
/// range, which the reverse conversion rejects.
pub fn is_precise(lat: f64, lon: f64, precision: i32) -> Result<bool, LocatorError> {
    let Some(loc) = loc_from_lat_lon_fixed(lat, lon, false, precision) else {
        return Ok(false);
    };
    if loc.is_empty() {
        return Ok(false);
    }
    let (mlat, mlon) = lat_lon_from_loc(&loc, PositionInRectangle::MiddleMiddle)?;
    Ok((mlat - lat).abs() > 0.00001 || (mlon - lon).abs() > 0.00001)
}

/// `MaidenheadLocator.Convert`: normalises case. Empty input returns `""`.
pub fn convert(loc: &str, small_letters_for_subsquares: bool) -> Result<String, LocatorError> {
    if loc.is_empty() {
        return Ok(String::new());
    }
    if !check(loc) {
        return Err(LocatorError::Invalid(loc.to_string()));
    }
    Ok(loc
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            if i > 1 && small_letters_for_subsquares {
                ch.to_ascii_lowercase()
            } else {
                ch.to_ascii_uppercase()
            }
        })
        .collect())
}

/// `MaidenheadLocator.LocFromLatLon`. `Ok(None)` where C# returns `null` (coordinates out of
/// range); `Err` where C# throws (only possible with `autolength`, via [`is_precise`]).
pub fn loc_from_lat_lon(
    lat: f64,
    lon: f64,
    small_letters_for_subsquares: bool,
    precision: i32,
    autolength: bool,
) -> Result<Option<String>, LocatorError> {
    if !autolength {
        return Ok(loc_from_lat_lon_fixed(
            lat,
            lon,
            small_letters_for_subsquares,
            precision,
        ));
    }
    let mut p = 3;
    let mut loc;
    loop {
        loc = loc_from_lat_lon_fixed(lat, lon, small_letters_for_subsquares, p);
        if is_precise(lat, lon, p)? {
            p += 1;
        } else {
            break;
        }
        if p > precision {
            break;
        }
    }
    Ok(loc)
}

fn loc_from_lat_lon_fixed(lat: f64, lon: f64, small: bool, precision: i32) -> Option<String> {
    if !check_lat_lon(lat, lon) {
        return None;
    }
    let precision = precision.clamp(MIN_PRECISION, MAX_PRECISION);
    let mut chars = String::new();
    let mut lat_work = lat + (-LOWER_LAT);
    let mut lon_work = lon + (-LOWER_LON);
    let (mut height, mut width) = initialize_zone_size();
    for step in MIN_PRECISION..=precision {
        let (zones, first) = retrieve_step_values(step, small);
        height /= zones as f64;
        width /= zones as f64;
        let lon_zone = get_zone(lon_work, width);
        chars.push(char::from_u32(first as u32 + lon_zone as u32).unwrap_or('?'));
        let lat_zone = get_zone(lat_work, height);
        chars.push(char::from_u32(first as u32 + lat_zone as u32).unwrap_or('?'));
        if step < MAX_PRECISION {
            lat_work -= lat_zone as f64 * height;
            lon_work -= lon_zone as f64 * width;
            if lat_work < 0.0 {
                lat_work = 0.0;
            }
            if lon_work < 0.0 {
                lon_work = 0.0;
            }
        }
    }
    Some(chars)
}

/// `MaidenheadLocator.LatFromLoc`.
pub fn lat_from_loc(loc: &str, pos: PositionInRectangle) -> Result<f64, LocatorError> {
    lat_lon_from_loc(loc, pos).map(|(lat, _)| lat)
}

/// `MaidenheadLocator.LonFromLoc`.
pub fn lon_from_loc(loc: &str, pos: PositionInRectangle) -> Result<f64, LocatorError> {
    lat_lon_from_loc(loc, pos).map(|(_, lon)| lon)
}

/// `MaidenheadLocator.GPointFromLoc`.
pub fn gpoint_from_loc(loc: &str, pos: PositionInRectangle) -> Result<GPoint, LocatorError> {
    lat_lon_from_loc(loc, pos).map(|(lat, lon)| GPoint::new(lat, lon))
}

/// `MaidenheadLocator.BoundsFromLoc`.
pub fn bounds_from_loc(loc: &str) -> Result<GRect, LocatorError> {
    let min = gpoint_from_loc(loc, PositionInRectangle::BottomLeft)?;
    let max = gpoint_from_loc(loc, PositionInRectangle::TopRight)?;
    Ok(GRect::new(min.lat, min.lon, max.lat, max.lon))
}

/// `MaidenheadLocator.LatLonFromLoc`, returns `(lat, lon)`.
pub fn lat_lon_from_loc(loc: &str, pos: PositionInRectangle) -> Result<(f64, f64), LocatorError> {
    let err = || LocatorError::Invalid(loc.to_string());
    let up: Vec<char> = loc.to_uppercase().chars().collect();
    if up.is_empty() || !up.len().is_multiple_of(2) {
        return Err(err());
    }
    let precision = (up.len() / 2) as i32;
    let mut lat = LOWER_LAT;
    let mut lon = LOWER_LON;
    let (mut height, mut width) = initialize_zone_size();
    for step in 1..=precision {
        let (zones, first) = retrieve_step_values(step, false);
        height /= zones as f64;
        width /= zones as f64;
        let lon_zone = up[(step * 2 - 2) as usize] as i64 - first as i64;
        if !(0..zones as i64).contains(&lon_zone) {
            return Err(err());
        }
        let lat_zone = up[(step * 2 - 1) as usize] as i64 - first as i64;
        if !(0..zones as i64).contains(&lat_zone) {
            return Err(err());
        }
        lon += lon_zone as f64 * width;
        lat += lat_zone as f64 * height;
    }
    shift_position_in_rectangle(&mut lat, &mut lon, pos, height, width);
    Ok((lat, lon))
}

/// `MaidenheadLocator.GetSubGrids`.
pub fn get_sub_grids(
    loc: &str,
    filter: SubGridsFilter,
    small_letters_for_subsquares: bool,
    reverse: bool,
) -> Result<Vec<String>, LocatorError> {
    let length = loc.chars().count() as i32;
    if length % 2 != 0 {
        return Err(LocatorError::Invalid(loc.to_string()));
    }
    let sub_precision = length / 2 + 1;
    let (zones, first) = retrieve_step_values(sub_precision, small_letters_for_subsquares);
    let (mut w_start, mut w_end, mut h_start, mut h_end) = (1, zones, 1, zones);
    match filter {
        SubGridsFilter::All => {}
        SubGridsFilter::Top => {
            w_start = 1;
            w_end = zones;
            h_start = zones;
            h_end = zones;
        }
        SubGridsFilter::Bottom => {
            w_start = 1;
            h_start = 1;
            h_end = 1;
            w_end = zones;
        }
        SubGridsFilter::Left => {
            w_start = 1;
            w_end = 1;
            h_start = 1;
            h_end = zones;
        }
        SubGridsFilter::Right => {
            w_start = zones;
            w_end = zones;
            h_end = zones;
            h_start = 1;
        }
    }
    let mut result = Vec::new();
    for w in w_start..=w_end {
        for h in h_start..=h_end {
            let mut s = loc.to_string();
            s.push(char::from_u32(first as u32 + w as u32 - 1).unwrap_or('?'));
            s.push(char::from_u32(first as u32 + h as u32 - 1).unwrap_or('?'));
            result.push(s);
        }
    }
    if reverse {
        result.reverse();
    }
    Ok(result)
}

fn initialize_zone_size() -> (f64, f64) {
    (UPPER_LAT - LOWER_LAT, UPPER_LON - LOWER_LON)
}

fn retrieve_step_values(step: i32, small: bool) -> (i32, char) {
    if step % 2 == 0 {
        (ZONES_EVEN_STEPS, '0')
    } else {
        let zones = if step == 1 {
            ZONES_ODD_STEP1
        } else {
            ZONES_ODD_STEPS_EXCEPT1
        };
        let first = if step >= 3 && small { 'a' } else { 'A' };
        (zones, first)
    }
}

/// `Utilities.Round`: round to a multiple of `interval`, midpoint away from zero.
fn round_to(value: f64, interval: f64) -> f64 {
    let q = (value / interval).round() as i32;
    q as f64 * interval
}

fn get_zone(value: f64, interval: f64) -> i32 {
    let mut result = (value / interval) as i32;
    let rounded = round_to(value, interval);
    let diff = rounded - value;
    if diff < 0.0000000000001 && rounded > value {
        result += 1;
    }
    result
}

fn shift_position_in_rectangle(
    lat: &mut f64,
    lon: &mut f64,
    pos: PositionInRectangle,
    height: f64,
    width: f64,
) {
    use PositionInRectangle::*;
    if matches!(pos, TopLeft | TopMiddle | TopRight) {
        *lat += height;
    }
    if matches!(pos, MiddleLeft | MiddleMiddle | MiddleRight) {
        *lat += height / 2.0;
    }
    if matches!(pos, TopRight | MiddleRight | BottomRight) {
        *lon += width;
    }
    if matches!(pos, TopMiddle | MiddleMiddle | BottomMiddle) {
        *lon += width / 2.0;
    }
}
