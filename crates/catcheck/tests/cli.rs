//! `catcheck` against the 4b fake rigctld on 127.0.0.1. Every run passes `--settings` in a
//! temporary directory, so the user's settings file is never read.

#[path = "../../as-cat/tests/common/fake_rigctld.rs"]
mod fake_rigctld;

use assert_cmd::Command;
use fake_rigctld::{Behaviour, FakeRigctld, RigState};
use std::path::Path;
use std::time::{Duration, Instant};

fn cc(dir: &Path) -> Command {
    let mut c = Command::cargo_bin("catcheck").unwrap();
    c.arg("--settings").arg(dir.join("settings.toml"));
    c.timeout(Duration::from_secs(10));
    c
}

/// `cc` with the rigctld engine pointed at `fake`.
fn rig(dir: &Path, fake: &FakeRigctld) -> Command {
    let mut c = cc(dir);
    c.args(["--engine", "rigctld", "--host", "127.0.0.1", "--port"])
        .arg(fake.addr.port().to_string());
    c
}

struct Out {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run(mut c: Command) -> Out {
    let o = c.output().unwrap();
    Out {
        code: o.status.code(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn lines(s: &str) -> Vec<&str> {
    s.lines().collect()
}

const ONLINE: &str = "ONLINE rx=144300000 tx=144300000 mode=USB split=OFF rit=OFF";

/// A free TCP port on 127.0.0.1 with nothing listening (bind and release).
fn closed_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

#[test]
fn status_online_from_fake() {
    let d = tempfile::tempdir().unwrap();
    let fake = FakeRigctld::start(RigState::default());
    let mut c = rig(d.path(), &fake);
    c.args(["status", "--count", "2", "--interval-ms", "100"]);
    let o = run(c);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    let rig_line = format!("rig: [rigctld] 127.0.0.1:{}", fake.addr.port());
    assert_eq!(lines(&o.stdout), vec![rig_line.as_str(), ONLINE, ONLINE]);
}

#[test]
fn status_reads_settings_file() {
    let d = tempfile::tempdir().unwrap();
    let fake = FakeRigctld::start(RigState::default());
    std::fs::write(
        d.path().join("settings.toml"),
        format!(
            "[cat]\nengine = \"rigctld\"\nrigctld_host = \"127.0.0.1\"\nrigctld_port = {}\n",
            fake.addr.port()
        ),
    )
    .unwrap();
    let mut c = cc(d.path());
    c.args(["status", "--count", "2", "--interval-ms", "100"]);
    let o = run(c);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    let rig_line = format!("rig: [rigctld] 127.0.0.1:{}", fake.addr.port());
    assert_eq!(lines(&o.stdout), vec![rig_line.as_str(), ONLINE, ONLINE]);
}

#[test]
fn status_without_rigctld_reports_nocat() {
    let d = tempfile::tempdir().unwrap();
    let port = closed_port();
    let mut c = cc(d.path());
    c.args(["--engine", "rigctld", "--host", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .args(["--timeout-ms", "200", "status", "--count", "1"]);
    let t = Instant::now();
    let o = run(c);
    assert!(t.elapsed() < Duration::from_secs(10));
    assert_eq!(o.code, Some(2), "{}", o.stderr);
    let l = lines(&o.stdout);
    assert_eq!(l.len(), 2, "{}", o.stdout);
    let want = format!("NOCAT rigctld 127.0.0.1:{port}");
    assert!(l[1].starts_with(&want), "{}", o.stdout);
}

#[test]
fn silent_rigctld_times_out() {
    let d = tempfile::tempdir().unwrap();
    let fake = FakeRigctld::start(RigState::default());
    fake.set_behaviour(Behaviour::Silent);
    let mut c = rig(d.path(), &fake);
    c.args(["--timeout-ms", "200", "status", "--count", "1"]);
    let t = Instant::now();
    let o = run(c);
    assert!(t.elapsed() < Duration::from_secs(5));
    assert_eq!(o.code, Some(2), "{}", o.stderr);
    assert_eq!(lines(&o.stdout).get(1), Some(&"OFFLINE"), "{}", o.stdout);
}

#[test]
fn set_freq_rx_and_tx() {
    let d = tempfile::tempdir().unwrap();
    let fake = FakeRigctld::start(RigState::default());
    let mut c = rig(d.path(), &fake);
    c.args(["set-freq", "144310000"]);
    let o = run(c);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    assert_eq!(fake.state().freq, 144_310_000);

    let mut c = rig(d.path(), &fake);
    c.args(["set-freq", "144320000", "--tx"]);
    let o = run(c);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    assert_eq!(fake.state().freq, 144_320_000);

    let split = FakeRigctld::start(RigState {
        split: true,
        ..RigState::default()
    });
    let mut c = rig(d.path(), &split);
    c.args(["set-freq", "144320000", "--tx"]);
    let o = run(c);
    assert_eq!(o.code, Some(0), "{}", o.stderr);
    assert_eq!(split.state().split_freq, 144_320_000);
    assert_eq!(split.state().freq, 144_300_000);
    assert!(split.lines().iter().any(|l| l == "I 144320000"));
}

#[test]
fn out_of_range_freq_never_sent() {
    let d = tempfile::tempdir().unwrap();
    let fake = FakeRigctld::start(RigState::default());
    for hz in ["0", "-5", "100000000001"] {
        let mut c = rig(d.path(), &fake);
        c.args(["set-freq", hz]);
        let o = run(c);
        assert_eq!(o.code, Some(1), "{hz}: {}", o.stderr);
        assert!(
            o.stderr.contains("frequency must be 1 to 100000000000 Hz"),
            "{hz}: {}",
            o.stderr
        );
    }
    let mut c = rig(d.path(), &fake);
    c.args([
        "doppler-test",
        "--base-hz",
        "1000",
        "--shift-hz",
        "1000",
        "--steps",
        "2",
    ]);
    let o = run(c);
    assert_eq!(o.code, Some(1), "{}", o.stderr);
    assert!(o.stderr.contains("frequency must be 1 to 100000000000 Hz"));
    assert_eq!(fake.accepts(), 0);

    // base from the rig: the rig is read, but nothing is set
    let mut c = rig(d.path(), &fake);
    c.args(["doppler-test", "--shift-hz", "100000000000", "--steps", "1"]);
    let o = run(c);
    assert_eq!(o.code, Some(1), "{}", o.stderr);
    assert!(o.stderr.contains("frequency must be 1 to 100000000000 Hz"));
    let sets = ["F ", "I ", "S ", "M ", "X ", "U "];
    for l in fake.lines() {
        assert!(!sets.iter().any(|p| l.starts_with(p)), "set sent: {l}");
    }
}

#[test]
fn set_mode_split_rit() {
    let d = tempfile::tempdir().unwrap();
    let fake = FakeRigctld::start(RigState::default());
    for args in [["set-mode", "cw"], ["split", "on"], ["rit", "on"]] {
        let mut c = rig(d.path(), &fake);
        c.args(args);
        let o = run(c);
        assert_eq!(o.code, Some(0), "{args:?}: {}", o.stderr);
    }
    let s = fake.state();
    assert_eq!(s.mode, "CW");
    assert!(s.split);
    assert!(s.rit);

    let mut c = rig(d.path(), &fake);
    c.args(["set-mode", "other"]);
    let o = run(c);
    assert_eq!(o.code, Some(1), "{}", o.stderr);
    assert!(o.stderr.contains("unknown mode: other"), "{}", o.stderr);
}

#[test]
fn doppler_test_restores_rig() {
    let d = tempfile::tempdir().unwrap();
    let fake = FakeRigctld::start(RigState::default());
    let mut c = rig(d.path(), &fake);
    c.args([
        "doppler-test",
        "--shift-hz",
        "1000",
        "--steps",
        "2",
        "--step-ms",
        "100",
    ]);
    let o = run(c);
    assert_eq!(o.code, Some(0), "{}\n{}", o.stdout, o.stderr);
    let l = lines(&o.stdout);
    assert!(l[0].starts_with("rig: "), "{}", o.stdout);
    assert_eq!(
        l[1], "saved: rx=144300000 tx=144300000 mode=USB split=OFF rit=OFF",
        "{}",
        o.stdout
    );
    assert_eq!(l.last(), Some(&"restored: yes"), "{}", o.stdout);
    let sent = fake.lines();
    for want in ["F 144301000", "I 144299000", "F 144302000", "I 144298000"] {
        assert!(sent.iter().any(|x| x == want), "{want} missing: {sent:?}");
    }
    let s = fake.state();
    assert_eq!(s.freq, 144_300_000);
    assert!(!s.split);
    assert_eq!(s.mode, "USB");
}

#[test]
fn no_engine_is_error() {
    let d = tempfile::tempdir().unwrap();
    let mut c = cc(d.path());
    c.arg("status");
    let o = run(c);
    assert_eq!(o.code, Some(1), "{}", o.stderr);
    assert!(o.stderr.contains("no CAT engine selected"), "{}", o.stderr);
}

#[cfg(not(feature = "hamlib"))]
#[test]
fn hamlib_not_compiled_is_error() {
    let d = tempfile::tempdir().unwrap();
    let mut c = cc(d.path());
    c.args(["--engine", "hamlib", "status"]);
    let o = run(c);
    assert_eq!(o.code, Some(1), "{}", o.stderr);
    assert!(o.stderr.contains("not compiled in"), "{}", o.stderr);
}

#[cfg(not(windows))]
#[test]
fn omnirig_off_windows_is_error() {
    let d = tempfile::tempdir().unwrap();
    let mut c = cc(d.path());
    c.args(["--engine", "omnirig", "status"]);
    let o = run(c);
    assert_eq!(o.code, Some(1), "{}", o.stderr);
    assert!(
        o.stderr.contains("OmniRig is available on Windows only"),
        "{}",
        o.stderr
    );
}

/// Finding #247: without the OmniRig COM server the rig reports NOCAT.
#[cfg(windows)]
#[test]
fn omnirig_without_server_reports_nocat() {
    let d = tempfile::tempdir().unwrap();
    let mut c = cc(d.path());
    c.args([
        "--engine", "omnirig", "--rig", "1", "status", "--count", "1",
    ]);
    let o = run(c);
    assert_eq!(o.code, Some(2), "{}", o.stderr);
    assert!(
        lines(&o.stdout)
            .get(1)
            .is_some_and(|l| l.starts_with("NOCAT")),
        "{}",
        o.stdout
    );
}

#[test]
fn broken_settings_never_echo_secret() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("settings.toml"),
        "[cat]\nengine = \"rigctld\"\n[feed_settings.OpenSky]\nclient_secret = \"SECRET-TEST-1",
    )
    .unwrap();
    let mut c = cc(d.path());
    c.args(["status", "--count", "1"]);
    let o = run(c);
    assert_eq!(o.code, Some(1), "{}", o.stderr);
    assert!(o.stderr.contains("settings.toml"), "{}", o.stderr);
    assert!(o.stderr.contains("line 4"), "{}", o.stderr);
    assert!(!o.stdout.contains("SECRET-TEST-1"));
    assert!(!o.stderr.contains("SECRET-TEST-1"));
}

#[test]
fn list_ports_does_not_panic() {
    let d = tempfile::tempdir().unwrap();
    let mut c = cc(d.path());
    c.arg("list-ports");
    let o = run(c);
    assert!(matches!(o.code, Some(0 | 1)), "{:?}: {}", o.code, o.stderr);
}
