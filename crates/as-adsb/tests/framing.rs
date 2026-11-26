//! Beast binary and AVR text framing (`RTL1090.cs` `ReceiveBinaryMsg` / `ReceiveAVRMsg`).
use as_adsb::framing::{avr_line, BeastReader};

const LONG: [u8; 14] = [
    0x8D, 0x48, 0x40, 0xD6, 0x20, 0x2C, 0xC3, 0x71, 0xC3, 0x2C, 0xE0, 0x57, 0x60, 0x98,
];
const SHORT: [u8; 7] = [0x5D, 0x48, 0x4F, 0xDE, 0xA2, 0x48, 0xF5];

fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![0x1A, kind, 1, 2, 3, 4, 5, 6, 0x80];
    v.extend_from_slice(payload);
    v
}

fn stuffed(kind: u8, timestamp: [u8; 6], signal: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x1A, kind];
    for b in timestamp
        .into_iter()
        .chain(std::iter::once(signal))
        .chain(payload.iter().copied())
    {
        if b == 0x1A {
            frame.extend([0x1A, 0x1A]);
        } else {
            frame.push(b);
        }
    }
    frame
}

fn three_frames() -> Vec<u8> {
    let mut v = frame(0x33, &LONG);
    v.extend(frame(0x32, &SHORT));
    v.extend(frame(0x33, &LONG));
    v
}

#[test]
fn beast_emits_long_and_short() {
    let mut r = BeastReader::default();
    assert_eq!(
        r.push(&three_frames()),
        vec![
            "*8D4840D6202CC371C32CE0576098;",
            "*5D484FDEA248F5;",
            "*8D4840D6202CC371C32CE0576098;"
        ]
    );
}

#[test]
fn beast_split_across_pushes() {
    let all = three_frames();
    let whole = BeastReader::default().push(&all);
    for cut in 0..=all.len() {
        let mut r = BeastReader::default();
        let mut out = r.push(&all[..cut]);
        out.extend(r.push(&all[cut..]));
        assert_eq!(out, whole, "cut at {cut}");
    }
    // byte by byte
    let mut r = BeastReader::default();
    let out: Vec<String> = all.iter().flat_map(|b| r.push(&[*b])).collect();
    assert_eq!(out, whole);
}

#[test]
fn beast_garbage_between_frames() {
    let mut v = vec![0x00, 0xFF, 0x33, 0x1A, 0x99];
    v.extend(frame(0x33, &LONG));
    v.extend([0x42, 0x1A, 0x34, 0x00]);
    // Mode A/C frame: C# skips only the two header bytes and rescans the rest
    v.extend([
        0x1A, 0x31, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x34,
    ]);
    v.extend(frame(0x32, &SHORT));
    assert_eq!(
        BeastReader::default().push(&v),
        vec!["*8D4840D6202CC371C32CE0576098;", "*5D484FDEA248F5;"]
    );
}

#[test]
fn finding_40_beast_unstuffs_escaped_markers_across_chunks() {
    let mut payload = LONG.to_vec();
    payload[4] = 0x1A;
    let mut v = stuffed(0x33, [1, 0x1A, 3, 0x1A, 5, 6], 0x1A, &payload);
    v.extend(frame(0x32, &SHORT));

    let mut expected = vec!["*8D4840D61A2CC371C32CE0576098;".to_string()];
    expected.push("*5D484FDEA248F5;".to_string());

    let whole = BeastReader::default().push(&v);
    assert_eq!(whole, expected);
    for cut in 0..=v.len() {
        let mut r = BeastReader::default();
        let mut out = r.push(&v[..cut]);
        out.extend(r.push(&v[cut..]));
        assert_eq!(out, expected, "cut at {cut}");
    }
}

#[test]
fn beast_truncated_stream_no_panic() {
    let all = three_frames();
    for n in 0..=all.len() {
        let out = BeastReader::default().push(&all[..n]);
        let complete = [23, 39, 62].iter().filter(|&&e| n >= e).count();
        assert_eq!(out.len(), complete, "prefix {n}");
    }
}

#[test]
fn avr_both_formats() {
    assert_eq!(
        avr_line("*8D4840D6202CC371C32CE0576098;").as_deref(),
        Some("*8D4840D6202CC371C32CE0576098;")
    );
    assert_eq!(
        avr_line("@0123456789AB8D4840D6202CC371C32CE0576098;\r").as_deref(),
        Some("*8D4840D6202CC371C32CE0576098;")
    );
    assert_eq!(avr_line("@0123;"), None);
    assert_eq!(avr_line("8D4840D6;"), None);
    assert_eq!(avr_line("*8D4840D6"), None);
    assert_eq!(avr_line(""), None);
}
