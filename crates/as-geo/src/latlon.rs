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

    /// Closed rectangle membership. Reversed longitude bounds cross the antimeridian.
    /// Both representations of the antimeridian refer to the same edge.
    pub fn contains(self, lat: f64, lon: f64) -> bool {
        let contains_lon = |lon| {
            if self.min_lon <= self.max_lon {
                (self.min_lon..=self.max_lon).contains(&lon)
            } else {
                (self.min_lon..=180.0).contains(&lon) || (-180.0..=self.max_lon).contains(&lon)
            }
        };
        (-180.0..=180.0).contains(&lon)
            && (self.min_lat..=self.max_lat).contains(&lat)
            && (contains_lon(wrap_longitude(lon)) || (lon.abs() == 180.0 && contains_lon(180.0)))
    }

    /// Bounding rectangle on the shorter longitude arc, with padding in degrees.
    /// Returns `None` for an empty input, invalid coordinates, or a span wider than 180°.
    pub fn bounding_points(points: impl IntoIterator<Item = GPoint>, padding: f64) -> Option<Self> {
        let mut lat_min = 90.0_f64;
        let mut lat_max = -90.0_f64;
        let mut lon_min = 180.0_f64;
        let mut lon_max = -180.0_f64;
        let mut east_min = 360.0_f64;
        let mut east_max = 0.0_f64;
        for p in points {
            if !crate::maidenhead::check_lat_lon(p.lat, p.lon) {
                return None;
            }
            let lon = wrap_longitude(p.lon);
            let east = if lon < 0.0 { lon + 360.0 } else { lon };
            lat_min = lat_min.min(p.lat);
            lat_max = lat_max.max(p.lat);
            lon_min = lon_min.min(lon);
            lon_max = lon_max.max(lon);
            east_min = east_min.min(east);
            east_max = east_max.max(east);
        }
        if lat_min > lat_max {
            return None;
        }
        if lon_max - lon_min > east_max - east_min {
            lon_min = east_min;
            lon_max = east_max;
        }
        if lon_max - lon_min > 180.0 {
            return None;
        }
        Some(Self::new(
            (lat_min - padding).max(-90.0),
            wrap_longitude(lon_min - padding),
            (lat_max + padding).min(90.0),
            wrap_longitude(lon_max + padding),
        ))
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
    let a = a.clamp(0.0, 1.0);
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

/// Canonical longitude for an angle within one turn of the valid range.
pub fn wrap_longitude(lon: f64) -> f64 {
    if lon >= 180.0 {
        lon - 360.0
    } else if lon < -180.0 {
        lon + 360.0
    } else {
        lon
    }
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
    GPoint::new(latm, wrap_longitude(lonm))
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
    GPoint::new(lat / PI * 180.0, wrap_longitude(lon / PI * 180.0))
}

// Stable initial bearing for nearly collinear intersection inputs. Use the
// latitude difference directly to avoid subtracting similar products.
fn intersection_bearing(lat1: f64, lat2: f64, delta_lon: f64) -> (f64, f64) {
    let lon = rad(delta_lon);
    let east = if delta_lon % 180.0 == 0.0 {
        0.0
    } else {
        rad(lat2).cos() * lon.sin()
    };
    let north = rad(lat2 - lat1).sin()
        + 2.0 * rad(lat1).sin() * rad(lat2).cos() * (lon / 2.0).sin().powi(2);
    let norm = east.hypot(north);
    (east / norm, north / norm)
}

fn track_components(bearing: f64) -> (f64, f64) {
    match bearing.rem_euclid(360.0) {
        0.0 => (0.0, 1.0),
        90.0 => (1.0, 0.0),
        180.0 => (0.0, -1.0),
        270.0 => (-1.0, 0.0),
        _ => rad(bearing).sin_cos(),
    }
}

fn relative_components((sa, ca): (f64, f64), (sb, cb): (f64, f64)) -> (f64, f64) {
    (sa * cb - ca * sb, ca * cb + sa * sb)
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
        .clamp(-1.0, 1.0)
        .asin();
    if dist12 == 0.0 {
        return None;
    }
    let sin_dist = dist12.sin();
    let direct12 = intersection_bearing(mylat, lat, d_lon);
    let direct21 = intersection_bearing(lat, mylat, -d_lon);
    let (sin1, cos1) = relative_components(track_components(mybearing), direct12);
    let (sin2, cos2) = relative_components(direct21, track_components(bearing));
    let collinear = |sin: f64| sin.abs() < 1e-12 || (sin * sin_dist).abs() < 1e-14;
    match (collinear(sin1), collinear(sin2)) {
        (true, true) => return None,
        (true, false) => return Some(GPoint::new(lat, wrap_longitude(lon))),
        (false, true) => return Some(GPoint::new(mylat, wrap_longitude(mylon))),
        (false, false) => {}
    }

    // The acos-based bearings lose precision as their east components approach
    // zero. Use those components directly and factor the distance formula
    // when either end of the baseline is close to a meridian.
    let ill_conditioned = direct12.0.abs().min(direct21.0.abs()) < 1e-3;
    let dist13 = if ill_conditioned {
        // Cancel sin(alpha1) from the spherical formula before evaluating it.
        // The original denominator subtracts nearly equal terms at the meridian.
        if sin1 * sin2 < 0.0 {
            return None;
        }
        let sign = sin1.signum();
        (sign * sin_dist * sin2).atan2(sign * (sin1 * cos2 + cos1 * sin2 * dist12.cos()))
    } else {
        let brng_a = ((rad(lat).sin() - rad(mylat).sin() * dist12.cos())
            / (sin_dist * rad(mylat).cos()))
        .clamp(-1.0, 1.0)
        .acos();
        let brng_b = ((rad(mylat).sin() - rad(lat).sin() * dist12.cos())
            / (sin_dist * rad(lat).cos()))
        .clamp(-1.0, 1.0)
        .acos();
        let (brng12, brng21) = if (lon / 180.0 * PI - mylon / 180.0 * PI).sin() > 0.0 {
            (brng_a, 2.0 * PI - brng_b)
        } else {
            (2.0 * PI - brng_a, brng_b)
        };
        let alpha1 = (brng13 - brng12 + PI) % (2.0 * PI) - PI;
        let alpha2 = (brng21 - brng23 + PI) % (2.0 * PI) - PI;
        let sin_alpha1 = alpha1.sin();
        let sin_alpha2 = alpha2.sin();
        if sin_alpha1 * sin_alpha2 < 0.0 {
            return None;
        }
        let alpha3 = (-alpha1.cos() * alpha2.cos() + sin_alpha1 * sin_alpha2 * dist12.cos())
            .clamp(-1.0, 1.0)
            .acos();
        (sin_dist * sin_alpha1 * sin_alpha2).atan2(alpha2.cos() + alpha1.cos() * alpha3.cos())
    };
    let lat3 = (rad(mylat).sin() * dist13.cos() + rad(mylat).cos() * dist13.sin() * brng13.cos())
        .clamp(-1.0, 1.0)
        .asin();
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
