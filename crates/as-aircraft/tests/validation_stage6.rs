use as_aircraft::check::{self, Lookup};
use as_aircraft::convert;
use as_aircraft::{PlaneCategory, PlaneInfo};
use chrono::{TimeZone, Utc};

struct Stage6Lookup;

impl Lookup for Stage6Lookup {
    fn airline_by_icao(&self, s: &str) -> bool {
        ["ZZZ"].contains(&s)
    }
    fn airline_by_iata(&self, s: &str) -> bool {
        ["Z8"].contains(&s)
    }
    fn registration_by_reg(&self, s: &str) -> bool {
        check::find_reg_prefix(s, |p| Ok::<_, ()>(["HB"].contains(&p)))
            .ok()
            .flatten()
            .is_some()
    }
    fn airport_by_icao(&self, s: &str) -> bool {
        ["AAAA", "BBBB"].contains(&s)
    }
    fn type_by_icao(&self, _: &str) -> bool {
        true
    }
    fn registration_min_len(&self) -> usize {
        1
    }
    fn type_icao_min_len(&self) -> usize {
        4
    }
}

#[test]
fn finding_19_call_and_registration_validation_share_discriminator() {
    let l = Stage6Lookup;
    assert!(check::call("Z8123", &l));
    assert!(check::call("ZZZ123", &l));
    assert_eq!(
        convert::to_call("Z8123", true, &l).as_deref(),
        Some("Z8123")
    );
    assert_eq!(
        convert::to_call("ZZZ123", true, &l).as_deref(),
        Some("ZZZ123")
    );

    assert!(!check::reg("HBTEST", &l));
    assert!(check::reg("HB-TEST", &l));
    assert_eq!(convert::to_reg("HBTEST", &l), None);
    assert_eq!(convert::to_reg("HB-TEST", &l).as_deref(), Some("HB-TEST"));
}

#[test]
fn finding_21_route_split_uses_full_airport_codes() {
    let l = Stage6Lookup;
    assert_eq!(
        convert::to_route("AAAA-BBBB", &l).as_deref(),
        Some("AAAA-BBBB")
    );
    assert_eq!(convert::to_route("AAAX-BBBB", &l), None);
}

fn valid_plane() -> PlaneInfo {
    PlaneInfo::new(
        Utc.with_ymd_and_hms(2023, 11, 14, 22, 13, 20).unwrap(),
        "ZZZ123",
        "HB-TEST",
        "3C0001",
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    )
}

#[test]
fn finding_367_plane_rejects_nan_required_measurements() {
    let base = valid_plane();
    assert!(check::plane(&base));

    for mut p in [
        {
            let mut p = base.clone();
            p.lat = f64::NAN;
            p
        },
        {
            let mut p = base.clone();
            p.lon = f64::NAN;
            p
        },
        {
            let mut p = base.clone();
            p.alt_ft = f64::NAN;
            p
        },
        {
            let mut p = base.clone();
            p.track = f64::NAN;
            p
        },
        {
            let mut p = base.clone();
            p.speed_kts = f64::NAN;
            p
        },
    ] {
        assert!(!check::plane(&p));
        p.lat = 90.0;
        p.lon = 180.0;
        p.alt_ft = 100000.0;
        p.track = 360.0;
        p.speed_kts = 5000.0;
        assert!(check::plane(&p));
    }
}
