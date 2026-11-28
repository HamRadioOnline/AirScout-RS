//! Pure hamlib settings mapping (always compiled; no libhamlib needed).

use std::ffi::c_long;
use std::time::Duration;

use crate::backend::BackendError;
use crate::config::{FlowControl, Parity, SerialConfig, StopBits};

pub const RIG_VFO_CURR: u32 = 0x2000_0000;
pub const RIG_VFO_A: u32 = 1;
pub const RIG_VFO_B: u32 = 2;
pub const RIG_PASSBAND_NOCHANGE: c_long = -1;
pub const RIG_DEBUG_ERR: i32 = 2;
/// Token value that `rig_token_lookup` returns for an unknown name.
pub const RIG_CONF_END: c_long = 0;

const RIG_ETIMEOUT: i32 = -5;
const RIG_EIO: i32 = -6;

fn level(f: FlowControl) -> &'static str {
    match f {
        FlowControl::Low => "OFF",
        FlowControl::High => "ON",
        FlowControl::Handshake => "Unset",
    }
}

/// `rig_set_conf` name/value pairs for a serial rig, in application order.
pub fn hamlib_conf(c: &SerialConfig, timeout: Duration) -> Vec<(&'static str, String)> {
    let stop_bits = match c.stop_bits {
        StopBits::One => "1",
        StopBits::One5 | StopBits::Two => "2",
    };
    let parity = match c.parity {
        Parity::None => "None",
        Parity::Odd => "Odd",
        Parity::Even => "Even",
        Parity::Mark => "Mark",
        Parity::Space => "Space",
    };
    let handshake = if c.rts == FlowControl::Handshake {
        "Hardware"
    } else {
        "None"
    };
    vec![
        ("rig_pathname", c.port_name.clone()),
        ("serial_speed", c.baudrate.to_string()),
        ("data_bits", c.data_bits.to_string()),
        ("stop_bits", stop_bits.to_string()),
        ("serial_parity", parity.to_string()),
        ("serial_handshake", handshake.to_string()),
        ("rts_state", level(c.rts).to_string()),
        ("dtr_state", level(c.dtr).to_string()),
        ("timeout", timeout.as_millis().to_string()),
    ]
}

/// Whether a failing `rig_set_conf` for this key aborts connect. The serial
/// keys are best effort: hamlib rejects them (`-RIG_EINVAL`) on rigs whose
/// port is not serial (Dummy, network, USB).
pub fn hamlib_conf_is_fatal(name: &str) -> bool {
    matches!(name, "rig_pathname" | "timeout")
}

/// Map a negative hamlib return code to a backend error.
pub fn hamlib_error(code: i32) -> BackendError {
    match code {
        RIG_ETIMEOUT => BackendError::Timeout,
        RIG_EIO => BackendError::Lost("hamlib: I/O error (-6)".into()),
        c => BackendError::Rejected(c),
    }
}
