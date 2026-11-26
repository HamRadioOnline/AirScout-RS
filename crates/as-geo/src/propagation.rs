//! Port of `ScoutBase.Core/Propagation.cs`: propagation geometry on an equivalent-radius earth.
//!
//! Units as in the original: heights in m, distances and radii in km, angles in rad.

/// Earth angle `alpha` [rad] for a ground distance `dist` [km] on radius `re` [km].
pub fn alpha_from_distance(dist: f64, re: f64) -> f64 {
    dist / re
}

/// Slant range [km] between observer height `h` [m] and object height `big_h` [m] at the
/// distance between two coordinates.
pub fn slant_range_from_heights_latlon(
    h: f64,
    mylat: f64,
    mylon: f64,
    lat: f64,
    lon: f64,
    big_h: f64,
    radius: f64,
) -> f64 {
    slant_range_from_heights(
        h,
        crate::latlon::distance(mylat, mylon, lat, lon),
        big_h,
        radius,
    )
}

/// Slant range [km] between observer `h` [m] and object `big_h` [m] at ground distance `dist` [km].
pub fn slant_range_from_heights(h: f64, dist: f64, big_h: f64, radius: f64) -> f64 {
    let h = h / 1000.0;
    let big_h = big_h / 1000.0;
    let alpha = alpha_from_distance(dist, radius);
    ((radius + big_h) * (radius + big_h) + (radius + h) * (radius + h)
        - (2.0 * (radius + big_h) * (radius + h) * alpha.cos()))
    .sqrt()
}

/// Elevation angle epsilon [rad] under which an observer at `h` [m] sees an object at
/// distance `dist` [km] and height `big_h` [m].
pub fn epsilon_from_heights(h: f64, dist: f64, big_h: f64, radius: f64) -> f64 {
    let d = slant_range_from_heights(h, dist, big_h, radius);
    let h = h / 1000.0;
    let big_h = big_h / 1000.0;
    let a = ((radius + big_h) * (radius + big_h) - (radius + h) * (radius + h) - d * d) / 2.0 / d;
    (a / (radius + h)).asin()
}

/// Angle theta [rad] at which the wavefront from `h` meets the object's horizontal plane.
pub fn theta_from_heights(h: f64, dist: f64, big_h: f64, radius: f64) -> f64 {
    let h = h / 1000.0;
    let big_h = big_h / 1000.0;
    let alpha = alpha_from_distance(dist, radius);
    let d = ((radius + h) * (radius + h) + (radius + big_h) * (radius + big_h)
        - 2.0 * (radius + h) * (radius + big_h) * alpha.cos())
    .sqrt();
    let a = ((radius + big_h) * (radius + big_h) - (radius + h) * (radius + h) - d * d) / 2.0 / d;
    ((a + d) / (radius + big_h)).asin()
}

/// Height [m] of an object at `dist` [km] seen from `h` [m] under elevation `eps` [rad].
pub fn height_from_epsilon(h: f64, dist: f64, eps: f64, radius: f64) -> f64 {
    let h = h / 1000.0;
    let beta = std::f64::consts::PI / 2.0 + eps;
    let gamma = std::f64::consts::PI - alpha_from_distance(dist, radius) - beta;
    ((radius + h) * beta.sin() / gamma.sin() - radius) * 1000.0
}

/// Maximum distance [km] at which an object at `big_h` [m] is still visible from `h` [m]
/// under elevation `eps` [rad].
pub fn distance_from_epsilon(h: f64, big_h: f64, eps: f64, radius: f64) -> f64 {
    let h = h / 1000.0;
    let big_h = big_h / 1000.0;
    let beta = std::f64::consts::PI / 2.0 + eps;
    let gamma = ((radius + h) / (radius + big_h) * beta.sin()).asin();
    let alpha = std::f64::consts::PI - beta - gamma;
    alpha * radius
}

/// Radius [m] of the first Fresnel zone; `qrg` in GHz, distances in km.
pub fn f1_radius(qrg: f64, dist1: f64, dist2: f64) -> f64 {
    let distance = dist1 + dist2;
    if distance == 0.0 {
        return 0.0;
    }
    (300.0 / qrg * dist1 * dist2 / distance).sqrt()
}

/// Radio visibility distance [km] between heights `h1`, `h2` [m] on radius `re` [km].
pub fn visibility_distance(h1: f64, h2: f64, re: f64) -> f64 {
    let h1 = h1 / 1000.0;
    let h2 = h2 / 1000.0;
    (2.0 * re).sqrt() * (h1.sqrt() + h2.sqrt())
}

/// Doppler shift [Hz] for frequency `qrg` [Hz] and radial speed [m/s] (positive = approaching, as
/// the caller passes `(old - new)` slant range; the C# doc says "moving away" but the formula and
/// its caller agree on approaching, see logic finding #179).
pub fn doppler_shift(qrg: f64, speed: f64) -> f64 {
    let c = 300_000_000.0;
    qrg - qrg * c / (c + speed)
}
