use as_aircraft::units::{ft_m, km_mi, kmh_kts, kts_kmh, m_ft, mi_km};
use as_aircraft::{HistJson, PlaneCategory, PlaneInfo, PlaneJson};
use chrono::{TimeZone, Utc};

#[test]
fn finding_16_units_use_si_factors() {
    assert!((ft_m(3280.84) - 1000.0).abs() < 1e-9);
    assert!((m_ft(1000.0) - 3280.84).abs() < 1e-9);
    assert!((kts_kmh(100.0) - 185.2).abs() < 1e-9);
    assert!((kmh_kts(185.2) - 100.0).abs() < 1e-9);
    assert!((km_mi(10.0) - 6.213_711_922_373_339_5).abs() < 1e-12);
    assert!((mi_km(1.0) - 1.609_344).abs() < 1e-12);
}

#[test]
fn category_parse_and_short() {
    assert_eq!(PlaneCategory::parse("Heavy"), PlaneCategory::Heavy);
    assert_eq!(PlaneCategory::parse("heavy"), PlaneCategory::None);
    assert_eq!(PlaneCategory::parse("xyz"), PlaneCategory::None);
    assert_eq!(PlaneCategory::Superheavy.short(), "S");
    assert_eq!(PlaneCategory::Medium.as_str(), "Medium");
    assert_eq!(PlaneCategory::from_i32(3), Some(PlaneCategory::Heavy));
    assert_eq!(PlaneCategory::from_i32(5), None);
    assert_eq!(PlaneCategory::Light as i32, 1);
}

#[test]
fn plane_json_roundtrip_pascal_case() {
    let src = serde_json::json!({"Hex":"3C6444","Call":"DLH1A","Lat":50.1,"Lon":8.2,"Alt":36000.0,"Track":90.0,"Speed":450.0,"Type":"A320","Category":2,"Manufacturer":"Airbus","Model":"A320","Reg":"D-AIZZ","Time":1700000000});
    let p: PlaneJson = serde_json::from_value(src.clone()).unwrap();
    assert_eq!(p.hex.as_deref(), Some("3C6444"));
    assert_eq!(p.time, Some(1_700_000_000));
    assert_eq!(serde_json::to_value(&p).unwrap(), src);
    let empty: PlaneJson = serde_json::from_str("{}").unwrap();
    assert_eq!(empty.lat, None);
}

#[test]
fn hist_json_field_names() {
    let h: HistJson = serde_json::from_str(r#"{"id":"x","latitude":1.0,"longitude":2.0,"track":3.0,"altitude":4.0,"speed":5.0,"icon":0,"status":0,"timestamp":7,"onGround":true,"callsign":"ABC","squawk":null,"dataSource":0,"schedule":{"std":"a"},"ems":null}"#).unwrap();
    assert!(h.on_ground);
    assert_eq!(h.data_source, 0);
    assert_eq!(h.schedule.unwrap().std.as_deref(), Some("a"));
}

#[test]
fn new_sets_old_time_none() {
    let t = Utc.with_ymd_and_hms(2023, 1, 1, 12, 0, 0).unwrap();
    let p = PlaneInfo::new(
        t,
        "DLH1A",
        "D-AIZZ",
        "3C6444",
        50.0,
        8.0,
        90.0,
        32808.4,
        400.0,
        "A320",
        "Airbus",
        "A320",
        PlaneCategory::Medium,
    );
    assert_eq!(p.old_time, None);
    assert_eq!(p.time, t);
    assert!((p.alt_m() - 10000.0).abs() < 1e-9);
    assert!((p.speed_kmh() - 740.8).abs() < 1e-9);
    assert_eq!(p.int_qrb, f64::MAX);
    assert_eq!(p.signal_strength, f64::MIN);
    assert_eq!(p.int_point, None);
    assert_eq!(p.potential, 0);
}
