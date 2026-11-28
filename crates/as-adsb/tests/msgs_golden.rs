//! Golden test: LibADSB message decoding against C# (`tests/golden/adsb_msgs.json`).
use as_adsb::cpr::{self, CprMsg};
use as_adsb::{decode, AdsbError, ModeSReply, Msg};
use as_golden::deltas::DeltaManifest;
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

fn decode_reference(index: usize, field: &str) -> Option<Value> {
    match (index, field) {
        (30, "ew") => Some(Value::from(-8)),
        (30, "ns") => Some(Value::from(-159)),
        (33, "ew") => Some(Value::from(-1)),
        (33, "heading") => Some(Value::from(179)),
        (38 | 39 | 41, "has_airspeed") => Some(Value::Bool(true)),
        (39, "airspeed") => Some(Value::from(1500)),
        (13, "altitude") => Some(Value::from(38000)),
        (34, "has_vrate") => Some(Value::Bool(false)),
        (35 | 36 | 38 | 39 | 40 | 41, "has_geo_minus_baro") => Some(Value::Bool(false)),
        (35 | 36 | 38 | 39 | 40 | 41, "geo_minus_baro") => Some(Value::from(0)),
        (7 | 11, "has_altitude") => Some(Value::Bool(false)),
        (76 | 101 | 106 | 119, "error") => Some(Value::String("BadFormatException".into())),
        (130 | 131 | 132 | 134, "surface_reference") => Some(Value::String("52.0,5.0".into())),
        _ => None,
    }
}

fn expected_bool(
    deltas: &mut DeltaManifest,
    index: usize,
    field: &str,
    csharp: bool,
    reference: bool,
    findings: &str,
) -> bool {
    deltas
        .expected(
            index,
            field,
            &Value::Bool(csharp),
            Value::Bool(reference),
            0.0,
            findings,
        )
        .as_bool()
        .expect("bool delta")
}

fn expected_i64(
    deltas: &mut DeltaManifest,
    index: usize,
    field: &str,
    csharp: i64,
    reference: i64,
    findings: &str,
) -> i64 {
    deltas
        .expected(
            index,
            field,
            &Value::from(csharp),
            Value::from(reference),
            0.0,
            findings,
        )
        .as_i64()
        .expect("i64 delta")
}

fn expected_error<'a>(
    deltas: &mut DeltaManifest,
    index: usize,
    csharp: Option<&'a str>,
    reference: Option<&'a str>,
    findings: &str,
) -> Option<String> {
    let csharp_v = csharp
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Null);
    let reference_v = reference
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Null);
    match deltas.expected(index, "error", &csharp_v, reference_v, 0.0, findings) {
        Value::Null => None,
        Value::String(s) => Some(s),
        other => panic!("unexpected error delta {other:?}"),
    }
}

