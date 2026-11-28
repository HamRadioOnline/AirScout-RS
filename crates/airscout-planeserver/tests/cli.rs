//! The `airscout-planeserver` command line and the headless daemon. Every run sets `--data`
//! to a temp dir and binds `127.0.0.1`; no test uses the internet.
use airscout_planeserver::config::PlaneServerConfig;
use airscout_planeserver::start_lines;
use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use std::io::{BufRead, BufReader};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::{Child, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const BIN: &str = "airscout-planeserver";

fn bin() -> Command {
    Command::cargo_bin(BIN).unwrap()
}

/// Kills the child if a test fails before it ends.
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn headless_serves_and_stops() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("ps.toml");
    let out = bin()
        .args([
            "--headless",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--run-for",
            "2",
        ])
        .arg("--data")
        .arg(tmp.path())
        .arg("--config")
        .arg(&cfg)
        .timeout(Duration::from_secs(10))
        .assert()
        .success()
        .get_output()
        .clone();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("serving /planes.json on http://127.0.0.1:"),
        "{err}"
    );
    assert!(err.contains("(no authentication)"), "{err}");
    assert!(!err.contains("0.0.0.0"), "{err}");
    assert!(err.contains("no feed configured; edit "), "{err}");
    assert!(err.contains("ps.toml"), "{err}");
    assert!(!err.contains("whole network"), "{err}");
}

#[test]
fn headless_serves_planes_json_while_running() {
    let tmp = tempfile::tempdir().unwrap();
    let child = std::process::Command::new(cargo_bin(BIN))
        .args([
            "--headless",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--run-for",
            "6",
        ])
        .arg("--data")
        .arg(tmp.path())
        .arg("--config")
        .arg(tmp.path().join("ps.toml"))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = Guard(child);
    let stderr = guard.0.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut url = None;
    while let Ok(line) = rx.recv_timeout(Duration::from_secs(5)) {
        if let Some(rest) = line.split("serving /planes.json on ").nth(1) {
            url = rest.split(' ').next().map(str::to_owned);
            break;
        }
    }
    let url = url.expect("no start line within 5 s");
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    assert!(!url.contains(":0/"), "{url}");
    let body = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
        .get(format!("{url}planes.json"))
        .send()
        .unwrap()
        .text()
        .unwrap();
    assert!(body.contains(r#""full_count":0,"version":1"#), "{body}");
    let status = guard.0.wait().unwrap();
    assert!(status.success(), "{status}");
}

#[test]
fn headless_refuses_broken_config() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("broken.toml");
    std::fs::write(&cfg, "password = \"hunter2-secret\"\nthis is [not toml").unwrap();
    let out = bin()
        .args([
            "--headless",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--run-for",
            "2",
        ])
        .arg("--data")
        .arg(tmp.path())
        .arg("--config")
        .arg(&cfg)
        .timeout(Duration::from_secs(10))
        .assert()
        .code(2)
        .get_output()
        .clone();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("broken.toml"), "{err}");
    assert!(!err.contains("hunter2"), "{err}");
    assert!(!err.contains("not toml"), "{err}");
}

#[test]
fn headless_port_in_use_no_serving_line() {
    let blocker = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = blocker.local_addr().unwrap().port();
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("ps.toml");
    let child = std::process::Command::new(cargo_bin(BIN))
        .args(["--headless", "--bind", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .args(["--run-for", "2"])
        .arg("--data")
        .arg(tmp.path())
        .arg("--config")
        .arg(&cfg)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = Guard(child);
    let mut stderr = guard.0.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
        let _ = tx.send(text);
    });
    let err = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("child did not end within 10 s");
    let status = guard.0.wait().unwrap();
    assert!(status.success(), "{status}: {err}");
    assert!(!err.contains("serving /planes.json"), "{err}");
    assert!(err.contains(&format!("127.0.0.1:{port}")), "{err}");
    assert!(err.contains("cannot listen"), "{err}");
    drop(blocker);
}

