//! Port of `ScoutBase.Core/LatLon.cs` (formulas courtesy of movable-type.co.uk).
//!
//! The arithmetic keeps the original operation order (`x / 180 * PI` instead of
//! `to_radians()`) so results are bit-for-bit comparable with the C# code.

use std::f64::consts::PI;

/// Mean earth radius in km (`LatLon.Earth.Radius`).
pub const EARTH_RADIUS_KM: f64 = 6371.0;

/// A geographical point in degrees.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GPoint {
    pub lat: f64,
    pub lon: f64,
}

impl GPoint {
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

/// A lat/lon rectangle in degrees.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GRect {
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}

impl GRect {
    pub const fn new(min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64) -> Self {
        Self {
            min_lat,
            min_lon,
            max_lat,
            max_lon,
        }
    }
}

#[inline]
fn rad(deg: f64) -> f64 {
    deg / 180.0 * PI
}

/// Great-circle distance in km (haversine, spherical earth).
pub fn distance(mylat: f64, mylon: f64, lat: f64, lon: f64) -> f64 {
    let r = EARTH_RADIUS_KM;
    let d_lat = mylat - lat;
    let d_lon = mylon - lon;
    let a = (d_lat / 180.0 * PI / 2.0).sin() * (d_lat / 180.0 * PI / 2.0).sin()
        + (d_lon / 180.0 * PI / 2.0).sin()
            * (d_lon / 180.0 * PI / 2.0).sin()
            * rad(mylat).cos()
            * rad(lat).cos();
    r * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}

/// Initial bearing in degrees `[0, 360)`.
pub fn bearing(mylat: f64, mylon: f64, lat: f64, lon: f64) -> f64 {
    let d_lon = lon - mylon;
    let y = rad(d_lon).sin() * rad(lat).cos();
    let x =
        rad(mylat).cos() * rad(lat).sin() - rad(mylat).sin() * rad(lat).cos() * rad(d_lon).cos();
    // C# `%` on doubles == Rust `%` (truncated remainder)
    (y.atan2(x) / PI * 180.0 + 360.0) % 360.0
}

/// Spherical midpoint.
pub fn midpoint(mylat: f64, mylon: f64, lat: f64, lon: f64) -> GPoint {
    let aslondiff = mylon - lon;
    let bx = rad(lat).cos() * (-aslondiff / 180.0 * PI).cos();
    let by = rad(lat).cos() * (-aslondiff / 180.0 * PI).sin();
    let latm = (rad(mylat).sin() + rad(lat).sin())
        .atan2(((rad(mylat).cos() + bx) * (rad(mylat).cos() + bx) + by * by).sqrt())
        / PI
        * 180.0;
    let lonm = mylon + by.atan2(rad(mylat).cos() + bx) / PI * 180.0;
    GPoint::new(latm, lonm)
}

/// Destination point from start, bearing (deg) and distance (km).
pub fn destination_point(mylat: f64, mylon: f64, bearing: f64, distance: f64) -> GPoint {
    let r = EARTH_RADIUS_KM;
    let lat = (rad(mylat).sin() * (distance / r).cos()
        + rad(mylat).cos() * (distance / r).sin() * rad(bearing).cos())
    .asin();
    let lon = mylon / 180.0 * PI
        + (rad(bearing).sin() * (distance / r).sin() * rad(mylat).cos())
            .atan2((distance / r).cos() - rad(mylat).sin() * lat.sin());
    GPoint::new(lat / PI * 180.0, lon / PI * 180.0)
}

