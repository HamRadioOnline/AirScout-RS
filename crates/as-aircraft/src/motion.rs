use crate::units::kts_kmh;
use chrono::Duration;

/// Seconds between two times (C# `TimeSpan.TotalSeconds`).
pub(crate) fn secs(d: Duration) -> f64 {
    d.num_nanoseconds()
        .map(|n| n as f64 / 1e9)
        .unwrap_or(d.num_milliseconds() as f64 / 1e3)
}

/// Normalize heading to `[0, 360)`.
pub(crate) fn heading_norm(track: f64) -> f64 {
    track.rem_euclid(360.0)
}

/// Signed shortest heading delta `from -> to` in `[-180, 180)`.
pub(crate) fn heading_delta(from: f64, to: f64) -> f64 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

/// Circular heading separation in degrees.
pub(crate) fn heading_separation(a: f64, b: f64) -> f64 {
    heading_delta(a, b).abs()
}

/// Track extrapolation along the shortest turn.
pub(crate) fn extrapolated_track(current: f64, previous: f64, dt_old: f64, dt_new: f64) -> f64 {
    let slope = heading_delta(previous, current) / dt_old;
    heading_norm(current + slope * dt_new)
}

/// Distance of a linear speed ramp over `dt_sec`: mean speed times elapsed time.
pub(crate) fn ramp_distance_km(start_kts: f64, end_kts: f64, dt_sec: f64) -> f64 {
    kts_kmh((start_kts + end_kts) * 0.5) * dt_sec / 3600.0
}

type Vector = [f64; 3];

fn dot(a: Vector, b: Vector) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}

fn cross(a: Vector, b: Vector) -> Vector {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: Vector) -> f64 {
    dot(v, v).sqrt()
}

fn normalize(v: Vector) -> Vector {
    let n = norm(v);
    [v[0] / n, v[1] / n, v[2] / n]
}

fn rotate(v: Vector, axis: Vector, angle: f64) -> Vector {
    let (s, c) = angle.sin_cos();
    let axv = cross(axis, v);
    let adv = dot(axis, v);
    [
        v[0] * c + axv[0] * s + axis[0] * adv * (1.0 - c),
        v[1] * c + axv[1] * s + axis[1] * adv * (1.0 - c),
        v[2] * c + axv[2] * s + axis[2] * adv * (1.0 - c),
    ]
}

/// Move a spherical position by great-circle arc length and transport the tangent direction.
pub(crate) fn advance_with_tangent(
    lat: f64,
    lon: f64,
    track: f64,
    distance_km: f64,
) -> (f64, f64, f64) {
    if distance_km == 0.0 {
        return (lat, lon, track);
    }
    let (s_lat, c_lat) = lat.to_radians().sin_cos();
    let (s_lon, c_lon) = lon.to_radians().sin_cos();
    let (east, north) = track.to_radians().sin_cos();
    let pos = [c_lat * c_lon, c_lat * s_lon, s_lat];
    let tangent = normalize([
        -north * s_lat * c_lon - east * s_lon,
        -north * s_lat * s_lon + east * c_lon,
        north * c_lat,
    ]);
    let axis = normalize(cross(pos, tangent));
    let angle = distance_km / as_geo::latlon::EARTH_RADIUS_KM;
    let pos2 = normalize(rotate(pos, axis, angle));
    let tangent2 = normalize(rotate(tangent, axis, angle));
    let lat2 = pos2[2].asin();
    let lon2 = pos2[1].atan2(pos2[0]);
    let (s_lat2, c_lat2) = lat2.sin_cos();
    let (s_lon2, c_lon2) = lon2.sin_cos();
    let east2 = [-s_lon2, c_lon2, 0.0];
    let north2 = [-s_lat2 * c_lon2, -s_lat2 * s_lon2, c_lat2];
    let track2 = heading_norm(
        dot(tangent2, east2)
            .atan2(dot(tangent2, north2))
            .to_degrees(),
    );
    (
        lat2.to_degrees(),
        as_geo::latlon::wrap_longitude(lon2.to_degrees()),
        track2,
    )
}
