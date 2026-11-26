use as_wintest::track::{as_nearest, set_azel, set_azimuth, set_elevation};
use as_wintest::*;

fn golden(text: &str, cs: u8) -> Vec<u8> {
    let mut b = text.as_bytes().to_vec();
    b.push(cs);
    b
}

#[test]
fn encode_set_azimuth_golden() {
    assert_eq!(
        encode(&set_azimuth("AS", 123.0)).unwrap(),
        golden("SETAZIMUTH: \"AS\" \"\" \"AUTO\" 00 123?", 0xB6)
    );
}

#[test]
fn encode_set_azimuth_wraps_359_6_to_000() {
    assert_eq!(
        encode(&set_azimuth("AS", 359.6)).unwrap(),
        golden("SETAZIMUTH: \"AS\" \"\" \"AUTO\" 00 000?", 0xB0)
    );
}

#[test]
fn encode_set_elevation_clamps_negative_to_000() {
    assert_eq!(
        encode(&set_elevation("AS", -5.0)).unwrap(),
        golden("SETELEVATION: \"AS\" \"\" \"AUTO\" 00 000?", 0xB5)
    );
}

#[test]
fn encode_set_elevation_golden() {
    assert_eq!(
        encode(&set_elevation("AS", 5.0)).unwrap(),
        golden("SETELEVATION: \"AS\" \"\" \"AUTO\" 00 005?", 0xBA)
    );
}

#[test]
fn encode_set_azel_golden() {
    let b = encode(&set_azel("AS", 123.456789, 5.0)).unwrap();
    assert_eq!(
        b,
        golden(
            "SETAZEL: \"AS\" \"\" \"AUTO\" 00 123.45678900 5.00000000?",
            0x98
        )
    );
    assert_eq!(b.len(), 52);
}

#[test]
fn encode_as_nearest_single_placeholder() {
    let d = "2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,0";
    assert_eq!(
        encode(&as_nearest("AS", "KST", d.into())).unwrap(),
        golden(&format!("ASNEAREST: \"AS\" \"KST\" \"{d}\""), 0xEA)
    );
    let d1 = "2026-11-24 12:00:00Z,TE1ST,JO50AA,TE2ND,JO60AA,1,TST123,M,120,85,7";
    assert_eq!(
        encode(&as_nearest("AS", "KST", d1.into())).unwrap().last(),
        Some(&0xDC)
    );
}

#[test]
fn encode_non_ascii_becomes_question_mark() {
    let b = encode(&as_nearest("AS", "K", "caf\u{e9}".into())).unwrap();
    assert!(b.windows(4).any(|w| w == b"caf?"));
}

#[test]
fn encode_none_and_unknown_is_none() {
    for kind in [WtKind::None, WtKind::Unknown] {
        let m = WtMessage {
            kind,
            src: "A".into(),
            dst: "B".into(),
            data: "C".into(),
            extra: String::new(),
        };
        assert_eq!(encode(&m), None);
    }
}

#[test]
fn checksum_sum_mod_256_or_0x80() {
    assert_eq!(checksum(&[0x01, 0x02]), 0x83);
    assert_eq!(checksum(&[0xFF, 0x02]), 0x81);
}

const PATH: &str = "1440000,TE1ST,JO50AA,TE2ND,JO60AA";

#[test]
fn decode_set_path_golden() {
    let b = golden(&format!("ASSETPATH: \"KST\" \"AS\" \"{PATH}\"?"), 0xB0);
    assert_eq!(b.len(), 59);
    let d = decode(&b).unwrap();
    assert_eq!(d.msg.kind, WtKind::AsSetPath);
    assert_eq!(d.msg.src, "KST");
    assert_eq!(d.msg.dst, "AS");
    assert_eq!(d.msg.data, PATH);
    assert!(d.checksum_ok);
}

#[test]
fn decode_show_path_golden() {
    let d = decode(&golden(
        &format!("ASSHOWPATH: \"KST\" \"AS\" \"{PATH}\"?"),
        0x85,
    ))
    .unwrap();
    assert_eq!(d.msg.kind, WtKind::AsShowPath);
    assert_eq!(d.msg.data, PATH);
    assert!(d.checksum_ok);
}

#[test]
fn decode_reports_bad_checksum_but_parses() {
    let d = decode(&golden(
        &format!("ASSETPATH: \"KST\" \"AS\" \"{PATH}\"?"),
        0x80,
    ))
    .unwrap();
    assert_eq!(d.msg.data, PATH);
    assert!(!d.checksum_ok);
    assert_eq!(d.checksum, 0x80);
}

