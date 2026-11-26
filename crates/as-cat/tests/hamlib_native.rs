//! Needs libhamlib; run by hand: `cargo test -p as-cat --features hamlib -- --ignored`.
#![cfg(all(feature = "hamlib", any(target_os = "linux", target_os = "macos")))]

use std::time::Duration;

use as_cat::{Backend, FlowControl, NativeHamlib, Parity, SerialConfig, StopBits};

#[test]
#[ignore = "needs libhamlib installed"]
fn native_dummy_rig_roundtrip() {
    let serial = SerialConfig {
        port_name: String::new(),
        baudrate: 9600,
        data_bits: 8,
        parity: Parity::None,
        stop_bits: StopBits::One,
        rts: FlowControl::Low,
        dtr: FlowControl::Low,
    };
    // Model 1 is the Dummy rig.
    let mut rig = NativeHamlib::new(1, serial, Duration::from_millis(500));
    rig.connect().expect("connect");
    rig.set_freq(144_300_000).expect("set_freq");
    assert_eq!(rig.get_freq(), Ok(144_300_000));
}
