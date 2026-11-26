//! Port of `ScoutBase.Core/Callsign.cs`: callsign check, suffix/prefix cutting, country prefix.
//!
//! `WCCheck.cs` duplicates these functions and is never called upstream, so it is not ported.

use std::sync::OnceLock;

/// (prefix, name, start, stop) as in the C# `PrefixList`, before sorting.
const PREFIXES: &[(&str, &str, &str, &str)] = &[
    ("C3", "Andorra", "C3", "C3"),
    ("OE", "Austria", "OE", "OE"),
    ("ON", "Belgium", "ON", "OT"),
    ("LZ", "Bulgaria", "LZ", "LZ"),
    ("EA8", "Canary Islands", "EA8", "EA8"),
    ("EA8", "Canary Islands", "EH8", "EH8"),
    ("SV9", "Crete", "SV9", "SV9"),
    ("5B", "Cyprus", "5B", "5B"),
    ("5B", "Cyprus", "C4", "C4"),
    ("5B", "Cyprus", "H2", "H2"),
    ("5B", "Cyprus", "P3", "P4"),
    ("OK", "Czechia", "OK", "OL"),
    ("OM", "Slovakia", "OM", "OM"),
    ("OZ", "Denmark", "OU", "OZ"),
    ("EA", "Spain", "AM", "AO"),
    ("EA", "Spain", "EA", "EH"),
    ("DL", "Germany", "DA", "DR"),
    ("EI", "Ireland", "EI", "EJ"),
    ("F", "France", "F", "F"),
    ("F", "France", "TP", "TP"),
    ("F", "France", "TM", "TM"),
    ("G", "England", "G", "G"),
    ("G", "England", "M", "M"),
    ("G", "England", "2E", "2E"),
    ("HA", "Hungaria", "HA", "HA"),
    ("HA", "Hungaria", "HG", "HG"),
    ("HB9", "Switzerland", "HB1", "HB9"),
    ("HB9", "Switzerland", "HE", "HE"),
    ("HB0", "Liechtenstein", "HB0", "HB0"),
    ("SP", "Poland", "HF", "HF"),
    ("SP", "Poland", "SP", "SP"),
    ("SP", "Poland", "SO", "SO"),
    ("SP", "Poland", "SQ", "SQ"),
    ("SP", "Poland", "SN", "SN"),
    ("SV", "Greece", "J4", "J4"),
    ("SV", "Greece", "SV", "SV"),
    ("LA", "Norway", "LA", "LN"),
    ("LX", "Luxembuorg", "LX", "LX"),
    ("PA", "Netherlands", "PA", "PI"),
    ("I", "Italy", "I0", "I9"),
    ("I", "Italy", "IA", "IZ"),
    ("YU", "Yugoslavia", "YU1", "YU1"),
    ("YU", "Yugoslavia", "YU6", "YU9"),
    ("YU", "Yugoslavia", "YT", "YT"),
    ("YU", "Yugoslavia", "YZ", "YZ"),
    ("YU", "Yugoslavia", "4N", "4N"),
    ("9A", "Croatia", "9A", "9A"),
    ("9A", "Croatia", "YU2", "YU2"),
    ("S5", "Slovenia", "S5", "S5"),
    ("S5", "Slovenia", "YU3", "YU3"),
    ("T9", "Bosnia-Herzegovina", "T9", "T9"),
    ("T9", "Bosnia-Herzegovina", "YU4", "YU4"),
    ("Z3", "Mazedonia", "Z3", "Z3"),
    ("Z3", "Mazedonia", "YU5", "YU5"),
    ("SM", "Sweden", "SA", "SM"),
    ("HV", "Vatikan", "HV", "HV"),
    ("EA6", "Balearic Islands", "EA6", "EA6"),
    ("EA6", "Balearic Islands", "EH6", "EH6"),
    ("GD", "Isle Of Man", "GD", "GD"),
    ("GI", "Northern Ireland", "GI", "GI"),
    ("GJ", "Jersey", "GJ", "GJ"),
    ("GM", "Scotland", "GM", "GM"),
    ("GW", "Wales", "GW", "GW"),
    ("IS", "Sardinia", "IS", "IS"),
    ("TK", "Corsica", "TK", "TK"),
    ("LY", "Lithuania", "LY", "LY"),
    ("ER", "Moldavia", "ER", "ER"),
    ("ES", "Estonia", "ES", "ES"),
    ("EV", "Byelorussia", "EV", "EV"),
    ("EV", "Byelorussia", "EW", "EW"),
    ("UA", "European Russia", "UA", "UA"),
    ("UA", "European Russia", "UA", "RA"),
    ("UA2", "Kaliningrad", "UA2", "UA2"),
    ("UA2", "Kaliningrad", "RA2", "RA2"),
    ("UA2", "Kaliningrad", "RN2", "RN2"),
    ("UA2", "Kaliningrad", "RK2", "RK2"),
    ("YO", "Romania", "YO", "YR"),
    ("ZA", "Albania", "ZA", "ZA"),
    ("ZB4", "Gibraltar", "ZB4", "ZB4"),
    ("9H", "Malta", "9H", "9H"),
    ("TA", "Turkey", "TA", "TA"),
    ("T7", "San Marino", "T7", "T7"),
];