fn check_decode(index: usize, c: &Case, deltas: &mut DeltaManifest) {
    let hex = c.s("hex");
    let r = &c.result;
    let got = decode(&hex);
    let csharp_error = r.get("error").and_then(Value::as_str);
    let reference_error = decode_reference(index, "error")
        .and_then(|v| v.as_str().map(str::to_string))
        .or_else(|| csharp_error.map(str::to_string));
    let expected_error = expected_error(
        deltas,
        index,
        csharp_error,
        reference_error.as_deref(),
        "#36",
    );
    if let Some(e) = expected_error {
        assert_eq!(err_name(&got.unwrap_err()), e, "{hex}");
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
            let has_alt = expected_bool(
                deltas,
                index,
                "has_altitude",
                b(&r["has_altitude"]),
                decode_reference(index, "has_altitude")
                    .and_then(|v| v.as_bool())
                    .unwrap_or_else(|| b(&r["has_altitude"])),
                "#362",
            );
            assert_eq!(m.alt_ft.is_some(), has_alt, "{hex}");
            if let Some(a) = m.alt_ft {
                let altitude_ref = decode_reference(index, "altitude")
                    .and_then(|v| v.as_i64())
                    .unwrap_or_else(|| i(&r["altitude"]));
                assert_eq!(
                    i64::from(a),
                    expected_i64(
                        deltas,
                        index,
                        "altitude",
                        i(&r["altitude"]),
                        altitude_ref,
                        "#363"
                    ),
                    "{hex}"
                );
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
            let ew_ref = decode_reference(index, "ew")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| i(&r["ew"]));
            assert_eq!(
                i64::from(m.ew),
                expected_i64(deltas, index, "ew", i(&r["ew"]), ew_ref, "#33"),
                "{hex}"
            );
            let ns_ref = decode_reference(index, "ns")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| i(&r["ns"]));
            assert_eq!(
                i64::from(m.ns),
                expected_i64(deltas, index, "ns", i(&r["ns"]), ns_ref, "#33"),
                "{hex}"
            );
            let heading_ref = decode_reference(index, "heading")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| i(&r["heading"]));
            assert_eq!(
                i64::from(m.heading_deg),
                expected_i64(
                    deltas,
                    index,
                    "heading",
                    i(&r["heading"]),
                    heading_ref,
                    "#33"
                ),
                "{hex}"
            );
            let has_vrate_ref = decode_reference(index, "has_vrate")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| b(&r["has_vrate"]));
            assert_eq!(
                m.vrate_valid,
                expected_bool(
                    deltas,
                    index,
                    "has_vrate",
                    b(&r["has_vrate"]),
                    has_vrate_ref,
                    "#35"
                ),
                "{hex}"
            );
            let vrate_ref = decode_reference(index, "vrate")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| i(&r["vrate"]));
            assert_eq!(
                i64::from(m.vrate_ms),
                expected_i64(deltas, index, "vrate", i(&r["vrate"]), vrate_ref, "#35"),
                "{hex}"
            );
            let has_geo_ref = decode_reference(index, "has_geo_minus_baro")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| b(&r["has_geo_minus_baro"]));
            assert_eq!(
                m.geo_minus_baro_valid,
                expected_bool(
                    deltas,
                    index,
                    "has_geo_minus_baro",
                    b(&r["has_geo_minus_baro"]),
                    has_geo_ref,
                    "#35"
                ),
                "{hex}"
            );
            let geo_ref = decode_reference(index, "geo_minus_baro")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| i(&r["geo_minus_baro"]));
            assert_eq!(
                i64::from(m.geo_minus_baro_ft),
                expected_i64(
                    deltas,
                    index,
                    "geo_minus_baro",
                    i(&r["geo_minus_baro"]),
                    geo_ref,
                    "#35"
                ),
                "{hex}"
            );
        }
        Msg::AirspeedHeading(m) => {
            assert_eq!(kind, "airspeed_heading", "{hex}");
            assert_eq!(m.supersonic, b(&r["supersonic"]));
            let has_airspeed_ref = decode_reference(index, "has_airspeed")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| b(&r["has_airspeed"]));
            assert_eq!(
                m.airspeed_valid,
                expected_bool(
                    deltas,
                    index,
                    "has_airspeed",
                    b(&r["has_airspeed"]),
                    has_airspeed_ref,
                    "#34"
                )
            );
            let airspeed_ref = decode_reference(index, "airspeed")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| i(&r["airspeed"]));
            assert_eq!(
                i64::from(m.airspeed_kts),
                expected_i64(
                    deltas,
                    index,
                    "airspeed",
                    i(&r["airspeed"]),
                    airspeed_ref,
                    "#34"
                ),
                "{hex}"
            );
            assert_eq!(m.true_airspeed, b(&r["true_airspeed"]));
            assert_eq!(m.heading_valid, b(&r["has_heading"]));
            assert_eq!(i64::from(m.heading_deg), i(&r["heading"]), "{hex}");
            assert_eq!(m.vrate_valid, b(&r["has_vrate"]));
            assert_eq!(i64::from(m.vrate_ftmin), i(&r["vrate"]), "{hex}");
            let has_geo_ref = decode_reference(index, "has_geo_minus_baro")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| b(&r["has_geo_minus_baro"]));
            assert_eq!(
                m.geo_minus_baro_valid,
                expected_bool(
                    deltas,
                    index,
                    "has_geo_minus_baro",
                    b(&r["has_geo_minus_baro"]),
                    has_geo_ref,
                    "#35"
                ),
                "{hex}"
            );
            let geo_ref = decode_reference(index, "geo_minus_baro")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| i(&r["geo_minus_baro"]));
            assert_eq!(
                i64::from(m.geo_minus_baro_ft),
                expected_i64(
                    deltas,
                    index,
                    "geo_minus_baro",
                    i(&r["geo_minus_baro"]),
                    geo_ref,
                    "#35"
                ),
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

fn check_pos(
    index: usize,
    c: &Case,
    got: Result<(f64, f64), AdsbError>,
    deltas: &mut DeltaManifest,
) {
    let r = &c.result;
    let expected_err = expected_error(
        deltas,
        index,
        r.get("error").and_then(Value::as_str),
        decode_reference(index, "error")
            .and_then(|v| v.as_str().map(str::to_string))
            .or_else(|| r.get("error").and_then(Value::as_str).map(str::to_string))
            .as_deref(),
        "#36",
    );
    match (got, expected_err) {
        (Err(e), Some(name)) => assert_eq!(err_name(&e), name, "{c:?}"),
        (Ok((lat, lon)), None) => {
            assert_close(lat, r["lat"].as_f64().unwrap(), TOL, c);
            assert_close(lon, r["lon"].as_f64().unwrap(), TOL, c);
        }
        (g, _) => panic!("{g:?} vs {r:?} for {c:?}"),
    }
}

fn global_airborne_case(index: usize, c: &Case, deltas: &mut DeltaManifest) {
    let newer = airborne(decode(&c.s("newer")).unwrap());
    let other = airborne(decode(&c.s("other")).unwrap());
    let got = if newer.odd() {
        cpr::global(&other, &newer, true)
    } else {
        cpr::global(&newer, &other, false)
    };
    check_pos(index, c, got, deltas);
}

fn global_surface_case(index: usize, c: &Case, deltas: &mut DeltaManifest) {
    let newer = surface(decode(&c.s("newer")).unwrap());
    let other = surface(decode(&c.s("other")).unwrap());
    let ref_lat = 52.0;
    let ref_lon = 5.0;
    if c.result.get("error").is_none() {
        let _ = deltas.expected(
            index,
            "surface_reference",
            &Value::Null,
            Value::String("52.0,5.0".into()),
            0.0,
            "#361",
        );
    }
    let got = if newer.odd() {
        cpr::global_surface(&other, &newer, true, ref_lat, ref_lon)
    } else {
        cpr::global_surface(&newer, &other, false, ref_lat, ref_lon)
    };
    check_pos(index, c, got, deltas);
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
    let mut deltas =
        DeltaManifest::load_from("docs/logic-fixes-area2/golden-deltas.tsv", "adsb_msgs");
    let mut n = 0;
    for (index, c) in g.cases.iter().enumerate() {
        match c.func.as_str() {
            "decode" => check_decode(index, c, &mut deltas),
            "nl" => assert_eq!(
                f64::from(cpr::nl(c.f("lat"))),
                c.result.as_f64().unwrap(),
                "{c:?}"
            ),
            "global_airborne" => global_airborne_case(index, c, &mut deltas),
            "global_surface" => global_surface_case(index, c, &mut deltas),
            "local_airborne" => check_pos(
                index,
                c,
                cpr::local(
                    &airborne(decode(&c.s("hex")).unwrap()),
                    c.f("ref_lat"),
                    c.f("ref_lon"),
                ),
                &mut deltas,
            ),
            "local_surface" => check_pos(
                index,
                c,
                cpr::local(
                    &surface(decode(&c.s("hex")).unwrap()),
                    c.f("ref_lat"),
                    c.f("ref_lon"),
                ),
                &mut deltas,
            ),
            f => panic!("unknown fn {f}"),
        }
        n += 1;
    }
    deltas.finish();
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

#[test]
fn finding_360_local_cpr_wraps_antimeridian_and_rejects_past_pole() {
    let modulo = |a: f64, b: f64| ((a % b) + b) % b;
    let encode = |lat: f64, lon: f64, odd: bool| {
        let dlat = if odd { 360.0 / 59.0 } else { 360.0 / 60.0 };
        let yz = (131072.0 * modulo(lat, dlat) / dlat).round() as u32 & 0x1FFFF;
        let nl = f64::from(cpr::nl(lat)).max(1.0);
        let dlon = 360.0 / (nl - if odd { 1.0 } else { 0.0 }).max(1.0);
        let xz = (131072.0 * modulo(lon, dlon) / dlon).round() as u32 & 0x1FFFF;
        (yz, xz)
    };
    let (cpr_lat, cpr_lon) = encode(0.0, -179.99, false);
    let msg = as_adsb::AirbornePosition {
        icao24: 0x3C0001,
        tc: 11,
        odd: false,
        cpr_lat,
        cpr_lon,
        has_position: true,
        alt_ft: Some(38000),
        surveillance_status: 0,
        nic_suppl_b: false,
        utc: false,
    };
    let (_, lon) = cpr::local(&msg, 0.0, 179.99).expect("decode near antimeridian");
    assert_close(lon, -179.99, 0.02, &"finding_360_lon");

    let mut pole = msg.clone();
    pole.cpr_lat = 1;
    let err = cpr::local(&pole, 89.99, 0.0).expect_err("past-pole candidate must fail");
    assert_eq!(err_name(&err), "BadFormatException");
}

#[test]
fn finding_361_surface_global_requires_reference() {
    let newer = surface(decode("8C4841753AAB238733C8CD4020B1").unwrap());
    let other = surface(decode("8C4841753A8A35323FAEBDAC702D").unwrap());
    let err = cpr::global(&other, &newer, true).expect_err("surface global needs reference");
    assert_eq!(err_name(&err), "MissingInformationException");
}