/// Intersection of two great circles given by point + bearing.
/// `None` where the original returns `null` (coincident points, infinite or ambiguous intersection).
pub fn intersection_point(
    mylat: f64,
    mylon: f64,
    mybearing: f64,
    lat: f64,
    lon: f64,
    bearing: f64,
) -> Option<GPoint> {
    let d_lat = lat - mylat;
    let d_lon = lon - mylon;
    let brng13 = mybearing / 180.0 * PI;
    let brng23 = bearing / 180.0 * PI;
    let dist12 = 2.0
        * ((d_lat / 2.0 / 180.0 * PI).sin() * (d_lat / 2.0 / 180.0 * PI).sin()
            + rad(mylat).cos()
                * rad(lat).cos()
                * (d_lon / 2.0 / 180.0 * PI).sin()
                * (d_lon / 2.0 / 180.0 * PI).sin())
        .sqrt()
        .asin();
    if dist12 == 0.0 {
        return None;
    }
    let mut brng_a = ((rad(lat).sin() - rad(mylat).sin() * dist12.cos())
        / (dist12.sin() * rad(mylat).cos()))
    .acos();
    if brng_a.is_nan() {
        brng_a = 0.0;
    }
    let brng_b = ((rad(mylat).sin() - rad(lat).sin() * dist12.cos())
        / (dist12.sin() * rad(lat).cos()))
    .acos();
    let (brng12, brng21) = if (lon / 180.0 * PI - mylon / 180.0 * PI).sin() > 0.0 {
        (brng_a, 2.0 * PI - brng_b)
    } else {
        (2.0 * PI - brng_a, brng_b)
    };
    let alpha1 = (brng13 - brng12 + PI) % (2.0 * PI) - PI;
    let alpha2 = (brng21 - brng23 + PI) % (2.0 * PI) - PI;
    if alpha1.sin() == 0.0 && alpha2.sin() == 0.0 {
        return None;
    }
    if alpha1.sin() * alpha2.sin() < 0.0 {
        return None;
    }
    let alpha3 = (-alpha1.cos() * alpha2.cos() + alpha1.sin() * alpha2.sin() * dist12.cos()).acos();
    let dist13 = (dist12.sin() * alpha1.sin() * alpha2.sin())
        .atan2(alpha2.cos() + alpha1.cos() * alpha3.cos());
    let lat3 =
        (rad(mylat).sin() * dist13.cos() + rad(mylat).cos() * dist13.sin() * brng13.cos()).asin();
    let d_lon13 = (brng13.sin() * dist13.sin() * rad(mylat).cos())
        .atan2(dist13.cos() - rad(mylat).sin() * lat3.sin());
    let mut lon3 = mylon / 180.0 * PI + d_lon13;
    lon3 = (lon3 + 3.0 * PI) % (2.0 * PI) - PI;
    Some(GPoint::new(lat3 / PI * 180.0, lon3 / PI * 180.0))
}

/// `LatLon.Distance(myloc, loc)` between locator centres.
pub fn distance_loc(myloc: &str, loc: &str) -> Result<f64, crate::maidenhead::LocatorError> {
    let a = crate::maidenhead::gpoint_from_loc(
        myloc,
        crate::maidenhead::PositionInRectangle::MiddleMiddle,
    )?;
    let b = crate::maidenhead::gpoint_from_loc(
        loc,
        crate::maidenhead::PositionInRectangle::MiddleMiddle,
    )?;
    Ok(distance(a.lat, a.lon, b.lat, b.lon))
}

/// `LatLon.Bearing(myloc, loc)` between locator centres.
pub fn bearing_loc(myloc: &str, loc: &str) -> Result<f64, crate::maidenhead::LocatorError> {
    let a = crate::maidenhead::gpoint_from_loc(
        myloc,
        crate::maidenhead::PositionInRectangle::MiddleMiddle,
    )?;
    let b = crate::maidenhead::gpoint_from_loc(
        loc,
        crate::maidenhead::PositionInRectangle::MiddleMiddle,
    )?;
    Ok(bearing(a.lat, a.lon, b.lat, b.lon))
}

/// `LatLon.IsPrecise(loc, lat, lon)`. Note: the original passes the locator *character count*
/// as precision (not the pair count); reproduced as-is.
pub fn is_precise_loc(
    loc: &str,
    lat: f64,
    lon: f64,
) -> Result<bool, crate::maidenhead::LocatorError> {
    use crate::maidenhead::{lat_lon_from_loc, loc_from_lat_lon, PositionInRectangle};
    let loc = loc.to_uppercase();
    let (mlat, mlon) = lat_lon_from_loc(&loc, PositionInRectangle::MiddleMiddle)?;
    let mloc = loc_from_lat_lon(lat, lon, false, loc.chars().count() as i32, false)?;
    Ok(mloc.as_deref() == Some(loc.as_str())
        && ((mlat - lat).abs() > 0.00001 || (mlon - lon) > 0.00001))
}
