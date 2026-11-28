//! Port of `AirScout.Aircrafts/PlaneInfoConverter.cs`: parse feed strings into typed values.
//! `None` stands for the C# `null` / `int.MinValue` / `double.MinValue` / `DateTime.MinValue`.
//!
//! The original parses numbers with the current culture (logic-findings #20); the port is
//! culture-invariant (`.` as decimal separator), which is what the golden harness runs with.

use crate::check::{len16, norm_upper, Lookup};
use chrono::{DateTime, NaiveDateTime, Utc};

fn is_int(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_digit())
}

fn strip_trim(s: &str) -> String {
    s.replace('"', "").trim().to_string()
}

/// `Convert.ToDouble` with the invariant culture: `NumberStyles.Float | AllowThousands`
/// (leading/trailing white space, sign, decimal point, exponent, `,` group separators in the
/// integer part).
fn dotnet_double(s: &str) -> Option<f64> {
    let s = s.trim();
    let int_end = s.find(['.', 'e', 'E']).unwrap_or(s.len());
    let (int_part, rest) = s.split_at(int_end);
    if rest.contains(',') {
        return None;
    }
    let digits = int_part.trim_start_matches(['+', '-']);
    if digits.starts_with(',') {
        return None;
    }
    let t = format!("{}{}", int_part.replace(',', ""), rest);
    if !t
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'))
    {
        return None;
    }
    t.parse::<f64>().ok()
}

/// `To_UTC`: unix seconds when at least 10 digits, else a date/time string (taken as UTC).
/// Supported string forms: ISO 8601 / RFC 3339 and `yyyy-MM-dd HH:mm:ss`; C# `DateTime.TryParse`
/// accepts more (culture-dependent) forms.
pub fn to_utc(s: &str) -> Option<DateTime<Utc>> {
    let s = strip_trim(s);
    if len16(&s) < 10 {
        return None;
    }
    if is_int(&s) {
        if let Some(t) = s
            .parse::<i64>()
            .ok()
            .and_then(|l| DateTime::from_timestamp(l, 0))
        {
            return Some(t);
        }
    }
    if let Ok(t) = DateTime::parse_from_rfc3339(&s) {
        return Some(t.with_timezone(&Utc));
    }
    for f in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(t) = NaiveDateTime::parse_from_str(&s, f) {
            return Some(t.and_utc());
        }
    }
    None
}

/// `To_Hex`: 6 hex digits, upper case.
pub fn to_hex(s: &str) -> Option<String> {
    let h = norm_upper(s);
    (len16(&h) == 6 && h.chars().all(|c| c.is_ascii_hexdigit())).then_some(h)
}

/// `To_Reg`: shared shape gate with `Check_Reg`, plus a known registration prefix.
pub fn to_reg(s: &str, l: &dyn Lookup) -> Option<String> {
    let r = reg_shape(s, l.registration_min_len())?;
    l.registration_by_reg(&r).then_some(r)
}

/// `To_Flight`: IATA airline code + digits.
pub fn to_flight(s: &str, l: &dyn Lookup) -> Option<String> {
    let f = norm_upper(s);
    let ch: Vec<char> = f.chars().collect();
    if len16(&f) < 3 || ch.len() < 2 {
        return None;
    }
    let iata: String = ch[..2].iter().collect();
    let number: String = ch[2..].iter().collect();
    (is_int(&number) && l.airline_by_iata(&iata)).then_some(f)
}

/// `To_Type`: alphanumeric, long enough, and (with `check_type`) a known ICAO type.
pub fn to_type(s: &str, check_type: bool, l: &dyn Lookup) -> Option<String> {
    let t = norm_upper(s);
    if t.is_empty() || !t.chars().all(char::is_alphanumeric) || len16(&t) < l.type_icao_min_len() {
        return None;
    }
    (!check_type || l.type_by_icao(&t)).then_some(t)
}

/// `To_Radar`: letter, `-`, at least one more character.
pub fn to_radar(s: &str) -> Option<String> {
    let r = norm_upper(s);
    let ch: Vec<char> = r.chars().collect();
    (len16(&r) >= 3 && ch.len() >= 2 && ch[0].is_alphabetic() && ch[1] == '-').then_some(r)
}

