use as_elevation::LocalObstruction;

#[test]
fn obstruction_truncates_bearing_index() {
    let mut d = [0.0; 360];
    let mut h = [0.0; 360];
    d[359] = 100.0;
    h[359] = 60.0;
    d[0] = 100.0;
    h[0] = 1000.0;
    let o = LocalObstruction::new(50.0, 10.0, Some(d), Some(h));
    // (int)359.6 == 359 (finding #8), not 0
    let eps = o.obstruction(10.0, 359.6);
    assert!((eps - (50.0f64 / 100.0).atan()).abs() < 1e-15, "{eps}");
}

#[test]
fn obstruction_zero_distance_is_inf_like_csharp() {
    let o = LocalObstruction::new(50.0, 10.0, None, None);
    // atan((0 - 10) / 0) = atan(-inf) = -pi/2
    assert_eq!(o.obstruction(10.0, 45.0), -std::f64::consts::FRAC_PI_2);
    // atan(0/0) = NaN
    assert!(o.obstruction(0.0, 45.0).is_nan());
}

#[test]
fn missing_height_wipes_given_distances_like_csharp() {
    // finding #4: with height == null the C# loop zeroes Distance instead of Height
    let d = [5.0; 360];
    let o = LocalObstruction::new(50.0, 10.0, Some(d), None);
    assert!(o.distance.iter().all(|v| *v == 0.0));
    assert!(o.height.iter().all(|v| *v == 0.0));
}
