use as_adsb::crc::parity;
use as_adsb::decoder::{AltitudeProvenance, Decoder};
use as_adsb::{decode, Msg};
use chrono::{Duration, TimeZone, Utc};
use std::sync::{Arc, Mutex};

fn t0() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 23, 12, 0, 0).unwrap()
}

fn at(secs: i64, nanos: i64) -> chrono::DateTime<Utc> {
    t0() + Duration::seconds(secs) + Duration::nanoseconds(nanos)
}

fn frame(icao: [u8; 3], me: [u8; 7]) -> String {
    let mut b = vec![0x8D];
    b.extend_from_slice(&icao);
    b.extend_from_slice(&me);
    let p = parity(&b);
    b.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    format!(
        "*{};",
        b.iter().map(|x| format!("{x:02X}")).collect::<String>()
    )
}

fn decode_msg(hex: &str) -> Msg {
    decode(hex).unwrap()
}

#[test]
fn finding_42_pair_window_accepts_10s_and_rejects_10s_plus_1ns() {
    let clock = Arc::new(Mutex::new(t0()));
    let ck = clock.clone();
    let mut dec = Decoder::with_clock(5, Box::new(move || *ck.lock().unwrap()));
    let even = "*8D40621D58C382D690C8AC2863A7;";
    let odd = "*8D40621D58C386435CC412692AD6;";

    dec.decode_message(even, at(0, 0), None).unwrap();
    dec.decode_message(odd, at(10, 0), None).unwrap();
    let s = dec.states();
    assert_eq!(s[0].num_pos, 1);
    assert_eq!(s[0].last_pos, Some(at(10, 0)));

    let mut dec = Decoder::with_clock(5, Box::new(t0));
    dec.decode_message(even, at(0, 0), None).unwrap();
    dec.decode_message(odd, at(10, 1), None).unwrap();
    let s = dec.states();
    assert_eq!(s[0].num_pos, 0);
    assert!(s[0].last_pos.is_none());
}

#[test]
fn finding_45_delayed_identity_emits_once_when_position_already_known() {
    let clock = Arc::new(Mutex::new(t0()));
    let ck = clock.clone();
    let mut dec = Decoder::with_clock(5, Box::new(move || *ck.lock().unwrap()));
    dec.decode_message("*8D40621D58C382D690C8AC2863A7;", at(1, 0), None)
        .unwrap();
    dec.decode_message("*8D40621D58C386435CC412692AD6;", at(2, 0), None)
        .unwrap();
    dec.decode_message("*8D40621D994409940838174550B1;", at(2, 0), None)
        .unwrap();
    *clock.lock().unwrap() = at(3, 0);
    assert!(dec.planes().is_empty());

    dec.decode_message("*8D40621D202CC371C32CE02E065C;", at(3, 0), None)
        .unwrap();
    let out = dec.planes();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].timestamp, at(2, 0));
    assert!(dec.planes().is_empty());
}

#[test]
fn finding_43_expiry_removes_stale_incomplete_aircraft_independently() {
    let clock = Arc::new(Mutex::new(t0()));
    let ck = clock.clone();
    let mut dec = Decoder::with_clock(1, Box::new(move || *ck.lock().unwrap()));
    dec.decode_message("*8D40621D202CC371C32CE02E065C;", at(0, 0), None)
        .unwrap();
    dec.decode_message("*8D4840D6202CC371C32CE0576098;", at(0, 0), None)
        .unwrap();
    dec.decode_message("*8D4840D658C382D690C8AC510563;", at(1, 0), None)
        .unwrap();
    dec.decode_message("*8D4840D658C38612345678DD9B2D;", at(2, 0), None)
        .unwrap();
    assert_eq!(dec.states().len(), 2);

    *clock.lock().unwrap() = at(61, 0);
    dec.decode_message("*8D4840D69A4009140838178DB858;", at(61, 0), None)
        .unwrap();
    let s = dec.states();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].icao24, "4840D6");
}