#[test]
fn start_lines_warn_on_lan_bind() {
    let cfg = PlaneServerConfig {
        bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        port: 9872,
        ..PlaneServerConfig::default()
    };
    let lan = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 9872);
    let lines = start_lines(&cfg, Some(lan), "listening on 0.0.0.0:9872");
    assert_eq!(
        lines[0],
        "airscout-planeserver: serving /planes.json on http://0.0.0.0:9872/ (no authentication)"
    );
    assert!(lines
        .iter()
        .any(|l| l == "warning: port open to the whole network"));
    // the bound address wins over the configured port 0
    let cfg = PlaneServerConfig {
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: 0,
        ..PlaneServerConfig::default()
    };
    let bound = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4321);
    let lines = start_lines(&cfg, Some(bound), "listening");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("http://127.0.0.1:4321/"), "{lines:?}");
    // IPv6 loopback is loopback, and is written with brackets
    let cfg = PlaneServerConfig {
        bind: "::1".parse().unwrap(),
        port: 9872,
        ..PlaneServerConfig::default()
    };
    let v6 = SocketAddr::new("::1".parse().unwrap(), 9872);
    let lines = start_lines(&cfg, Some(v6), "listening");
    assert!(lines[0].contains("http://[::1]:9872/"), "{lines:?}");
    assert_eq!(lines.len(), 1, "{lines:?}");
}

#[test]
fn start_lines_no_serving_line_without_a_bind() {
    let cfg = PlaneServerConfig {
        bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        port: 9872,
        ..PlaneServerConfig::default()
    };
    let err = "Web server: cannot listen on 0.0.0.0:9872: Address already in use";
    let lines = start_lines(&cfg, None, err);
    assert!(
        !lines.iter().any(|l| l.contains("serving /planes.json")),
        "{lines:?}"
    );
    assert!(lines[0].contains(err), "{lines:?}");
    // the LAN warning stays
    assert!(
        lines.iter().any(|l| l.contains("whole network")),
        "{lines:?}"
    );
}

#[test]
fn overrides_not_saved() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = tmp.path().join("ps.toml");
    let text = "port = 9999\nbind = \"127.0.0.1\"\n";
    std::fs::write(&cfg, text).unwrap();
    bin()
        .args([
            "--headless",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--run-for",
            "1",
        ])
        .arg("--data")
        .arg(tmp.path().join("d"))
        .arg("--config")
        .arg(&cfg)
        .timeout(Duration::from_secs(10))
        .assert()
        .success();
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), text);
    // a missing file stays missing
    let none = tmp.path().join("none.toml");
    bin()
        .args([
            "--headless",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--run-for",
            "1",
        ])
        .arg("--data")
        .arg(tmp.path().join("d"))
        .arg("--config")
        .arg(&none)
        .timeout(Duration::from_secs(10))
        .assert()
        .success();
    assert!(!none.exists());
}

#[test]
fn help_lists_flags() {
    let out = bin().arg("--help").assert().success().get_output().clone();
    let help = String::from_utf8_lossy(&out.stdout);
    for f in ["--headless", "--config", "--bind", "--port", "--data"] {
        assert!(help.contains(f), "{f} missing: {help}");
    }
    assert!(!help.contains("--run-for"), "hidden flag shown: {help}");
}

#[test]
fn run_for_range_is_checked() {
    let tmp = tempfile::tempdir().unwrap();
    for v in ["0", "3601"] {
        bin()
            .args(["--headless", "--run-for", v])
            .arg("--data")
            .arg(tmp.path())
            .timeout(Duration::from_secs(10))
            .assert()
            .code(2);
    }
}

/// Without `--headless` the window opens (not run here); a broken config still ends the
/// program before any window, as in headless mode.
#[test]
fn without_headless_a_broken_config_fails_before_the_window() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("ps.toml");
    std::fs::write(&path, "port = \"not a number\"").unwrap();
    bin()
        .arg("--data")
        .arg(tmp.path())
        .arg("--config")
        .arg(&path)
        .timeout(Duration::from_secs(10))
        .assert()
        .code(2);
}
