//! Port of `AirScout.Aircrafts/PlaneInfoChecker.cs`. Database lookups go through [`Lookup`].

use crate::PlaneInfo;
use chrono::{DateTime, Utc};

/// The aircraft-database queries the checker and converter need (C# `AircraftData.Database`).
/// A lookup that fails counts as "not found", as the C# code catches all exceptions.
pub trait Lookup {
    /// `AirlineFindByICAO(icao) != null`.
    fn airline_by_icao(&self, icao: &str) -> bool;
    /// `AirlineFindByIATA(iata) != null`.
    fn airline_by_iata(&self, iata: &str) -> bool;
    /// `AircraftRegistrationFindByReg(reg) != null`.
    fn registration_by_reg(&self, reg: &str) -> bool;
    /// `AirportFindByICAO(icao) != null`.
    fn airport_by_icao(&self, icao: &str) -> bool;
    /// `AircraftTypeFindByICAO(icao) != null`.
    fn type_by_icao(&self, icao: &str) -> bool;
    /// `AircraftRegistrationMinLength`.
    fn registration_min_len(&self) -> usize;
    /// `AircraftTypeICAOMinLength`.
    fn type_icao_min_len(&self) -> usize;
}

/// The prefix search of `AircraftRegistrationFindByReg` (`AircraftDatabase.cs:1986-2013`): try
/// `reg[..i]` for `i` from two characters past the `-` down to the character before it; `N` +
/// digit becomes `N-…`. `exists(prefix)` asks the database. Returns the prefix found.
///
/// Where C# throws (`"N"` alone: `reg[1]`; no `-`: `Substring(0, -1)` after trying `""`), this
/// returns `Ok(None)` (logic-findings #17).
pub fn find_reg_prefix<E>(
    reg: &str,
    mut exists: impl FnMut(&str) -> Result<bool, E>,
) -> Result<Option<String>, E> {
    let mut reg = reg.trim().to_uppercase();
    if reg.is_empty() {
        return Ok(None);
    }
    if reg.starts_with('N') {
        match reg.chars().nth(1) {
            Some(c) if c.is_ascii_digit() => reg = format!("N-{}", &reg[1..]),
            _ => return Ok(None),
        }
    }
    let chars: Vec<char> = reg.chars().collect();
    let Some(dash) = chars.iter().position(|c| *c == '-') else {
        // C#: stop = -2, i = 0 tries "" first, then throws on Substring(0, -1)
        return Ok(exists("")?.then(String::new));
    };
    if dash == 0 {
        return Ok(None);
    }
    let stop = dash - 1;
    let mut i = (stop + 2).min(chars.len());
    loop {
        let prefix: String = chars[..i].iter().collect();
        if exists(&prefix)? {
            return Ok(Some(prefix));
        }
        if i == stop {
            return Ok(None);
        }
        i -= 1;
    }
}

/// `s.Replace("\"", "").ToUpper().Trim()`.
pub(crate) fn norm_upper(s: &str) -> String {
    s.replace('"', "").to_uppercase().trim().to_string()
}

/// String length as C# counts it (UTF-16 code units).
pub(crate) fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `Check_Hex`: exactly 6 hex digits (quotes stripped, case-insensitive).
pub fn hex(s: &str) -> bool {
    let h = norm_upper(s);
    len16(&h) == 6 && h.chars().all(|c| c.is_ascii_hexdigit())
}

/// `Check_Call`. A callsign whose third character is a digit must start with a known IATA
/// airline code, and (always) its first three characters must be a known ICAO airline. So
/// IATA-style calls (`LH123`) never pass (logic-findings #19).
pub fn call(s: &str, l: &dyn Lookup) -> bool {
    if s.trim().is_empty() || s.contains("[unknown]") {
        return false;
    }
    let c = norm_upper(s);
    let ch: Vec<char> = c.chars().collect();
    if len16(&c) < 4 || ch.len() < 3 || c.contains('-') {
        return false;
    }
    if ch[2].is_numeric() && !l.airline_by_iata(&ch[..2].iter().collect::<String>()) {
        return false;
    }
    if !ch[0].is_alphabetic() || !ch[1].is_alphabetic() {
        return false;
    }
    l.airline_by_icao(&ch[..3].iter().collect::<String>())
}

/// `Check_Reg`: long enough and contains `-` or starts with `N` or `H`.
pub fn reg(s: &str, l: &dyn Lookup) -> bool {
    if s.trim().is_empty() || s.contains("[unknown]") {
        return false;
    }
    let r = norm_upper(s);
    if len16(&r) < l.registration_min_len() + 1 {
        return false;
    }
    r.contains('-') || r.starts_with('N') || r.starts_with('H')
}

/// `Check_Lat`.
pub fn lat(v: f64) -> bool {
    !v.is_nan() && (-90.0..=90.0).contains(&v)
}

/// `Check_Lon`.
pub fn lon(v: f64) -> bool {
    !v.is_nan() && (-180.0..=180.0).contains(&v)
}

/// `Check_Alt` [ft].
pub fn alt(v: f64) -> bool {
    !v.is_nan() && (0.0..=100000.0).contains(&v)
}

/// `Check_Track`: `0 <= t < 360`.
pub fn track(v: f64) -> bool {
    !v.is_nan() && (0.0..360.0).contains(&v)
}

/// `Check_Speed` [kts].
pub fn speed(v: f64) -> bool {
    !v.is_nan() && (0.0..=800.0).contains(&v)
}

/// `Check_Type`: alphanumeric and at least as long as the shortest ICAO type code.
pub fn type_code(s: &str, l: &dyn Lookup) -> bool {
    if s.trim().is_empty() || s.contains("[unknown]") {
        return false;
    }
    let t = norm_upper(s);
    t.chars().all(char::is_alphanumeric) && len16(&t) >= l.type_icao_min_len()
}

/// `Check_Manufacturer`, `Check_Model`, `Check_Airport`: not empty.
pub fn not_empty(s: &str) -> bool {
    !s.is_empty()
}

/// `Check(PlaneInfo)`: the mandatory data. Looser than the field checks (track up to 360, speed
/// up to 5000), and NaN positions pass. `DateTime::MIN_UTC`/`MAX_UTC` stand for C#
/// `DateTime.MinValue`/`MaxValue`.
pub fn plane(p: &PlaneInfo) -> bool {
    if crate::convert::to_hex(&p.hex).is_none_or(|h| h.is_empty()) {
        return false;
    }
    if crate::convert::call_shape(&p.call).is_none_or(|c| c.is_empty()) {
        return false;
    }
    if p.time == DateTime::<Utc>::MIN_UTC || p.time == DateTime::<Utc>::MAX_UTC {
        return false;
    }
    !(p.lat > 90.0
        || p.lat < -90.0
        || p.lon > 180.0
        || p.lon < -180.0
        || p.alt_ft < 0.0
        || p.alt_ft > 100000.0
        || p.track > 360.0
        || p.track < 0.0
        || p.speed_kts < 0.0
        || p.speed_kts > 5000.0)
}
