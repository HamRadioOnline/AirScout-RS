//! Independent spherical intersection reference for the polar and on-track fixtures.
//! Uses the great-circle normals from Gade (2010), J. Navigation 63(3), 395–417.

type Vector = [f64; 3];

fn cross(a: Vector, b: Vector) -> Vector {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: Vector, b: Vector) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}

fn track([lat, lon, bearing]: [f64; 3]) -> (Vector, Vector, Vector) {
    let (s_lat, c_lat) = lat.to_radians().sin_cos();
    let (s_lon, c_lon) = lon.to_radians().sin_cos();
    let (east, north) = bearing.to_radians().sin_cos();
    let pos = [c_lat * c_lon, c_lat * s_lon, s_lat];
    let tangent = [
        -north * s_lat * c_lon - east * s_lon,
        -north * s_lat * s_lon + east * c_lon,
        north * c_lat,
    ];
    (pos, tangent, cross(pos, tangent))
}

/// Intersection of two directed spherical arcs. A station on the other
/// great circle is the intersection. Coincident great circles are ambiguous.
pub fn directed_intersection(a: [f64; 3], b: [f64; 3]) -> Option<(f64, f64)> {
    let (pos_a, tan_a, normal_a) = track(a);
    let (pos_b, tan_b, normal_b) = track(b);
    let b_on_a = dot(pos_b, normal_a).abs() < 1e-14;
    let a_on_b = dot(pos_a, normal_b).abs() < 1e-14;
    match (b_on_a, a_on_b) {
        (true, true) => return None,
        (true, false) => return Some((b[0], b[1])),
        (false, true) => return Some((a[0], a[1])),
        (false, false) => {}
    }
    let x = cross(normal_a, normal_b);
    let x = [x, x.map(|i| -i)]
        .into_iter()
        .find(|&p| dot(p, tan_a) > 0.0 && dot(p, tan_b) > 0.0)?;
    Some((
        x[2].atan2(x[0].hypot(x[1])).to_degrees(),
        x[1].atan2(x[0]).to_degrees(),
    ))
}
