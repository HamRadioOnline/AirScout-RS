#[path = "common/fake_rigctld.rs"]
mod fake_rigctld;

use std::io::Cursor;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use as_cat::rigctld_proto::{parse_flag, parse_freq, parse_rprt, read_line, MAX_LINE};
use as_cat::{
    rigctld_rig, Backend, BackendError, Doppler, DopplerSession, Rig, RigMode, Rigctld,
    RigctldConfig, MAX_FREQ_HZ,
};
use fake_rigctld::{Behaviour, FakeRigctld, RigState};

fn cfg(f: &FakeRigctld, ms: u64) -> RigctldConfig {
    RigctldConfig {
        host: "127.0.0.1".into(),
        port: f.addr.port(),
        timeout: Duration::from_millis(ms),
    }
}

fn client(f: &FakeRigctld) -> Rigctld {
    let mut c = Rigctld::new(cfg(f, 500));
    c.connect().expect("connect");
    c
}

#[test]
fn commands_are_byte_exact() {
    let f = FakeRigctld::start(RigState::default());
    let mut c = client(&f);
    c.set_freq(144_300_000).unwrap();
    c.set_mode("USB").unwrap();
    c.set_split(true).unwrap();
    c.set_split(false).unwrap();
    c.set_split_freq(144_300_500).unwrap();
    c.set_split_mode("PKTUSB").unwrap();
    c.set_rit_on(true).unwrap();
    c.get_freq().unwrap();
    c.get_mode().unwrap();
    c.get_split().unwrap();
    c.get_split_freq().unwrap();
    c.get_rit_on().unwrap();
    let want: &[u8] = b"F 144300000\nM USB -1\nS 1 VFOB\nS 0 VFOA\nI 144300500\nX PKTUSB -1\n\
U RIT 1\nf\nm\ns\ni\nu RIT\n";
    assert_eq!(f.raw(), want);
}

#[test]
fn replies_parsed() {
    let f = FakeRigctld::start(RigState {
        split: true,
        split_freq: 144_300_500,
        ..RigState::default()
    });
    let mut c = client(&f);
    assert_eq!(c.get_freq(), Ok(144_300_000));
    assert_eq!(c.get_mode(), Ok("USB".to_string()));
    assert_eq!(c.get_split(), Ok(true));
    assert_eq!(c.get_split_freq(), Ok(144_300_500));
    assert_eq!(c.get_rit_on(), Ok(false));
}

#[test]
fn decimal_and_crlf_freq_accepted() {
    let f = FakeRigctld::start(RigState::default());
    f.set_behaviour(Behaviour::Fixed(b"144300000.000000\r\n".to_vec()));
    assert_eq!(client(&f).get_freq(), Ok(144_300_000));
    f.set_behaviour(Behaviour::Fixed(b"144300000.5\n".to_vec()));
    assert_eq!(client(&f).get_freq(), Ok(144_300_001));
}

#[test]
fn rprt_error_is_rejected() {
    let f = FakeRigctld::start(RigState::default());
    f.set_behaviour(Behaviour::Fixed(b"RPRT -11\n".to_vec()));
    let mut c = client(&f);
    assert_eq!(c.get_freq(), Err(BackendError::Rejected(-11)));
    assert_eq!(c.set_freq(1), Err(BackendError::Rejected(-11)));
    assert!(c.is_connected());
}

#[test]
fn set_needs_rprt_zero_and_get_rejects_rprt_zero() {
    let f = FakeRigctld::start(RigState::default());
    f.set_behaviour(Behaviour::Fixed(b"144300000\n".to_vec()));
    let mut c = client(&f);
    assert!(matches!(c.set_freq(1), Err(BackendError::Lost(_))));
    assert!(!c.is_connected());
    f.set_behaviour(Behaviour::Fixed(b"RPRT 0\n".to_vec()));
    let mut c = client(&f);
    assert!(matches!(c.get_freq(), Err(BackendError::Lost(_))));
    assert!(!c.is_connected());
}

#[test]
fn silent_server_times_out_and_reconnects() {
    let f = FakeRigctld::start(RigState::default());
    f.set_behaviour(Behaviour::Silent);
    let mut c = Rigctld::new(cfg(&f, 300));
    c.connect().unwrap();
    let t = Instant::now();
    assert_eq!(c.get_freq(), Err(BackendError::Timeout));
    assert!(t.elapsed() < Duration::from_secs(3));
    assert!(!c.is_connected());
    f.set_behaviour(Behaviour::Normal);
    c.connect().unwrap();
    assert_eq!(c.get_freq(), Ok(144_300_000));
    assert_eq!(f.accepts(), 2);
}

#[test]
fn eof_is_lost() {
    let f = FakeRigctld::start(RigState::default());
    f.set_behaviour(Behaviour::CloseAfterRead);
    let mut c = client(&f);
    assert!(matches!(c.get_freq(), Err(BackendError::Lost(_))));
    assert!(!c.is_connected());
}

