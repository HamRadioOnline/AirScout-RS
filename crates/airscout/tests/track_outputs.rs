//! Track outputs: UDP datagrams, azel.dat formats, output order.

use airscout::net_addrs::NetAddrs;
use airscout::track_outputs::{
    build_outputs, native_azel, udp_datagrams, wsjt_azel, FileFormat, FileOutput, OutputError,
    OutputGroup, TrackConfig, TrackOutput, UdpFormat, UdpOutput, NEWLINE,
};
use as_settings::{Settings, TrackFile, TrackSerial, TrackUdp};
use as_wintest::encode;
use as_wintest::track::{set_azel, set_azimuth, set_elevation};
use chrono::{TimeZone, Utc};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

fn receiver() -> (UdpSocket, SocketAddr) {
    let rx = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    rx.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let addr = rx.local_addr().unwrap();
    (rx, addr)
}

fn recv(rx: &UdpSocket) -> Vec<u8> {
    let mut buf = [0u8; 2048];
    let n = rx.recv(&mut buf).unwrap();
    buf[..n].to_vec()
}

fn nothing_more(rx: &UdpSocket) {
    rx.set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let mut buf = [0u8; 2048];
    assert!(rx.recv(&mut buf).is_err());
}

#[test]
fn native_azel_format() {
    assert_eq!(native_azel(0.25, 5.04), format!("0.3,5.0{NEWLINE}"));
}

#[test]
fn wsjt_azel_format() {
    let t = Utc.with_ymd_and_hms(2026, 11, 24, 12, 34, 56).unwrap();
    let want = [
        "12:34:56,0,0,Moon",
        "12:34:56,0,0,Sun",
        "12:34:56,123.5,5.0,Source",
        "0,  0, 0,  0, 0,Doppler, R",
    ]
    .map(|l| l.to_string() + NEWLINE)
    .concat();
    assert_eq!(wsjt_azel(t, 123.45, 5.0), want);
}

#[test]
fn udp_wintest_sends_one_datagram() {
    let (rx, addr) = receiver();
    let mut out = UdpOutput::new(UdpFormat::WinTest, "AS", Ipv4Addr::LOCALHOST, addr);
    out.connect().unwrap();
    assert!(out.is_connected());
    out.send(123.0, 5.0).unwrap();
    assert_eq!(recv(&rx), encode(&set_azimuth("AS", 123.0)).unwrap());
    nothing_more(&rx);
}

#[test]
fn udp_airscout_sends_three_datagrams_in_order() {
    let (rx, addr) = receiver();
    let mut out = UdpOutput::new(UdpFormat::AirScout, "AS", Ipv4Addr::LOCALHOST, addr);
    out.connect().unwrap();
    out.send(123.0, 5.0).unwrap();
    assert_eq!(recv(&rx), encode(&set_azimuth("AS", 123.0)).unwrap());
    assert_eq!(recv(&rx), encode(&set_elevation("AS", 5.0)).unwrap());
    let azel = recv(&rx);
    assert_eq!(azel, encode(&set_azel("AS", 123.0, 5.0)).unwrap());
    assert!(String::from_utf8_lossy(&azel).contains("00 123.00000000 5.00000000"));
    assert_eq!(
        udp_datagrams(UdpFormat::AirScout, "AS", 123.0, 5.0).len(),
        3
    );
}

#[test]
fn udp_send_without_connect_is_not_connected() {
    let (_rx, addr) = receiver();
    let mut out = UdpOutput::new(UdpFormat::WinTest, "AS", Ipv4Addr::LOCALHOST, addr);
    assert_eq!(
        out.send(1.0, 2.0),
        Err(OutputError::NotConnected(
            "[UDP]: Client and/or IP endpoint not initialized.".into()
        ))
    );
}

#[test]
fn file_native_overwrites_each_step() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Tmp").join("azel.dat");
    let mut out = FileOutput::new(FileFormat::Native, path.clone());
    out.connect().unwrap();
    out.send(10.0, 1.0).unwrap();
    out.send(20.0, 2.0).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("20.0,2.0{NEWLINE}")
    );
}

#[test]
fn azel_dat_is_replaced_atomically() {
    // #206: the line is written to a same-directory temp file and renamed over azel.dat, so a
    // reader holding the old file keeps the old, complete line while the path already holds the
    // new one, and no temp file is left behind.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Tmp").join("azel.dat");
    let mut out = FileOutput::new(FileFormat::Native, path.clone());
    out.connect().unwrap();
    out.send(10.0, 1.0).unwrap();
    let mut old = std::fs::File::open(&path).unwrap();
    out.send(20.0, 2.0).unwrap();
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut old, &mut buf).unwrap();
    assert_eq!(buf, format!("10.0,1.0{NEWLINE}"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("20.0,2.0{NEWLINE}")
    );
    let leftovers: Vec<String> = std::fs::read_dir(dir.path().join("Tmp"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != "azel.dat")
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn file_output_unwritable_dir_reports_error() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    let mut out = FileOutput::new(FileFormat::Native, blocker.join("azel.dat"));
    let failed = out.connect().is_err() || out.send(1.0, 2.0).is_err();
    assert!(failed);
}

#[test]
fn build_outputs_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Settings::default();
    s.track.udp = TrackUdp::AirScout;
    s.track.file = TrackFile::Wsjt;
    let c = TrackConfig::from_settings(&s, dir.path(), NetAddrs::loopback());
    assert_eq!(c.azel_path, dir.path().join("Tmp").join("azel.dat"));
    let groups: Vec<_> = build_outputs(&c).iter().map(|o| o.group()).collect();
    assert_eq!(groups, [OutputGroup::Udp, OutputGroup::File]);
    s.track.udp = TrackUdp::None;
    s.track.file = TrackFile::None;
    let c = TrackConfig::from_settings(&s, dir.path(), NetAddrs::loopback());
    assert!(build_outputs(&c).is_empty());
}

fn kinds(c: &TrackConfig) -> Vec<OutputGroup> {
    build_outputs(c).iter().map(|o| o.group()).collect()
}

#[test]
fn outputs_in_csharp_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Settings::default();
    s.track.udp = TrackUdp::WinTest;
    s.track.serial = TrackSerial::Gs232Az;
    s.track.file = TrackFile::Native;
    let c = TrackConfig::from_settings(&s, dir.path(), NetAddrs::loopback());
    assert_eq!(
        kinds(&c),
        [OutputGroup::Udp, OutputGroup::Serial, OutputGroup::File]
    );
}

#[cfg(not(windows))]
#[test]
fn dde_ignored_off_windows() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Settings::default();
    s.track.dde = as_settings::TrackDde::Hrd;
    let c = TrackConfig::from_settings(&s, dir.path(), NetAddrs::loopback());
    assert!(kinds(&c).is_empty());
}

/// HRD DDE goes before the other outputs (`MapDlg.cs:8162-8166`); built and dropped on this
/// thread, without a connect.
#[cfg(windows)]
#[test]
fn dde_first_in_csharp_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Settings::default();
    s.track.dde = as_settings::TrackDde::Hrd;
    s.track.udp = TrackUdp::WinTest;
    s.track.serial = TrackSerial::Gs232Az;
    s.track.file = TrackFile::Native;
    let c = TrackConfig::from_settings(&s, dir.path(), NetAddrs::loopback());
    assert_eq!(
        kinds(&c),
        [
            OutputGroup::Dde,
            OutputGroup::Udp,
            OutputGroup::Serial,
            OutputGroup::File
        ]
    );
}