#[test]
fn decode_bit7_only_corruption_fails_checksum() {
    // The correct trailing byte is 0xB0 (sum 0x30 | 0x80). A corrupt byte that differs only
    // in bit 7 (0x30) must not be reported as valid: masking bit 7 back on would accept it.
    let d = decode(&golden(
        &format!("ASSETPATH: \"KST\" \"AS\" \"{PATH}\"?"),
        0x30,
    ))
    .unwrap();
    assert_eq!(d.msg.data, PATH);
    assert!(!d.checksum_ok);
    assert_eq!(d.checksum, 0xB0);
}

#[test]
fn decode_as_nearest_roundtrip() {
    let m = as_nearest("AS", "KST", "a,b,c".into());
    let d = decode(&encode(&m).unwrap()).unwrap();
    assert_eq!(d.msg.kind, WtKind::AsNearest);
    assert_eq!(d.msg.data, "a,b,c");
    assert!(d.checksum_ok);
}

#[test]
fn decode_extra_stays_in_data() {
    let d = decode(&encode(&set_azimuth("AS", 123.0)).unwrap()).unwrap();
    assert_eq!(d.msg.dst, "");
    assert_eq!(d.msg.data, "AUTO 00 123");
    assert_eq!(d.msg.extra, "");
}

#[test]
fn decode_trims_trailing_question_marks() {
    let d = decode(&encode(&as_nearest("AS", "K", "WHAT?".into())).unwrap()).unwrap();
    assert_eq!(d.msg.data, "WHAT");
}

#[test]
fn decode_strips_quotes_in_data() {
    let mut b = b"ASSETPATH: \"K\" \"AS\" \"a\"b\"??".to_vec();
    *b.last_mut().unwrap() = 0x91;
    assert_eq!(decode(&b).unwrap().msg.data, "ab");
}

#[test]
fn decode_non_ascii_bytes_become_question_marks() {
    let mut b = b"ASSETPATH: \"K\" \"AS\" \"a".to_vec();
    b.extend([0xE9, b'b']);
    b.extend(b"\"??");
    assert_eq!(decode(&b).unwrap().msg.data, "a?b");
}

#[test]
fn decode_errors() {
    assert_eq!(decode(b""), Err(DecodeError::Empty));
    assert_eq!(decode(b"HELLO"), Err(DecodeError::NoNameSeparator));
    assert_eq!(decode(b"ASSETPATH: \"K\""), Err(DecodeError::MissingField));
    assert_eq!(decode(b"ASSETPATH: \"K\" \"AS\" "), Err(DecodeError::Empty));
}

#[test]
fn decode_never_panics_on_random_bytes() {
    let mut s: u64 = 0x1234_5678_9ABC_DEF0;
    let mut next = || {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (s >> 33) as u32
    };
    for _ in 0..10_000 {
        let len = (next() % 300) as usize;
        let v: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        let _ = decode(&v);
        let mut p = b"ASSETPATH: ".to_vec();
        p.extend(&v);
        let _ = decode(&p);
    }
}

#[test]
fn kind_parse() {
    assert_eq!(WtKind::parse("ASSETPATH"), WtKind::AsSetPath);
    assert_eq!(WtKind::parse(" ASSETPATH "), WtKind::AsSetPath);
    assert_eq!(WtKind::parse("assetpath"), WtKind::Unknown);
    assert_eq!(WtKind::parse("252"), WtKind::AsSetPath);
    assert_eq!(WtKind::parse("7"), WtKind::Unknown);
    assert_eq!(WtKind::parse("SETAZIMUTH,ASSETPATH"), WtKind::Unknown);
}

#[test]
fn wire_names_and_values_match_csharp() {
    let all = [
        ("NONE", 0u8, WtKind::None),
        ("STATUS", 1, WtKind::Status),
        ("PKTRCVD", 2, WtKind::PktRcvd),
        ("SETAZIMUTH", 100, WtKind::SetAzimuth),
        ("SETELEVATION", 101, WtKind::SetElevation),
        ("SETAZEL", 102, WtKind::SetAzEl),
        ("ASADDWATCH", 247, WtKind::AsAddWatch),
        ("ASREMOVEWATCH", 248, WtKind::AsRemoveWatch),
        ("ASWATCHLIST", 249, WtKind::AsWatchlist),
        ("ASSETPATH", 252, WtKind::AsSetPath),
        ("ASSHOWPATH", 253, WtKind::AsShowPath),
        ("ASNEAREST", 254, WtKind::AsNearest),
        ("UNKNOWN", 255, WtKind::Unknown),
    ];
    for (name, v, k) in all {
        assert_eq!(k.wire_name(), name);
        assert_eq!(k as u8, v);
        assert_eq!(WtKind::parse(name), k);
    }
}
