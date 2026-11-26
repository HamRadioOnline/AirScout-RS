//! Windows smoke test of the DDEML binding: without HRD Rotator running, connecting fails cleanly.
#![cfg(windows)]

use as_dde::{DdeLink, Ddeml, SERVICE, TOPIC};

#[test]
fn connect_without_hrd_fails_cleanly() {
    let mut d = Ddeml::new();
    match d.connect(SERVICE, TOPIC) {
        Err(t) => assert!(t.starts_with("DDE: HRDRotator/Position: "), "{t}"),
        Ok(()) => panic!("connected to an HRD Rotator; the test needs none running"),
    }
    assert!(!d.is_connected());
    // without a conversation a poke fails and a disconnect is a no-op
    assert!(!d.poke(
        as_dde::ITEM,
        b"SET-AZ:10.0",
        as_dde::CF_TEXT,
        as_dde::POKE_TIMEOUT
    ));
    d.disconnect();
    drop(d);
}
