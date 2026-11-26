//! The OmniRig COM backend on Windows. The hosted CI machine has no OmniRig installed, so the
//! open fails and the rig reports NoCat; the roundtrip runs by hand where OmniRig is installed.
#![cfg(windows)]

use as_cat::{omnirig_opener as opener, Doppler, OmniRig, Rig, RigStatus};

#[test]
fn com_open_without_omnirig_reports_nocat() {
    let mut r = OmniRig::new(opener(), 1);
    assert_eq!(r.get_status(Doppler::None), RigStatus::NoCat);
    let e = r.take_error().unwrap();
    assert!(e.starts_with("Error while initializing OmniRig: "), "{e}");
    // The open is retried on the next call and fails the same way.
    assert_eq!(r.get_status(Doppler::None), RigStatus::NoCat);
}

#[test]
#[ignore = "needs an installed OmniRig"]
fn omnirig_installed_roundtrip() {
    let mut r = OmniRig::new(opener(), 1);
    let st = r.get_status(Doppler::None);
    assert!(
        matches!(st, RigStatus::Online | RigStatus::Offline),
        "{st:?} {:?}",
        r.take_error()
    );
    let v = r.ident().cat_version;
    assert!(v == "OmniRig V1.19" || v == "OmniRig V2.x", "{v}");
}
