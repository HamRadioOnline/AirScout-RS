//! Mapping between `RigMode` and hamlib mode names.

use crate::RigMode;

const PAIRS: [(&str, RigMode); 8] = [
    ("CW", RigMode::Cw),
    ("CWR", RigMode::CwR),
    ("LSB", RigMode::Lsb),
    ("USB", RigMode::Usb),
    ("PKTLSB", RigMode::Dig),
    ("PKTUSB", RigMode::DigR),
    ("AM", RigMode::Am),
    ("FM", RigMode::Fm),
];

/// Hamlib mode name to `RigMode`; anything else is `None` (as ScoutBaseRig `GetMode`).
pub fn mode_from_hamlib(s: &str) -> RigMode {
    PAIRS
        .iter()
        .find(|(n, _)| *n == s)
        .map_or(RigMode::None, |(_, m)| *m)
}

/// `RigMode` to hamlib mode name; `None` and `Other` have no name.
pub fn mode_to_hamlib(m: RigMode) -> Option<&'static str> {
    PAIRS.iter().find(|(_, pm)| *pm == m).map(|(n, _)| *n)
}
