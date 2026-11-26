use as_wintest::{decode, encode, WtKind, WtMessage};
use assert_cmd::Command;
use std::net::UdpSocket;
use std::process::Stdio;
use std::time::{Duration, Instant};

const LO: &str = "127.0.0.1";

fn sim(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("airscout-cli")
        .unwrap()
        .arg("wintest-sim")
        .args(args)
        .output()
        .unwrap()
}

fn run(args: &[&str]) -> serde_json::Value {
    let out = sim(args);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn run_err(args: &[&str]) -> String {
    let out = sim(args);
    assert!(!out.status.success(), "expected failure");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn free_udp_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn receiver() -> (UdpSocket, String) {
    let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
    rx.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let port = rx.local_addr().unwrap().port().to_string();
    (rx, port)
}

fn recv(rx: &UdpSocket) -> Vec<u8> {
    let mut buf = [0u8; 65536];
    let n = rx.recv(&mut buf).unwrap();
    buf[..n].to_vec()
}

#[test]
fn sends_csharp_bytes() {
    let (rx, port) = receiver();
    let v = run(&[
        "--msg",
        "assetpath",
        "--from",
        "Test",
        "--to",
        "AS",
        "--data",
        "TE1ST,JO50AA,TE2ND,JO60AA",
        "--addr",
        LO,
        "--bind",
        LO,
        "--port",
        &port,
    ]);
    let want = encode(&WtMessage {
        kind: WtKind::AsSetPath,
        src: "Test".into(),
        dst: "AS".into(),
        data: "TE1ST,JO50AA,TE2ND,JO60AA".into(),
        extra: String::new(),
    })
    .unwrap();
    assert_eq!(recv(&rx), want);
    assert_eq!(v["sent"]["bytes"], want.len());
    assert!(v["sent"]["text"]
        .as_str()
        .unwrap()
        .starts_with("ASSETPATH: \"Test\" \"AS\" \"TE1ST"));
    assert_eq!(v["sent"]["checksum"], *want.last().unwrap());
}

#[test]
fn defaults_match_csharp() {
    let (rx, port) = receiver();
    run(&["--addr", LO, "--bind", LO, "--port", &port]);
    let d = decode(&recv(&rx)).unwrap();
    assert_eq!(d.msg.kind, WtKind::AsAddWatch);
    assert_eq!(d.msg.src, "Test");
    assert_eq!(d.msg.dst, "AS");
    assert_eq!(d.msg.data, "");
    assert!(d.checksum_ok);
}

#[test]
fn msg_trimmed_and_upper_cased() {
    let (rx, port) = receiver();
    run(&[
        "--msg",
        " asnearest ",
        "--addr",
        LO,
        "--bind",
        LO,
        "--port",
        &port,
    ]);
    let b = recv(&rx);
    assert_eq!(decode(&b).unwrap().msg.kind, WtKind::AsNearest);
    assert_eq!(b[b.len() - 2], b'"');
    assert!(b.last().unwrap() & 0x80 != 0);
}

#[test]
fn numeric_msg_accepted() {
    let (rx, port) = receiver();
    run(&["--msg", "252", "--addr", LO, "--bind", LO, "--port", &port]);
    assert_eq!(decode(&recv(&rx)).unwrap().msg.kind, WtKind::AsSetPath);
}

#[test]
fn unknown_msg_is_error() {
    let (rx, port) = receiver();
    rx.set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    for (m, shown) in [
        ("FOO", "FOO"),
        ("none", "NONE"),
        ("UNKNOWN", "UNKNOWN"),
        ("7", "7"),
    ] {
        let e = run_err(&["--msg", m, "--addr", LO, "--bind", LO, "--port", &port]);
        assert!(
            e.contains(&format!("unknown Win-Test message: {shown}")),
            "{e}"
        );
    }
    let mut buf = [0u8; 64];
    assert!(rx.recv(&mut buf).is_err());
}

#[test]
fn bad_port_and_listen_rejected() {
    let e = run_err(&["--port", "0", "--addr", LO, "--bind", LO]);
    assert!(e.contains("port must be 1-65535"), "{e}");
    for l in ["0", "3601"] {
        let e = run_err(&["--listen", l, "--port", "9872", "--addr", LO, "--bind", LO]);
        assert!(e.contains("--listen must be 1 to 3600 seconds"), "{e}");
    }
}

fn asnearest() -> Vec<u8> {
    encode(&WtMessage {
        kind: WtKind::AsNearest,
        src: "AS".into(),
        dst: "Test".into(),
        data: "TE1ST,JO50AA".into(),
        extra: String::new(),
    })
    .unwrap()
}

/// Runs the simulator with --listen 2 and sends `rounds` of datagrams at it.
fn listen_with(extra_junk: bool) -> serde_json::Value {
    let p = free_udp_port().to_string();
    let child = std::process::Command::new(assert_cmd::cargo::cargo_bin("airscout-cli"))
        .args([
            "wintest-sim",
            "--addr",
            LO,
            "--bind",
            LO,
            "--port",
            &p,
            "--listen",
            "2",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let tx = UdpSocket::bind("127.0.0.1:0").unwrap();
    let dest = format!("{LO}:{p}");
    let start = Instant::now();
    std::thread::sleep(Duration::from_millis(300));
    while start.elapsed() < Duration::from_millis(1500) {
        if extra_junk {
            tx.send_to(&[], &dest).unwrap();
            tx.send_to(b"abc", &dest).unwrap();
            tx.send_to(&vec![0xFFu8; 9000], &dest).unwrap();
        }
        tx.send_to(&asnearest(), &dest).unwrap();
        std::thread::sleep(Duration::from_millis(100));
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn listen_reports_replies_not_own_echo() {
    let v = listen_with(false);
    let r = v["received"].as_array().unwrap();
    assert!(!r.is_empty(), "{v}");
    for e in r {
        assert_eq!(e["kind"], "ASNEAREST", "{e}");
        assert_eq!(e["src"], "AS");
        assert_eq!(e["checksum_ok"], true);
    }
}

#[test]
fn listen_shares_port_with_receiver() {
    // Bind port 0 shared and read the port back: picking a free port first
    // and binding it later races with parallel tests that grab the same
    // ephemeral port without SO_REUSEADDR.
    let _held = as_wintest::socket::bind_shared(format!("{LO}:0").parse().unwrap()).unwrap();
    let p = _held.local_addr().unwrap().port();
    let out = sim(&[
        "--addr",
        LO,
        "--bind",
        LO,
        "--port",
        &p.to_string(),
        "--listen",
        "1",
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn listen_survives_garbage() {
    let v = listen_with(true);
    let r = v["received"].as_array().unwrap();
    assert!(r.iter().any(|e| e.get("error").is_some()), "{v}");
    assert!(r.iter().any(|e| e["kind"] == "ASNEAREST"), "{v}");
}
