//! Golden test: LibADSB message decoding against C# (`tests/golden/adsb_msgs.json`).
use as_adsb::cpr::{self, CprMsg};
use as_adsb::{decode, AdsbError, ModeSReply, Msg};
use as_golden::{assert_close, load, Case};
use serde_json::Value;

const TOL: f64 = 1e-9;

fn err_name(e: &AdsbError) -> &'static str {
    match e {
        AdsbError::BadFormat(_) => "BadFormatException",
        AdsbError::BadHex(_) => "FormatException",
        AdsbError::MissingInformation(_) => "MissingInformationException",
        AdsbError::PositionStraddle => "PositionStraddleException",
        AdsbError::IllegalArgument(_) => "IllegalArgumentException",
    }
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("not an int: {v:?}"))
}
fn b(v: &Value) -> bool {
    v.as_bool().unwrap_or_else(|| panic!("not a bool: {v:?}"))
}

fn check_decode(c: &Case) {
    let hex = c.s("hex");
    let r = &c.result;
    let got = decode(&hex);
    if let Some(e) = r.get("error") {
        assert_eq!(err_name(&got.unwrap_err()), e.as_str().unwrap(), "{hex}");
        return;
    }
    let reply = ModeSReply::from_hex(&hex).unwrap();
    assert_eq!(i64::from(reply.df), i(&r["df"]), "{hex}");
    assert_eq!(i64::from(reply.icao24), i(&r["icao24"]), "{hex}");
    assert_eq!(i64::from(reply.parity), i(&r["parity"]), "{hex}");
    assert_eq!(
        i64::from(reply.recalculated_parity()),
        i(&r["recalc_parity"]),
        "{hex}"
    );
    assert_eq!(reply.check_parity(), b(&r["parity_ok"]), "{hex}");
    assert_eq!(reply.is_zero_parity(), b(&r["zero_parity"]), "{hex}");
    let payload: String = reply.payload.iter().map(|x| format!("{x:02X}")).collect();
    assert_eq!(payload, r["payload"].as_str().unwrap(), "{hex}");
    let kind = r["kind"].as_str().unwrap();
    if kind == "mode_s" {
        // ExtendedSquitter overrides Capabilities with ICAO bits (#38); compare only here
        assert_eq!(i64::from(reply.ca), i(&r["ca"]), "{hex}");
    }
    match got.unwrap() {
        Msg::Identification(m) => {
            assert_eq!(kind, "identification", "{hex}");
            assert_eq!(i64::from(m.tc), i(&r["tc"]));
            assert_eq!(i64::from(m.emitter_category), i(&r["emitter_category"]));
            assert_eq!(m.call, r["call"].as_str().unwrap(), "{hex}");
        }
        Msg::AirbornePosition(m) => {
            assert_eq!(kind, "airborne_position", "{hex}");
            assert_eq!(i64::from(m.tc), i(&r["tc"]));
            assert_eq!(m.odd, b(&r["odd"]));
            assert_eq!(i64::from(m.cpr_lat), i(&r["cpr_lat"]));
            assert_eq!(i64::from(m.cpr_lon), i(&r["cpr_lon"]));
            assert_eq!(m.has_position, b(&r["has_position"]));
            assert_eq!(m.alt_ft.is_some(), b(&r["has_altitude"]), "{hex}");
            if let Some(a) = m.alt_ft {
                assert_eq!(i64::from(a), i(&r["altitude"]), "{hex}");
            }
            assert_eq!(
                i64::from(m.surveillance_status),
                i(&r["surveillance_status"])
            );
            assert_eq!(m.nic_suppl_b, b(&r["nic_suppl_b"]));
            assert_eq!(m.utc, b(&r["utc"]));
        }
        Msg::SurfacePosition(m) => {
            assert_eq!(kind, "surface_position", "{hex}");
            assert_eq!(i64::from(m.tc), i(&r["tc"]));
            assert_eq!(m.odd, b(&r["odd"]));
            assert_eq!(i64::from(m.cpr_lat), i(&r["cpr_lat"]));
            assert_eq!(i64::from(m.cpr_lon), i(&r["cpr_lon"]));
            assert_eq!(m.has_position, b(&r["has_position"]));
            assert_eq!(m.has_ground_speed(), b(&r["has_ground_speed"]), "{hex}");
            match (m.ground_speed(), &r["ground_speed"]) {
                (Ok(v), e) if e.is_number() => assert_close(v, e.as_f64().unwrap(), TOL, &hex),
                (Err(e), Value::String(s)) => assert_eq!(err_name(&e), s, "{hex}"),
                (g, e) => panic!("ground speed {g:?} vs {e:?} for {hex}"),
            }
            assert_eq!(m.heading_valid, b(&r["has_heading"]));
            assert_close(m.heading(), r["heading"].as_f64().unwrap(), TOL, &hex);
        }
        Msg::VelocityOverGround(m) => {
            assert_eq!(kind, "velocity_over_ground", "{hex}");
            assert_eq!(m.supersonic, b(&r["supersonic"]));
            assert_eq!(m.velocity_valid, b(&r["has_velocity"]), "{hex}");
            assert_eq!(i64::from(m.speed_kts), i(&r["velocity"]), "{hex}");
            assert_eq!(i64::from(m.ew), i(&r["ew"]), "{hex}");
            assert_eq!(i64::from(m.ns), i(&r["ns"]), "{hex}");
            assert_eq!(i64::from(m.heading_deg), i(&r["heading"]), "{hex}");
            assert_eq!(m.vrate_valid, b(&r["has_vrate"]), "{hex}");
            assert_eq!(i64::from(m.vrate_ms), i(&r["vrate"]), "{hex}");
            assert_eq!(m.geo_minus_baro_valid, b(&r["has_geo_minus_baro"]));
            assert_eq!(
                i64::from(m.geo_minus_baro_ft),
                i(&r["geo_minus_baro"]),
                "{hex}"
            );
        }
        Msg::AirspeedHeading(m) => {
            assert_eq!(kind, "airspeed_heading", "{hex}");
            assert_eq!(m.supersonic, b(&r["supersonic"]));
            assert_eq!(m.airspeed_valid, b(&r["has_airspeed"]));
            assert_eq!(i64::from(m.airspeed_kts), i(&r["airspeed"]), "{hex}");
            assert_eq!(m.true_airspeed, b(&r["true_airspeed"]));
            assert_eq!(m.heading_valid, b(&r["has_heading"]));
            assert_eq!(i64::from(m.heading_deg), i(&r["heading"]), "{hex}");
            assert_eq!(m.vrate_valid, b(&r["has_vrate"]));
            assert_eq!(i64::from(m.vrate_ftmin), i(&r["vrate"]), "{hex}");
            assert_eq!(m.geo_minus_baro_valid, b(&r["has_geo_minus_baro"]));
            assert_eq!(
                i64::from(m.geo_minus_baro_ft),
                i(&r["geo_minus_baro"]),
                "{hex}"
            );
        }
        Msg::Other { df, tc, icao24 } => {
            assert_eq!(i64::from(df), i(&r["df"]));
            assert_eq!(i64::from(icao24), i(&r["icao24"]));
            match tc {
                Some(t) => {
                    assert_eq!(kind, "extended_squitter", "{hex}");
                    assert_eq!(i64::from(t), i(&r["tc"]));
                }
                None => assert_eq!(kind, "mode_s", "{hex}"),
            }
        }
    }
}