/// `To_Route`: `FROM-TO` with both whole ICAO airports known.
pub fn to_route(s: &str, l: &dyn Lookup) -> Option<String> {
    let r = norm_upper(s);
    let ch: Vec<char> = r.chars().collect();
    if len16(&r) < 9 {
        return None;
    }
    let index = ch.iter().position(|c| *c == '-')?;
    if index < 3 || index > ch.len() - 3 {
        return None;
    }
    let from: String = ch[..index].iter().collect();
    let to: String = ch[index + 1..].iter().collect();
    (l.airport_by_icao(&from) && l.airport_by_icao(&to)).then_some(r)
}

/// The shape part of `To_Call` (no airline check): at least 4 characters, no `-`.
pub(crate) fn call_shape(s: &str) -> Option<String> {
    let c = norm_upper(s);
    (len16(&c) >= 4 && !c.contains('-')).then_some(c)
}

pub(crate) enum CallDiscriminator {
    Iata(String),
    Icao(String),
}

pub(crate) fn call_discriminator(call: &str) -> Option<CallDiscriminator> {
    let ch: Vec<char> = call.chars().collect();
    if ch.len() < 3 {
        return None;
    }
    if ch[2].is_ascii_digit() {
        return Some(CallDiscriminator::Iata(ch[..2].iter().collect()));
    }
    let icao: String = ch[..3].iter().collect();
    icao.chars()
        .all(|c| c.is_ascii_alphabetic())
        .then_some(CallDiscriminator::Icao(icao))
}

pub(crate) fn reg_shape(s: &str, min_len: usize) -> Option<String> {
    let r = norm_upper(s);
    if r.is_empty() || len16(&r) < min_len + 1 {
        return None;
    }
    (r.contains('-') || r.starts_with('N')).then_some(r)
}

/// `To_Call`: at least 4 characters, no `-`, and (with `check_airline`) a known airline by the
/// shared call discriminator.
pub fn to_call(s: &str, check_airline: bool, l: &dyn Lookup) -> Option<String> {
    let c = call_shape(s)?;
    if !check_airline {
        return Some(c);
    }
    match call_discriminator(&c)? {
        CallDiscriminator::Iata(iata) => l.airline_by_iata(&iata).then_some(c),
        CallDiscriminator::Icao(icao) => l.airline_by_icao(&icao).then_some(c),
    }
}

/// `To_Squawk`: 4 octal digits.
pub fn to_squawk(s: &str) -> Option<i32> {
    let q = norm_upper(s);
    if len16(&q) != 4 || !q.chars().all(|c| ('0'..='7').contains(&c)) {
        return None;
    }
    i32::from_str_radix(&q, 8).ok()
}

fn to_coord(s: &str, max: f64) -> Option<f64> {
    let c = strip_trim(s);
    if len16(&c) < 3 || !c.contains('.') {
        return None;
    }
    dotnet_double(&c).filter(|d| *d >= -max && *d <= max)
}

/// `To_Lat`: needs a decimal point, -90..90.
pub fn to_lat(s: &str) -> Option<f64> {
    to_coord(s, 90.0)
}

/// `To_Lon`: needs a decimal point, -180..180.
pub fn to_lon(s: &str) -> Option<f64> {
    to_coord(s, 180.0)
}

fn to_int(s: &str, max_len: usize, ok: impl Fn(i64) -> bool) -> Option<i32> {
    let c = strip_trim(s);
    if len16(&c) > max_len || !is_int(&c) {
        return None;
    }
    c.parse::<i64>().ok().filter(|v| ok(*v)).map(|v| v as i32)
}

/// `To_Alt` [ft]: up to 5 digits, 0..100000. Integers only (logic-findings #39 of the survey).
pub fn to_alt(s: &str) -> Option<i32> {
    to_int(s, 5, |v| (0..=100000).contains(&v))
}

/// `To_Track`: up to 3 digits, 0..359.
pub fn to_track(s: &str) -> Option<i32> {
    to_int(s, 3, |v| (0..360).contains(&v))
}

/// `To_Speed` [kts]: up to 3 digits, 0..800.
pub fn to_speed(s: &str) -> Option<i32> {
    to_int(s, 3, |v| (0..=800).contains(&v))
}
