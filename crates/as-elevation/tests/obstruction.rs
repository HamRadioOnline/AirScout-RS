use as_elevation::LocalObstruction;

#[test]
fn finding_8_obstruction_uses_centred_sectors() {
    let mut d = [0.0; 360];
    let mut h = [0.0; 360];
    d[359] = 100.0;
    h[359] = 60.0;
    d[0] = 100.0;
    h[0] = 1000.0;
    let o = LocalObstruction::new(50.0, 10.0, Some(d), Some(h));
    for (bearing, ratio) in [
        (359.6, 9.9_f64),
        (359.5, 9.9),
        (-0.5, 9.9),
        (359.4, 0.5),
        (0.4, 9.9),
    ] {
        assert!((o.obstruction(10.0, bearing) - ratio.atan()).abs() < 1e-15);
    }
}

#[test]
fn finding_8_zero_distance_has_no_obstruction() {
    for height in [None, Some([5000.0; 360])] {
        let o = LocalObstruction::new(50.0, 10.0, None, height);
        for antenna in [0.0, 10.0] {
            assert_eq!(o.obstruction(antenna, 45.0), f64::MIN);
        }
    }
}

#[test]
fn finding_4_missing_height_keeps_distances() {
    let o = LocalObstruction::new(50.0, 10.0, Some([5.0; 360]), None);
    assert_eq!(o.distance, [5.0; 360]);
    assert_eq!(o.height, [0.0; 360]);
}
