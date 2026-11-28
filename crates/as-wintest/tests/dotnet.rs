use as_wintest::dotnet::{clamp_elevation, fixed, whole_degrees, wrap_azimuth, zero3};

#[test]
fn fixed_rounds_half_away_from_zero() {
    assert_eq!(fixed(0.25, 1), "0.3");
    assert_eq!(fixed(0.35, 1), "0.4");
    assert_eq!(fixed(-0.25, 1), "-0.3");
}

#[test]
fn fixed_uses_15_digit_step() {
    assert_eq!(fixed(123.45678901499996, 8), "123.45678902");
}

#[test]
fn fixed_padding_and_specials() {
    assert_eq!(fixed(5.0, 8), "5.00000000");
    assert_eq!(fixed(-0.04, 1), "-0.0");
    assert_eq!(fixed(9.96, 1), "10.0");
    assert_eq!(fixed(0.0, 0), "0");
    assert_eq!(fixed(f64::NAN, 1), "NaN");
    assert_eq!(fixed(f64::INFINITY, 1), "Infinity");
    assert_eq!(fixed(f64::NEG_INFINITY, 1), "-Infinity");
}

#[test]
fn zero3_cases() {
    assert_eq!(zero3(122.5), "123");
    assert_eq!(zero3(359.6), "360");
    assert_eq!(zero3(0.4), "000");
    assert_eq!(zero3(-5.0), "-005");
    assert_eq!(zero3(-0.4), "-000");
    assert_eq!(zero3(1234.0), "1234");
}

/// `f64::MAX` is finite but its 15-digit decimal overflows `f64` on the parse back, which must
/// not leak `Infinity`/`NaN` onto the wire (#202, #236).
#[test]
fn whole_degrees_rejects_out_of_range() {
    assert_eq!(whole_degrees(f64::MAX), 0.0);
    assert_eq!(whole_degrees(f64::MIN), 0.0);
    assert_eq!(whole_degrees(f64::NAN), 0.0);
    assert_eq!(whole_degrees(f64::INFINITY), 0.0);
    assert_eq!(wrap_azimuth(f64::MAX), 0.0);
    assert_eq!(wrap_azimuth(f64::NAN), 0.0);
    assert_eq!(clamp_elevation(f64::MAX), 0.0);
    // a normal value still rounds and wraps
    assert_eq!(wrap_azimuth(359.6), 0.0);
    assert_eq!(clamp_elevation(91.4), 90.0);
}