/// Prefix table sorted like `LengthComparer`: longest `start` first, then by `start`.
fn sorted_prefixes() -> &'static [(&'static str, &'static str, &'static str, &'static str)] {
    static S: OnceLock<Vec<(&str, &str, &str, &str)>> = OnceLock::new();
    S.get_or_init(|| {
        let mut v = PREFIXES.to_vec();
        v.sort_by(|a, b| b.2.len().cmp(&a.2.len()).then_with(|| a.2.cmp(b.2)));
        v
    })
}

/// `Callsign.Cut`: removes a portable suffix (`/P`, `/MM`, `/QRP`: up to 3 chars after the last
/// `/`) and a country prefix (`PA/`). The original's `Trim().ToUpper()` result is discarded, so
/// the input is *not* trimmed or upper-cased. Inputs that make the C# `Remove` throw give `""`.
pub fn cut(call: &str) -> String {
    let mut c: Vec<char> = call.chars().collect();
    let idx = |c: &[char]| c.iter().position(|&x| x == '/').map_or(-1, |i| i as i64);
    if idx(&c) >= 0 {
        // cut the back part
        let i = idx(&c);
        if i >= c.len() as i64 - 4 {
            c.truncate(i as usize);
        }
        // cut the front part
        let i = idx(&c);
        if i >= 0 {
            c.drain(..=i as usize);
        }
        // cut the back part again; with no '/' left and <= 3 chars C# calls Remove(-1, ..) and throws
        let i = idx(&c);
        if i >= c.len() as i64 - 4 {
            if i < 0 {
                return String::new();
            }
            c.truncate(i as usize);
        }
    }
    c.into_iter().collect()
}

/// `Callsign.Check`: plausibility check of a callsign (with optional `/` additions).
pub fn check(call: &str) -> bool {
    if call.is_empty() {
        return false;
    }
    let call = call.trim().to_uppercase();
    if !call
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '/')
    {
        return false;
    }
    if call.starts_with('/') || call.ends_with('/') {
        return false;
    }
    if call.matches('/').count() > 2 {
        return false;
    }
    let c: Vec<char> = cut(&call).chars().collect();
    if c.len() < 3 {
        return false;
    }
    // indexes past the end throw in C# and are caught -> false
    let letter = |i: usize| c.get(i).is_some_and(|x| x.is_alphabetic());
    let number = |i: usize| c.get(i).is_some_and(|x| x.is_numeric());
    if number(0) {
        // 9A5O
        return letter(1) && number(2);
    }
    if letter(1) {
        // DL0GTH
        return number(2);
    }
    // G7RAU, T91CO
    letter(2) || letter(3)
}

/// `Callsign.Prefix`: country prefix of a call, `"???"` if unknown.
pub fn prefix(call: &str) -> String {
    let s: Vec<char> = cut(call).to_uppercase().chars().collect();
    for (prefix, _, start, stop) in sorted_prefixes() {
        let n = start.chars().count();
        // C#: S = S.Substring(0, n) throws for short calls -> entry skipped. Truncation is
        // cumulative in C#, but the list is sorted by decreasing length, so this is the same.
        if s.len() < n {
            continue;
        }
        let head: String = s[..n].iter().collect();
        if head.as_str() >= *start && head.as_str() <= *stop {
            return prefix.to_string();
        }
    }
    "???".to_string()
}
