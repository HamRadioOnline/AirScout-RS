//! Port of `GetElevationColor` and `DEMColorPalette` (`ElevationDatabase.cs:520-529`,
//! `:2056-2100`): the colour of an elevation on the elevation maps.

/// The palette stops: percentage and colour (`DEMColorPalette()`, `:2059-2066`).
const STOPS: [(f64, [u8; 3]); 8] = [
    (0.00, [0, 97, 71]),
    (1.02, [16, 122, 47]),
    (10.20, [232, 215, 125]),
    (24.49, [161, 67, 0]),
    (34.69, [158, 0, 0]),
    (57.14, [110, 110, 110]),
    (81.63, [255, 255, 255]),
    (100.00, [255, 255, 255]),
];

/// Steps of the lookup table (`Count`).
const COUNT: i32 = 1000;

/// `GetColorFromBase`: interpolation between the two stops around `percentage`. The channel
/// differences are truncated toward zero like the C# `(int)` casts.
fn colour_from_base(percentage: f64) -> [u8; 3] {
    if !(0.0..=100.0).contains(&percentage) {
        return [0, 0, 0];
    }
    for w in STOPS.windows(2) {
        let ((lk, lc), (hk, hc)) = (w[0], w[1]);
        if percentage >= lk && percentage <= hk {
            let p = (percentage - lk) / (hk - lk);
            let ch = |i: usize| {
                let (l, h) = (i32::from(lc[i]), i32::from(hc[i]));
                (l + (p * f64::from(h - l)) as i32) as u8
            };
            return [ch(0), ch(1), ch(2)];
        }
    }
    [0, 0, 0]
}

/// `GetColor` (`ElevationDatabase.cs:2079-2090`): the lookup entry `(int)(percentage / 100 *
/// Count)` of the 1001-entry table. An index outside `0..=1000` (and NaN) is black, as the
/// C# `catch` returns.
pub fn palette_color(percentage: f64) -> [u8; 3] {
    if percentage.is_nan() {
        return [0, 0, 0];
    }
    let i = (percentage / 100.0 * f64::from(COUNT)) as i32;
    if !(0..=COUNT).contains(&i) {
        return [0, 0, 0];
    }
    colour_from_base(f64::from(i) / f64::from(COUNT) * 100.0)
}

/// `GetElevationColor`: `value / 100`, clamped to 0..100, then the palette (1000 steps). A NaN
/// (a flat view normalises to 0/0) gives the colour of 0; the C# lets the lookup throw and
/// returns black.
pub fn elevation_color(value: f64) -> [u8; 3] {
    let e = if value.is_nan() {
        0.0
    } else {
        (value / 100.0).clamp(0.0, 100.0)
    };
    palette_color(e)
}