#[test]
fn finding_363_tc20_decodes_direct_geometric_height() {
    let tc20 = decode_msg("8D40621DA0C382D690C8AC000000");
    match tc20 {
        Msg::AirbornePosition(p) => assert_eq!(p.alt_ft, Some(38000)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn finding_35_raw_zero_unavailable_and_raw_one_zero_valid() {
    let icao = [0x3C, 0x00, 0x01];
    let me0 = [0x99, 0x44, 0x09, 0x94, 0x08, 0x00, 0x00];
    let mut me1 = me0;
    me1[5] = 0x04;
    me1[6] = 0x01;

    let h0 = frame(icao, me0);
    let h1 = frame(icao, me1);
    let got0 = decode_msg(&h0[1..h0.len() - 1]);
    let got1 = decode_msg(&h1[1..h1.len() - 1]);
    match (got0, got1) {
        (Msg::VelocityOverGround(a), Msg::VelocityOverGround(b)) => {
            assert!(!a.vrate_valid);
            assert!(!a.geo_minus_baro_valid);
            assert!(b.vrate_valid);
            assert_eq!(b.vrate_ms, 0);
            assert!(b.geo_minus_baro_valid);
            assert_eq!(b.geo_minus_baro_ft, 0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn finding_366_ttl_is_inclusive_and_pressure_is_marked_estimate() {
    let clock = Arc::new(Mutex::new(t0()));
    let ck = clock.clone();
    let mut dec = Decoder::with_clock(1, Box::new(move || *ck.lock().unwrap()));
    dec.decode_message("*8D40621D202CC371C32CE02E065C;", at(0, 0), None)
        .unwrap();
    dec.decode_message("*8D40621D58C382D690C8AC2863A7;", at(0, 0), None)
        .unwrap();
    dec.decode_message("*8D40621D58C386435CC412692AD6;", at(0, 0), None)
        .unwrap();
    dec.decode_message("*8D40621D994409940838174550B1;", at(0, 0), None)
        .unwrap();
    *clock.lock().unwrap() = at(60, 0);
    let out = dec.planes();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].components_valid_until, at(60, 0));
    assert_eq!(
        out[0].altitude_provenance,
        AltitudeProvenance::PressureEstimate
    );

    let clock = Arc::new(Mutex::new(t0()));
    let ck = clock.clone();
    let mut dec = Decoder::with_clock(1, Box::new(move || *ck.lock().unwrap()));
    dec.decode_message("*8D40621D202CC371C32CE02E065C;", at(0, 0), None)
        .unwrap();
    dec.decode_message("*8D40621D58C382D690C8AC2863A7;", at(0, 0), None)
        .unwrap();
    dec.decode_message("*8D40621D58C386435CC412692AD6;", at(0, 0), None)
        .unwrap();
    dec.decode_message("*8D40621D994409940838174550B1;", at(0, 0), None)
        .unwrap();
    *clock.lock().unwrap() = at(60, 1);
    assert!(dec.planes().is_empty());
}

#[test]
fn finding_42_event_time_pairing_survives_delay_and_restarts_on_receipt_timing() {
    let even = frame(
        [0x3C, 0x00, 0x01],
        [0x58, 0xC3, 0x82, 0xD6, 0x90, 0xC8, 0xAC],
    );
    let odd = frame(
        [0x3C, 0x00, 0x01],
        [0x58, 0xC3, 0x86, 0x43, 0x5C, 0xC4, 0x12],
    );
    for ((first, first_event), (second, second_event)) in [
        ((&even, at(0, 0)), (&odd, at(10, 0))),
        ((&odd, at(10, 0)), (&even, at(0, 0))),
    ] {
        let mut dec = Decoder::with_clock(5, Box::new(|| at(40, 0)));
        dec.decode_message(first, at(20, 0), Some(first_event))
            .unwrap();
        dec.decode_message(second, at(40, 0), Some(second_event))
            .unwrap();
        let state = &dec.states()[0];
        assert_eq!(state.last_pos, Some(at(10, 0)));
        assert!((state.lat - 52.26578).abs() < 0.0001);
        assert!((state.lon - 3.93891).abs() < 0.0001);

        // The receipt-timed even word must not reuse the event-timed odd word or fix.
        dec.decode_message(&even, at(41, 0), None).unwrap();
        let state = &dec.states()[0];
        assert_eq!(state.last_even, Some(at(41, 0)));
        assert_eq!(state.last_odd, None);
        assert_eq!(state.last_pos, None);
    }
}

#[test]
fn finding_43_delayed_events_expire_by_receipt_age() {
    let clock = Arc::new(Mutex::new(at(120, 0)));
    let poll_clock = clock.clone();
    let mut dec = Decoder::with_clock(1, Box::new(move || *poll_clock.lock().unwrap()));
    let even = frame(
        [0x3C, 0x00, 0x01],
        [0x58, 0xC3, 0x82, 0xD6, 0x90, 0xC8, 0xAC],
    );
    dec.decode_message(&even, at(120, 0), Some(at(0, 0)))
        .unwrap();
    for poll_at in [at(119, 0), at(180, 0)] {
        *clock.lock().unwrap() = poll_at;
        assert!(dec.planes().is_empty());
        assert_eq!(dec.states()[0].last_even, Some(at(0, 0)));
    }

    *clock.lock().unwrap() = at(180, 1);
    assert!(dec.planes().is_empty());
    assert!(dec.is_empty());
}

#[test]
fn finding_366_newest_unavailable_motion_cannot_be_restored_by_older_word() {
    let addr = [0x3C, 0, 1];
    let velocity = frame(addr, [0x99, 0x44, 0x09, 0x94, 0x08, 0x38, 0x17]);
    let missing_velocity = frame(addr, [0x99, 0, 0, 0, 0, 0, 0]);
    let mut dec = Decoder::with_clock(1, Box::new(|| at(20, 0)));
    dec.decode_message(&missing_velocity, at(15, 0), Some(at(10, 0)))
        .unwrap();
    dec.decode_message(&velocity, at(16, 0), Some(at(9, 0)))
        .unwrap();
    let state = &dec.states()[0];
    assert!(state.speed_kts.is_none());
    assert!(state.heading.is_none());
    assert!(state.geo_minus_baro_ft.is_none());

    dec.decode_message(&velocity, at(17, 0), Some(at(11, 0)))
        .unwrap();
    let state = &dec.states()[0];
    assert!(state.speed_kts.is_some());
    assert!(state.heading.is_some());
    assert!(state.geo_minus_baro_ft.is_some());
}
