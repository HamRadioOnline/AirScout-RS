use as_geo::band::Band;

#[test]
fn names_round_trip_for_every_band() {
    for b in Band::all_except_none_and_all() {
        assert_eq!(Band::parse(b.name()), *b, "{b:?}");
        assert_eq!(Band::parse(&b.name().to_lowercase()), *b, "{b:?}");
    }
}

#[test]
fn parse_special_and_unknown() {
    assert_eq!(Band::parse("144M"), Band::B144M);
    assert_eq!(Band::parse(" all "), Band::All);
    assert_eq!(Band::parse("none"), Band::None);
    assert_eq!(Band::parse("2m"), Band::None, "unknown -> None like C#");
}

#[test]
fn twelve_real_bands_in_order() {
    let b = Band::all_except_none_and_all();
    assert_eq!(b.len(), 12);
    assert_eq!(b[0], Band::B50M);
    assert_eq!(b[11], Band::B76G);
}

#[test]
fn frequencies() {
    assert_eq!(Band::B144M.to_ghz(), 0.144);
    assert_eq!(Band::B10G.to_mhz(), 10368.0);
    assert_eq!(Band::B432M.to_hz(), 432_000_000.0);
}

#[test]
fn next_previous_clamp_like_original() {
    assert_eq!(Band::B144M.next(), Band::B432M);
    assert_eq!(Band::B76G.next(), Band::B76G);
    assert_eq!(
        Band::None.next(),
        Band::B70M,
        "C#: IndexOf<0 -> i=0 -> bands[1]"
    );
    assert_eq!(Band::B50M.previous(), Band::B50M);
    assert_eq!(Band::B432M.previous(), Band::B144M);
}

#[test]
fn from_i32_maps_enum_values() {
    assert_eq!(Band::from_i32(144), Some(Band::B144M));
    assert_eq!(Band::from_i32(76032), Some(Band::B76G));
    assert_eq!(Band::from_i32(0), Some(Band::None));
    assert_eq!(Band::from_i32(999_999_999), Some(Band::All));
    assert_eq!(Band::from_i32(145), None);
}
