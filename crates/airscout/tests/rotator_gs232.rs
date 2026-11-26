//! GS-232A serial rotator output: commands, reply check, bounded reads, fanout behaviour.

use airscout::rotator_gs232::{
    parse_dotnet_float, query, read_reply, reply_ok, set_command, Gs232, Gs232Output, SerialIo,
    SerialOpen, IO_TIMEOUT, MAX_REPLY,
};
use airscout::track_outputs::{OutputError, OutputGroup, TrackOutput};
use airscout::track_service::{TrackEvent, TrackFanout};
use airscout::tracking::{RotStatus, TrackStep, TrackValues};
use as_aircraft::enums::TrackMode;
use chrono::Utc;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FORMAT_ERR: &str = "[Serial]: Wrong serial data format.";

/// One read call returns bytes, an error, or end of data.
enum Chunk {
    Bytes(Vec<u8>),
    Err(io::ErrorKind),
}

#[derive(Default)]
struct Shared {
    input: VecDeque<Chunk>,
    written: Vec<u8>,
}

struct FakeSerial(Arc<Mutex<Shared>>);

impl Read for FakeSerial {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut s = self.0.lock().unwrap();
        match s.input.pop_front() {
            None => Err(io::ErrorKind::TimedOut.into()),
            Some(Chunk::Err(k)) => Err(k.into()),
            Some(Chunk::Bytes(mut b)) => {
                let n = b.len().min(buf.len());
                buf[..n].copy_from_slice(&b[..n]);
                if n < b.len() {
                    s.input.push_front(Chunk::Bytes(b.split_off(n)));
                }
                Ok(n)
            }
        }
    }
}

impl Write for FakeSerial {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().written.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// An opener that hands out a `FakeSerial` on one shared state and counts its calls.
fn opener(shared: Arc<Mutex<Shared>>) -> (SerialOpen, Arc<AtomicU32>) {
    let n = Arc::new(AtomicU32::new(0));
    let c = n.clone();
    let open: SerialOpen = Arc::new(move |_name, _baud| {
        c.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeSerial(shared.clone())) as Box<dyn SerialIo>)
    });
    (open, n)
}

fn reply(shared: &Arc<Mutex<Shared>>, b: &[u8]) {
    shared
        .lock()
        .unwrap()
        .input
        .push_back(Chunk::Bytes(b.to_vec()));
}

fn written(shared: &Arc<Mutex<Shared>>) -> Vec<u8> {
    shared.lock().unwrap().written.clone()
}

fn out(k: Gs232, shared: &Arc<Mutex<Shared>>) -> (Gs232Output, Arc<AtomicU32>) {
    let (open, n) = opener(shared.clone());
    (Gs232Output::new(k, "/dev/ttyTEST0".into(), 4800, open), n)
}

fn step_ok() -> TrackStep {
    let mut v = TrackValues::new(Utc::now());
    v.my_azimuth = 123.4;
    v.my_elevation = 45.2;
    TrackStep {
        values: Some(v),
        mode: TrackMode::Track,
    }
}

#[test]
fn commands_match_csharp() {
    assert_eq!(set_command(Gs232::Az, 123.4, 0.0), "M123");
    assert_eq!(set_command(Gs232::Az, 359.6, 0.0), "M360");
    assert_eq!(set_command(Gs232::AzEl, 5.0, 45.2), "W005 045");
    assert_eq!(set_command(Gs232::AzEl, 10.0, -5.0), "W010 -005");
    assert_eq!(query(Gs232::Az), "C");
    assert_eq!(query(Gs232::AzEl), "C2");
}

#[test]
fn reply_check() {
    for ok in ["+0123", "+0123+0045", "+0 12", "+01.5", "+01,2", "+0-12"] {
        assert!(reply_ok(ok), "{ok}");
    }
    for bad in ["?0123", "+0", "+012", "+0abc", "+0   ", ""] {
        assert!(!reply_ok(bad), "{bad}");
    }
}

#[test]
fn dotnet_float_forms() {
    assert_eq!(parse_dotnet_float(" 1,234.5e1 "), Some(12345.0));
    assert_eq!(parse_dotnet_float("-.5"), Some(-0.5));
    assert_eq!(parse_dotnet_float("+7."), Some(7.0));
    assert_eq!(parse_dotnet_float("."), None);
    assert_eq!(parse_dotnet_float("1e"), None);
    assert_eq!(parse_dotnet_float("1.2.3"), None);
    // a group separator needs a digit before it
    assert_eq!(parse_dotnet_float(",1"), None);
    assert_eq!(parse_dotnet_float("1,,2"), Some(12.0));
    assert_eq!(parse_dotnet_float("1,.5"), Some(1.5));
    assert_eq!(parse_dotnet_float("inf"), None);
    assert!(parse_dotnet_float("NaN").unwrap().is_nan());
    assert_eq!(parse_dotnet_float("-Infinity"), Some(f64::NEG_INFINITY));
}

#[test]
fn az_exchange_bytes() {
    let sh = Arc::new(Mutex::new(Shared::default()));
    reply(&sh, b"+0123\r");
    let (mut o, _) = out(Gs232::Az, &sh);
    assert_eq!(o.group(), OutputGroup::Serial);
    o.connect().unwrap();
    o.send(123.4, 0.0).unwrap();
    assert_eq!(written(&sh), b"C\rM123\r");
}