#[test]
fn long_line_is_lost() {
    let f = FakeRigctld::start(RigState::default());
    let mut b = vec![b'x'; 300];
    b.push(b'\n');
    f.set_behaviour(Behaviour::Fixed(b));
    let mut c = client(&f);
    assert!(matches!(c.get_freq(), Err(BackendError::Lost(_))));
}

#[test]
fn garbage_freq_is_lost() {
    let f = FakeRigctld::start(RigState::default());
    f.set_behaviour(Behaviour::Fixed(b"abc\n".to_vec()));
    let mut c = client(&f);
    assert!(matches!(c.get_freq(), Err(BackendError::Lost(_))));
    assert!(!c.is_connected());
}

#[test]
fn refused_connect_is_nocat() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    let mut c = Rigctld::new(RigctldConfig {
        host: "127.0.0.1".into(),
        port: p,
        timeout: Duration::from_millis(500),
    });
    let t = Instant::now();
    match c.connect() {
        Err(BackendError::NoCat(t)) => {
            assert!(t.starts_with(&format!("rigctld 127.0.0.1:{p}: ")), "{t}")
        }
        other => panic!("{other:?}"),
    }
    assert!(t.elapsed() < Duration::from_secs(10));
}

#[test]
fn ops_without_connection_are_lost() {
    let f = FakeRigctld::start(RigState::default());
    let mut c = Rigctld::new(cfg(&f, 500));
    assert_eq!(
        c.get_freq(),
        Err(BackendError::Lost("not connected".into()))
    );
    assert_eq!(f.accepts(), 0);
}

#[test]
fn ident_text() {
    let c = Rigctld::new(RigctldConfig {
        host: "localhost".into(),
        port: 4532,
        timeout: Duration::from_millis(500),
    });
    let i = c.ident();
    assert_eq!(i.cat_version, "Hamlib rigctld (net protocol)");
    assert_eq!(i.rig_type, "[rigctld] localhost:4532");
}

#[test]
fn proto_helpers() {
    assert_eq!(parse_rprt("RPRT -11"), Some(-11));
    assert_eq!(parse_rprt("RPRT x"), None);
    assert_eq!(parse_rprt("144"), None);
    assert_eq!(parse_freq("-0.5"), Ok(-1));
    assert!(parse_freq("NaN").is_err());
    assert!(parse_freq("inf").is_err());
    assert_eq!(parse_freq("100000000000"), Ok(MAX_FREQ_HZ));
    for t in [
        "1e300",
        "9223372036854775807",
        "100000000001",
        "100000000000.6",
    ] {
        assert!(matches!(parse_freq(t), Err(BackendError::Lost(_))), "{t}");
    }
    assert_eq!(parse_flag("1"), Ok(true));
    assert!(parse_flag("2").is_err());
    let mut ok = vec![b'y'; MAX_LINE];
    ok.extend_from_slice(b"\r\n");
    assert_eq!(read_line(&mut Cursor::new(ok)).unwrap().len(), MAX_LINE);
    let mut long = vec![b'y'; MAX_LINE + 1];
    long.push(b'\n');
    assert!(read_line(&mut Cursor::new(long)).is_err());
    assert!(read_line(&mut Cursor::new(b"\xff\n".to_vec())).is_err());
    assert!(read_line(&mut Cursor::new(b"no newline".to_vec())).is_err());
}

#[test]
fn parse_never_panics_on_random_bytes() {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (x >> 33) as usize
    };
    for _ in 0..10_000 {
        let len = next() % 400;
        let bytes: Vec<u8> = (0..len)
            .map(|_| match next() % 4 {
                0 => b'\n',
                1 => b"0123456789.-eE+"[next() % 15],
                _ => (next() % 256) as u8,
            })
            .collect();
        let mut cur = Cursor::new(bytes);
        if let Ok(line) = read_line(&mut cur) {
            assert!(line.len() <= MAX_LINE);
            if let Ok(f) = parse_freq(&line) {
                let _ = f; // an i64 is finite by construction; parse_freq rejects NaN/inf
            }
            let _ = parse_flag(&line);
            let _ = parse_rprt(&line);
        }
    }
}

#[test]
fn golden_doppler_session_over_rigctld() {
    let f = FakeRigctld::start(RigState::default());
    let mut rig = rigctld_rig(cfg(&f, 500));
    let mut d = DopplerSession::default();
    assert!(d.enter(&mut rig, Doppler::A));
    assert!(rig.set_split_mode(144_300_312, 144_300_000, RigMode::None));
    d.leave(&mut rig);
    let want = [
        "f",
        "s",
        "m",
        "s",
        "u RIT",
        "f",
        "s",
        "f",
        "S 1 VFOB",
        "F 144300000",
        "I 144300000",
        "M USB -1",
        "X USB -1",
        "S 1 VFOB",
        "F 144300312",
        "I 144300000",
        "M USB -1",
        "S 0 VFOA",
        "U RIT 0",
        "F 144300000",
        "s",
        "F 144300000",
        "S 0 VFOA",
        "F 144300000",
        "M USB -1",
    ];
    assert_eq!(f.lines(), want);
}