fn check_pos(c: &Case, got: Result<(f64, f64), AdsbError>) {
    let r = &c.result;
    match (got, r.get("error")) {
        (Err(e), Some(name)) => assert_eq!(err_name(&e), name.as_str().unwrap(), "{c:?}"),
        (Ok((lat, lon)), None) => {
            assert_close(lat, r["lat"].as_f64().unwrap(), TOL, c);
            assert_close(lon, r["lon"].as_f64().unwrap(), TOL, c);
        }
        (g, _) => panic!("{g:?} vs {r:?} for {c:?}"),
    }
}

fn global_case<T: CprMsg + Clone>(c: &Case, get: fn(Msg) -> T) {
    let newer = get(decode(&c.s("newer")).unwrap());
    let other = get(decode(&c.s("other")).unwrap());
    let got = if newer.odd() {
        cpr::global(&other, &newer, true)
    } else {
        cpr::global(&newer, &other, false)
    };
    check_pos(c, got);
}

fn airborne(m: Msg) -> as_adsb::AirbornePosition {
    match m {
        Msg::AirbornePosition(p) => p,
        o => panic!("not airborne: {o:?}"),
    }
}
fn surface(m: Msg) -> as_adsb::SurfacePosition {
    match m {
        Msg::SurfacePosition(p) => p,
        o => panic!("not surface: {o:?}"),
    }
}

#[test]
fn adsb_golden() {
    let g = load("adsb_msgs.json");
    let mut n = 0;
    for c in &g.cases {
        match c.func.as_str() {
            "decode" => check_decode(c),
            "nl" => assert_eq!(
                f64::from(cpr::nl(c.f("lat"))),
                c.result.as_f64().unwrap(),
                "{c:?}"
            ),
            "global_airborne" => global_case(c, airborne),
            "global_surface" => global_case(c, surface),
            "local_airborne" => check_pos(
                c,
                cpr::local(
                    &airborne(decode(&c.s("hex")).unwrap()),
                    c.f("ref_lat"),
                    c.f("ref_lon"),
                ),
            ),
            "local_surface" => check_pos(
                c,
                cpr::local(
                    &surface(decode(&c.s("hex")).unwrap()),
                    c.f("ref_lat"),
                    c.f("ref_lon"),
                ),
            ),
            f => panic!("unknown fn {f}"),
        }
        n += 1;
    }
    assert_eq!(n, 202);
}

#[test]
fn known_vectors() {
    match decode("8D4840D6202CC371C32CE0576098").unwrap() {
        Msg::Identification(m) => assert_eq!(m.call.trim_end(), "KLM1023"),
        o => panic!("{o:?}"),
    }
    let e = airborne(decode("8D40621D58C382D690C8AC2863A7").unwrap());
    let o = airborne(decode("8D40621D58C386435CC412692AD6").unwrap());
    assert_eq!(e.alt_ft, Some(38000));
    let (lat, lon) = cpr::global(&e, &o, false).unwrap();
    assert!((lat - 52.2572).abs() < 1e-3 && (lon - 3.9194).abs() < 1e-3);
}