#[test]
fn azel_exchange_bytes() {
    let sh = Arc::new(Mutex::new(Shared::default()));
    reply(&sh, b"+0123+0045\r\n");
    let (mut o, _) = out(Gs232::AzEl, &sh);
    o.connect().unwrap();
    o.send(123.0, 45.0).unwrap();
    assert_eq!(written(&sh), b"C2\rW123 045\r");
}

#[test]
fn non_ascii_reply_is_format_error() {
    let sh = Arc::new(Mutex::new(Shared::default()));
    reply(&sh, b"\xAB0123\r");
    let (mut o, _) = out(Gs232::Az, &sh);
    o.connect().unwrap();
    assert_eq!(
        o.send(123.0, 0.0),
        Err(OutputError::NotConnected(FORMAT_ERR.into()))
    );
    assert_eq!(written(&sh), b"C\r");
    assert!(o.is_connected());
}

#[test]
fn long_reply_fails() {
    let sh = Arc::new(Mutex::new(Shared::default()));
    reply(&sh, &[b'1'; 300]);
    let (mut o, _) = out(Gs232::Az, &sh);
    o.connect().unwrap();
    assert!(matches!(o.send(1.0, 0.0), Err(OutputError::Failed(_))));
    assert_eq!(written(&sh), b"C\r");
}

#[test]
fn read_reply_bounds_and_decoding() {
    let far = Instant::now() + Duration::from_secs(10);
    let mut r: &[u8] = b"a\n\xFFb\rrest";
    assert_eq!(read_reply(&mut r, far).unwrap(), "a?b");
    let mut r: &[u8] = &[b'x'; MAX_REPLY];
    // exactly MAX_REPLY bytes then end of data: still no terminator, so not a long reply
    assert!(read_reply(&mut r, far).is_err());
    let mut data = vec![b'x'; MAX_REPLY];
    data.push(b'\r');
    let mut r: &[u8] = &data;
    assert_eq!(read_reply(&mut r, far).unwrap().len(), MAX_REPLY);
    let mut data = vec![b'x'; MAX_REPLY + 1];
    data.push(b'\r');
    let mut r: &[u8] = &data;
    let e = read_reply(&mut r, far).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    assert_eq!(e.to_string(), "reply too long");
}

#[test]
fn read_reply_times_out_at_deadline() {
    struct Slow;
    impl Read for Slow {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            std::thread::sleep(Duration::from_millis(5));
            Err(io::ErrorKind::WouldBlock.into())
        }
    }
    let t = Instant::now();
    let e = read_reply(&mut Slow, t + Duration::from_millis(50)).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::TimedOut);
    assert!(t.elapsed() < Duration::from_secs(5));
    assert_eq!(IO_TIMEOUT, Duration::from_millis(1000));
}

#[test]
fn timeout_closes_and_reopens() {
    let sh = Arc::new(Mutex::new(Shared::default()));
    sh.lock()
        .unwrap()
        .input
        .push_back(Chunk::Err(io::ErrorKind::TimedOut));
    let (o, opens) = out(Gs232::Az, &sh);
    let mut f = TrackFanout::new(vec![Box::new(o)]);
    let ev = f.step(&step_ok());
    assert!(ev.iter().any(|e| matches!(e, TrackEvent::Text(_))));
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    reply(&sh, b"+0123\r");
    let ev = f.step(&step_ok());
    assert!(!ev.iter().any(|e| matches!(e, TrackEvent::Text(_))));
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    // the failed first step wrote only its query; the second step wrote query and set
    assert_eq!(written(&sh), b"C\rC\rM123\r");
}

#[test]
fn timeout_leaves_output_not_connected() {
    let sh = Arc::new(Mutex::new(Shared::default()));
    let (mut o, _) = out(Gs232::Az, &sh);
    o.connect().unwrap();
    assert!(matches!(o.send(1.0, 0.0), Err(OutputError::Failed(_))));
    o.close();
    assert!(!o.is_connected());
}

#[test]
fn open_failure_is_shown_not_counted() {
    let open: SerialOpen = Arc::new(|_, _| Err(io::ErrorKind::NotFound.into()));
    let o = Gs232Output::new(Gs232::Az, "COM13".into(), 4800, open);
    let mut f = TrackFanout::new(vec![Box::new(o)]);
    for i in 0..20 {
        let ev = f.step(&step_ok());
        assert!(
            !ev.iter().any(|e| matches!(e, TrackEvent::Disabled(_))),
            "step {i}"
        );
        assert!(
            ev.iter()
                .any(|e| matches!(e, TrackEvent::Text(t) if t.starts_with("Track error: "))),
            "step {i}: {ev:?}"
        );
        assert!(!ev.contains(&TrackEvent::Rot(RotStatus::Error)));
    }
}

#[test]
fn serial_disabled_at_sixth_format_error() {
    let sh = Arc::new(Mutex::new(Shared::default()));
    for _ in 0..8 {
        reply(&sh, b"?\r");
    }
    let (o, _) = out(Gs232::Az, &sh);
    let mut f = TrackFanout::new(vec![Box::new(o)]);
    for i in 1..=5 {
        let ev = f.step(&step_ok());
        assert!(
            !ev.iter().any(|e| matches!(e, TrackEvent::Disabled(_))),
            "step {i}"
        );
    }
    let ev = f.step(&step_ok());
    assert!(ev.contains(&TrackEvent::Text("Tracking via Serial disabled.".into())));
    assert!(ev.contains(&TrackEvent::Rot(RotStatus::Error)));
    assert!(ev.contains(&TrackEvent::Disabled(OutputGroup::Serial)));
}
