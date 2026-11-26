use std::time::Duration;

use as_cat::hamlib_conf::{hamlib_conf, hamlib_error};
use as_cat::{BackendError, FlowControl, Parity, SerialConfig, StopBits};

fn base() -> SerialConfig {
    SerialConfig {
        port_name: String::new(),
        baudrate: 9600,
        data_bits: 8,
        parity: Parity::None,
        stop_bits: StopBits::One,
        rts: FlowControl::Low,
        dtr: FlowControl::Low,
    }
}

fn pairs(v: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    v.iter().map(|(k, s)| (*k, s.to_string())).collect()
}

#[test]
fn hamlib_conf_defaults() {
    assert_eq!(
        hamlib_conf(&base(), Duration::from_millis(500)),
        pairs(&[
            ("rig_pathname", ""),
            ("serial_speed", "9600"),
            ("data_bits", "8"),
            ("stop_bits", "1"),
            ("serial_parity", "None"),
            ("serial_handshake", "None"),
            ("rts_state", "OFF"),
            ("dtr_state", "OFF"),
            ("timeout", "500"),
        ])
    );
}

#[test]
fn hamlib_conf_variants() {
    let c = SerialConfig {
        stop_bits: StopBits::One5,
        parity: Parity::Mark,
        rts: FlowControl::Handshake,
        dtr: FlowControl::High,
        ..base()
    };
    let v = hamlib_conf(&c, Duration::from_millis(500));
    for p in [
        ("stop_bits", "2"),
        ("serial_parity", "Mark"),
        ("serial_handshake", "Hardware"),
        ("rts_state", "Unset"),
        ("dtr_state", "ON"),
    ] {
        assert!(
            v.contains(&(p.0, p.1.to_string())),
            "missing {p:?} in {v:?}"
        );
    }
}

#[test]
fn hamlib_error_mapping() {
    assert_eq!(hamlib_error(-5), BackendError::Timeout);
    assert_eq!(
        hamlib_error(-6),
        BackendError::Lost("hamlib: I/O error (-6)".into())
    );
    assert_eq!(hamlib_error(-11), BackendError::Rejected(-11));
    assert_eq!(hamlib_error(-1), BackendError::Rejected(-1));
}

#[test]
fn hamlib_conf_fatal_keys() {
    use as_cat::hamlib_conf::{hamlib_conf_is_fatal, RIG_DEBUG_ERR};
    let v = hamlib_conf(&base(), Duration::from_millis(500));
    let fatal: Vec<_> = v
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| hamlib_conf_is_fatal(k))
        .collect();
    assert_eq!(fatal, ["rig_pathname", "timeout"]);
    // hamlib rig.h: NONE=0, BUG=1, ERR=2.
    assert_eq!(RIG_DEBUG_ERR, 2);
}
