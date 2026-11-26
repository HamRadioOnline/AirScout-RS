//! Flexible JSON feed (`AirScout.PlaneFeeds.Plugin.FlexJSON`): reads JSON of unknown layout
//! and guesses which column holds which plane field.
//!
//! Faithful to `FlexJSON.cs`, including its quirks (see `docs/logic-findings.md`): column 0
//! (the container key) is detected but never read, the detection compares the units with
//! `"m "` / `"km/h "` (trailing space), altitude, track and speed parse integers only,
//! objects drop fractional numbers, a JSON `null` anywhere fails the request, and
//! `min_elements` / `min_planes` are never used. The C# number parsing is modelled with the
//! invariant culture.

use super::{http_get, set_str, set_u64};
use crate::vars::{self, format_double};
use crate::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::{DateTime, Datelike, NaiveDateTime, Utc};

/// `JavaScriptSerializer.MaxJsonLength` default (UTF-16 code units).
const MAX_JSON_LENGTH: usize = 2_097_152;
/// `JavaScriptSerializer.RecursionLimit` default.
const RECURSION_LIMIT: usize = 100;

// ------------------------------------------------------------------ JSON (JavaScriptSerializer)

/// A value as `JavaScriptSerializer.Deserialize<dynamic>` yields it, reduced to what
/// FlexJSON can tell apart: `Text` is a string or a primitive (int, long, double, bool) as
/// `ToString()` prints it, `Dec` a `decimal` (not a primitive).
#[derive(Debug)]
enum J {
    Obj(Vec<(String, J)>),
    Arr(Vec<J>),
    Text(String),
    Dec(String),
    Null,
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

fn jerr(msg: &str, at: usize) -> FeedError {
    FeedError::Parse(format!("{msg} at offset {at}"))
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += c.len_utf8();
        Some(c)
    }

    fn ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn eat(&mut self, lit: &str) -> Result<(), FeedError> {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            Ok(())
        } else {
            Err(jerr("invalid JSON literal", self.i))
        }
    }

    fn value(&mut self, depth: usize) -> Result<J, FeedError> {
        if depth > RECURSION_LIMIT {
            return Err(jerr("RecursionLimit exceeded", self.i));
        }
        self.ws();
        match self.peek() {
            Some('{') => self.object(depth),
            Some('[') => self.array(depth),
            Some('"') => Ok(J::Text(self.string()?)),
            Some('t') => self.eat("true").map(|()| J::Text("True".into())),
            Some('f') => self.eat("false").map(|()| J::Text("False".into())),
            Some('n') => self.eat("null").map(|()| J::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            _ => Err(jerr("invalid JSON value", self.i)),
        }
    }

    fn object(&mut self, depth: usize) -> Result<J, FeedError> {
        self.bump();
        let mut m: Vec<(String, J)> = Vec::new();
        self.ws();
        if self.peek() == Some('}') {
            self.bump();
            return Ok(J::Obj(m));
        }
        loop {
            self.ws();
            if self.peek() != Some('"') {
                return Err(jerr("expected a member name", self.i));
            }
            let k = self.string()?;
            self.ws();
            if self.bump() != Some(':') {
                return Err(jerr("expected ':'", self.i));
            }
            let v = self.value(depth + 1)?;
            // `dictionary[name] = value`: a repeated name keeps its place, the last value wins
            match m.iter_mut().find(|(key, _)| *key == k) {
                Some(e) => e.1 = v,
                None => m.push((k, v)),
            }
            self.ws();
            match self.bump() {
                Some(',') => {}
                Some('}') => return Ok(J::Obj(m)),
                _ => return Err(jerr("expected ',' or '}'", self.i)),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<J, FeedError> {
        self.bump();
        let mut a = Vec::new();
        self.ws();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(J::Arr(a));
        }
        loop {
            a.push(self.value(depth + 1)?);
            self.ws();
            match self.bump() {
                Some(',') => {}
                Some(']') => return Ok(J::Arr(a)),
                _ => return Err(jerr("expected ',' or ']'", self.i)),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, FeedError> {
        let h = self
            .s
            .get(self.i..self.i + 4)
            .filter(|h| h.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| jerr("bad \\u escape", self.i))?;
        self.i += 4;
        u32::from_str_radix(h, 16).map_err(|_| jerr("bad \\u escape", self.i))
    }

    fn string(&mut self) -> Result<String, FeedError> {
        self.bump();
        let mut out = String::new();
        loop {
            match self
                .bump()
                .ok_or_else(|| jerr("unterminated string", self.i))?
            {
                '"' => return Ok(out),
                '\\' => {
                    let c = match self.bump() {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('/') => '/',
                        Some('b') => '\u{8}',
                        Some('f') => '\u{c}',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('u') => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi)
                                && self.s[self.i..].starts_with("\\u")
                            {
                                let save = self.i;
                                self.i += 2;
                                let lo = self.hex4()?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                } else {
                                    self.i = save;
                                    hi
                                }
                            } else {
                                hi
                            };
                            char::from_u32(code).unwrap_or('\u{FFFD}')
                        }
                        _ => return Err(jerr("bad escape", self.i)),
                    };
                    out.push(c);
                }
                c => out.push(c),
            }
        }
    }

    /// JSON number grammar; classified like `JavaScriptObjectDeserializer`: without
    /// exponent and point an `int`/`long`, else a `decimal` (no exponent), else a `double`.
    fn number(&mut self) -> Result<J, FeedError> {
        let start = self.i;
        let digits = |p: &mut Self| {
            let s = p.i;
            while p.peek().is_some_and(|c| c.is_ascii_digit()) {
                p.bump();
            }
            p.i > s
        };
        if self.peek() == Some('-') {
            self.bump();
        }
        if !digits(self) {
            return Err(jerr("invalid number", start));
        }
        let mut point = false;
        if self.peek() == Some('.') {
            point = true;
            self.bump();
            if !digits(self) {
                return Err(jerr("invalid number", start));
            }
        }
        let mut exp = false;
        if matches!(self.peek(), Some('e' | 'E')) {
            exp = true;
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if !digits(self) {
                return Err(jerr("invalid number", start));
            }
        }
        let t = &self.s[start..self.i];
        let dbl = || {
            t.parse::<f64>()
                .map(|d| J::Text(format_double(d)))
                .map_err(|_| jerr("invalid number", start))
        };
        if exp {
            return dbl();
        }
        if !point {
            if let Ok(l) = t.parse::<i64>() {
                return Ok(J::Text(l.to_string()));
            }
        }
        // decimal holds 28-29 significant digits; a longer integer part overflows it
        let int_digits = t
            .split('.')
            .next()
            .unwrap_or("")
            .trim_start_matches('-')
            .len();
        if int_digits <= 28 {
            Ok(J::Dec(t.to_string()))
        } else {
            dbl()
        }
    }
}

fn parse_json(s: &str) -> Result<J, FeedError> {
    if s.encode_utf16().count() > MAX_JSON_LENGTH {
        return Err(FeedError::Parse(
            "the JSON is longer than MaxJsonLength (2097152)".into(),
        ));
    }
    let mut p = Parser {
        s: s.trim_start_matches('\u{feff}'),
        i: 0,
    };
    p.ws();
    if p.peek().is_none() {
        // Deserialize("") yields null, which AddElement cannot handle
        return Err(FeedError::Parse("empty response".into()));
    }
    let v = p.value(1)?;
    p.ws();
    if p.peek().is_some() {
        return Err(jerr("data after the JSON value", p.i));
    }
    Ok(v)
}

// ------------------------------------------------------------------ flattening

/// `AddElement` / `GetAllPlaneInfos`: every object and array with its key, and how often
/// each length occurs (in order of first occurrence).
struct Flat<'a> {
    elems: Vec<(String, &'a J, usize)>,
    lengths: Vec<(usize, usize)>,
    guids: u64,
}

impl<'a> Flat<'a> {
    fn count(&mut self, len: usize) {
        match self.lengths.iter_mut().find(|(l, _)| *l == len) {
            Some(e) => e.1 += 1,
            None => self.lengths.push((len, 1)),
        }
    }

    /// Stands in for `Guid.NewGuid().ToString()`: same shape, and every FlexJSON check
    /// treats it like a random GUID (only `IsReg` matches, because of the `-`).
    fn guid(&mut self) -> String {
        self.guids += 1;
        format!("{:08x}-0000-4000-8000-{:012x}", self.guids, self.guids)
    }

    fn add(&mut self, key: String, el: &'a J) -> Result<(), FeedError> {
        match el {
            // `el.GetType()` on null throws
            J::Null => Err(FeedError::Parse(
                "null value in the JSON (FlexJSON cannot handle it)".into(),
            )),
            J::Obj(m) => {
                self.count(m.len());
                self.elems.push((key, el, m.len()));
                for (k, v) in m {
                    self.add(k.clone(), v)?;
                }
                Ok(())
            }
            J::Arr(a) => {
                self.count(a.len());
                self.elems.push((key, el, a.len()));
                for v in a {
                    let k = self.guid();
                    self.add(k, v)?;
                }
                Ok(())
            }
            J::Text(_) | J::Dec(_) => Ok(()),
        }
    }
}

/// `GetPlaneInfo`: `[key, v1, .., vn]`. Objects keep strings and primitives only (a
/// `decimal` becomes `""`), arrays keep decimals too; nested containers become `""`.
fn row(key: &str, el: &J) -> Vec<String> {
    let mut r = vec![key.to_string()];
    match el {
        J::Obj(m) => r.extend(m.iter().map(|(_, v)| match v {
            J::Text(s) => s.clone(),
            _ => String::new(),
        })),
        J::Arr(a) => r.extend(a.iter().map(|v| match v {
            J::Text(s) | J::Dec(s) => s.clone(),
            _ => String::new(),
        })),
        _ => {}
    }
    r
}

/// The plane rows: containers of the most common length.
fn plane_rows(root: &J) -> Result<(Vec<Vec<String>>, usize), FeedError> {
    let mut f = Flat {
        elems: Vec::new(),
        lengths: Vec::new(),
        guids: 0,
    };
    f.add(String::new(), root)?;
    let (mut maxcount, mut maxkey) = (0, 0);
    for &(len, n) in &f.lengths {
        if n > maxcount {
            maxcount = n;
            maxkey = len;
        }
    }
    let rows = f
        .elems
        .iter()
        .filter(|(_, _, len)| *len == maxkey)
        .map(|(k, el, _)| row(k, el))
        .collect();
    Ok((rows, maxkey + 1))
}

// ------------------------------------------------------------------ cell checks and converters

/// `double.TryParse` (Float | AllowThousands, invariant culture).
fn net_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    match s {
        "NaN" => return Some(f64::NAN),
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let b = s.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'+' | b'-')) {
        i = 1;
    }
    let mut num = String::from(&s[..i]);
    let mut seen = false;
    while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b',' && seen)) {
        if b[i] != b',' {
            num.push(char::from(b[i]));
            seen = true;
        }
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        num.push('.');
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            num.push(char::from(b[i]));
            seen = true;
            i += 1;
        }
    }
    if !seen {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        num.push('e');
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            num.push(char::from(b[i]));
            i += 1;
        }
        let d = i;
        while i < b.len() && b[i].is_ascii_digit() {
            num.push(char::from(b[i]));
            i += 1;
        }
        if i == d {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    num.parse().ok()
}

fn to_double(s: &str) -> f64 {
    net_f64(s).unwrap_or(f64::MIN)
}

/// `long.TryParse` / `Convert.ToInt64` (NumberStyles.Integer).
fn net_i64(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn is_int(s: &str) -> bool {
    let s = s.trim();
    s.chars().all(|c| "-01234567890".contains(c)) && net_i64(s).is_some()
}

fn is_double(s: &str) -> bool {
    let s = s.trim();
    (s.contains('.') || s.contains(',')) && net_f64(s).is_some()
}

fn is_hex(s: &str) -> bool {
    let s = s.trim();
    utf16_len(s) == 6 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// `IsUTC`: only millisecond time stamps within 1000 s of now pass; a 10-digit value in
/// seconds passes the first check and then fails the millisecond one.
fn is_utc(s: &str, now: DateTime<Utc>) -> bool {
    let now_int = now.timestamp() as i32;
    let now_long = i64::from(now_int) * 1000;
    let s = s.trim();
    if utf16_len(s) < 10 {
        return false;
    }
    if !is_int(s) && !s.chars().any(|c| "-_:".contains(c)) {
        return false;
    }
    if utf16_len(s) < 13 {
        match net_i64(s) {
            Some(l) if i64::from(now_int).wrapping_sub(l).unsigned_abs() <= 1000 => {}
            _ => return false,
        }
    }
    match net_i64(s) {
        Some(l) => now_long.wrapping_sub(l).unsigned_abs() <= 1_000_000,
        None => false,
    }
}

fn in_range(s: &str, lo: f64, hi: f64) -> bool {
    let d = to_double(s.trim());
    d >= lo && d <= hi
}

fn is_lat(s: &str) -> bool {
    is_double(s) && in_range(s, -90.0, 90.0)
}

fn is_lon(s: &str) -> bool {
    is_double(s) && in_range(s, -180.0, 180.0)
}

fn is_alt(s: &str, units_alt: &str) -> bool {
    if !is_int(s) && !is_double(s) {
        return false;
    }
    let d = to_double(s.trim());
    // "m " with a trailing space: never true for the setting "m"
    !(d < 0.0 || (units_alt.to_lowercase() == "m " && d > 30000.0) || d > 100_000.0)
}

fn is_track(s: &str) -> bool {
    (is_int(s) || is_double(s)) && in_range(s, 0.0, 360.0)
}

fn is_speed(s: &str, units_speed: &str) -> bool {
    if !is_int(s) && !is_double(s) {
        return false;
    }
    let d = to_double(s.trim());
    !(d < 0.0 || (units_speed.to_lowercase() == "km/h " && d > 1000.0) || d > 550.0)
}

fn is_reg(s: &str) -> bool {
    let s = s.trim();
    s.contains('-') || s.starts_with('N')
}

fn is_type(s: &str) -> bool {
    let s = s.trim();
    utf16_len(s) == 4 && s.chars().any(char::is_numeric)
}

fn clean(s: &str) -> String {
    s.replace('"', "").trim().to_string()
}

fn to_hex(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let s = s.replace('"', "").to_uppercase().trim().to_string();
    is_hex(&s).then_some(s)
}

fn to_text(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.replace('"', "").to_uppercase().trim().to_string())
}

/// `new DateTime(1970,1,1).AddMilliseconds(ms)`; out of the DateTime range throws.
fn from_ms(ms: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms).filter(|t| (1..=9999).contains(&t.year()))
}

/// `To_UTC`; `None` stands for `DateTime.MinValue`.
fn to_utc(s: &str) -> Option<DateTime<Utc>> {
    if s.is_empty() {
        return None;
    }
    let s = clean(s);
    if utf16_len(&s) >= 10 && is_int(&s) {
        if let Some(l) = net_i64(&s) {
            let t = if utf16_len(&s) > 10 {
                from_ms(l)
            } else {
                l.checked_mul(1000).and_then(from_ms)
            };
            if t.is_some() {
                return t;
            }
        }
    }
    // DateTime.TryParse: ISO forms, taken as UTC
    DateTime::parse_from_rfc3339(&s)
        .map(|t| t.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"]
                .iter()
                .find_map(|f| NaiveDateTime::parse_from_str(&s, f).ok())
                .map(|t| t.and_utc())
        })
}

fn to_latlon(s: &str, limit: f64) -> f64 {
    let s = clean(s);
    if utf16_len(&s) < 3 || !s.contains('.') {
        return f64::MIN;
    }
    match net_f64(&s) {
        Some(d) if d >= -limit && d <= limit => d,
        _ => f64::MIN,
    }
}

fn to_alt(s: &str, units_alt: &str) -> i32 {
    let Some(mut alt) = net_i64(&clean(s)) else {
        return i32::MIN;
    };
    if units_alt.to_lowercase() == "m" {
        alt = i64::from((alt as f64 * 3.28084) as i32);
    }
    if !(0..=100_000).contains(&alt) {
        return i32::MIN;
    }
    alt as i32
}

fn to_track(s: &str) -> i32 {
    match net_i64(&clean(s)) {
        Some(t) if (0..=360).contains(&t) => t as i32,
        _ => i32::MIN,
    }
}

fn to_speed(s: &str, units_speed: &str) -> i32 {
    let Some(speed) = net_i64(&clean(s)) else {
        return i32::MIN;
    };
    if units_speed.to_lowercase() == "km/h" {
        // no range check in this branch
        return (speed as f64 / 1.852) as i32;
    }
    if !(0..=550).contains(&speed) {
        return i32::MIN;
    }
    speed as i32
}

// ------------------------------------------------------------------ column detection

/// Per-column statistics of `GetColumnInfo`.
struct Columns {
    max: Vec<f64>,
    mean: Vec<f64>,
    hex: Vec<usize>,
    utc: Vec<usize>,
    lat: Vec<usize>,
    lon: Vec<usize>,
    alt: Vec<usize>,
    track: Vec<usize>,
    speed: Vec<usize>,
    typ: Vec<usize>,
    reg: Vec<usize>,
}

fn column_info(
    rows: &[Vec<String>],
    n: usize,
    units_alt: &str,
    units_speed: &str,
    now: DateTime<Utc>,
) -> Columns {
    let z = || vec![0usize; n];
    let mut c = Columns {
        max: vec![0.0; n],
        mean: vec![0.0; n],
        hex: z(),
        utc: z(),
        lat: z(),
        lon: z(),
        alt: z(),
        track: z(),
        speed: z(),
        typ: z(),
        reg: z(),
    };
    let mut min = vec![0.0; n];
    let mut sum = vec![0.0; n];
    for r in rows {
        for (i, s) in r.iter().enumerate().take(n) {
            let mut stat = |v: f64| {
                // `if (v < min) .. else if (v > max)`
                if v < min[i] {
                    min[i] = v;
                } else if v > c.max[i] {
                    c.max[i] = v;
                }
            };
            if is_double(s) {
                let d = to_double(s);
                if d != f64::MIN {
                    stat(d);
                }
                sum[i] += d;
            }
            if is_int(s) {
                if let Some(l) = net_i64(s) {
                    if l != i64::MIN {
                        stat(l as f64);
                    }
                    sum[i] += l as f64;
                }
            }
            let tally = [
                (&mut c.hex, is_hex(s)),
                (&mut c.utc, is_utc(s, now)),
                (&mut c.lat, is_lat(s)),
                (&mut c.lon, is_lon(s)),
                (&mut c.alt, is_alt(s, units_alt)),
                (&mut c.track, is_track(s)),
                (&mut c.speed, is_speed(s, units_speed)),
                (&mut c.typ, is_type(s)),
                (&mut c.reg, is_reg(s)),
            ];
            for (v, hit) in tally {
                if hit {
                    v[i] += 1;
                }
            }
        }
    }
    for i in 0..n {
        c.mean[i] = sum[i] / rows.len() as f64;
    }
    c
}

/// `Find*Index`: the first free column with the most hits, if more than 90 % of the rows.
fn find(
    hits: &[usize],
    detected: &mut [bool],
    rows: usize,
    ok: impl Fn(usize) -> bool,
) -> Option<usize> {
    let (mut maxindex, mut maxcount) = (None, 0);
    for (i, &h) in hits.iter().enumerate() {
        if !detected[i] && ok(i) && h > maxcount {
            maxcount = h;
            maxindex = Some(i);
        }
    }
    let i = maxindex?;
    if maxcount as i64 > (0.9 * rows as f64) as i64 {
        detected[i] = true;
        Some(i)
    } else {
        None
    }
}

// ------------------------------------------------------------------ the feed

pub const DISCLAIMER: &str = "This feed reads JSON from a web server you configure and \
guesses its layout. Some servers do not allow automated use of their data: use only sources \
you are permitted to use. You use this feed at your own risk.";

/// The FlexJSON feed. Indices are column numbers in a row `[key, v1, .., vn]`; -1 means
/// "detect on the next request".
#[derive(Debug, Clone)]
pub struct FlexJson {
    pub url: String,
    pub timeout_s: u64,
    pub units_alt: String,
    pub units_speed: String,
    pub min_elements: i32,
    pub min_planes: i32,
    pub index_hex: i32,
    pub index_utc: i32,
    pub index_call: i32,
    pub index_type: i32,
    pub index_lat: i32,
    pub index_lon: i32,
    pub index_alt: i32,
    pub index_track: i32,
    pub index_speed: i32,
    pub index_reg: i32,
}

impl Default for FlexJson {
    fn default() -> Self {
        Self {
            url: String::new(),
            timeout_s: 30,
            units_alt: "ft".into(),
            units_speed: "kts".into(),
            min_elements: 10,
            min_planes: 50,
            index_hex: -1,
            index_utc: -1,
            index_call: -1,
            index_type: -1,
            index_lat: -1,
            index_lon: -1,
            index_alt: -1,
            index_track: -1,
            index_speed: -1,
            index_reg: -1,
        }
    }
}

fn set_i32(t: &toml::Table, key: &str, dst: &mut i32) -> Result<(), FeedError> {
    match t.get(key) {
        None => Ok(()),
        Some(toml::Value::Integer(i)) => {
            *dst = i32::try_from(*i)
                .map_err(|_| FeedError::Config(format!("{key} is out of range")))?;
            Ok(())
        }
        Some(_) => Err(FeedError::Config(format!("{key} must be a number"))),
    }
}

impl FlexJson {
    fn indices(&self) -> [i32; 10] {
        [
            self.index_hex,
            self.index_utc,
            self.index_call,
            self.index_type,
            self.index_lat,
            self.index_lon,
            self.index_alt,
            self.index_track,
            self.index_speed,
            self.index_reg,
        ]
    }

    /// Detect the missing indices, then map every row.
    fn planes(&mut self, json: &str, now: DateTime<Utc>) -> Result<Vec<FeedPlane>, FeedError> {
        let root = parse_json(json)?;
        let (rows, n) = plane_rows(&root)?;
        let mut detected = vec![false; n];
        for i in self.indices() {
            if let Some(d) = usize::try_from(i).ok().and_then(|i| detected.get_mut(i)) {
                *d = true;
            }
        }
        let c = column_info(&rows, n, &self.units_alt, &self.units_speed, now);
        let r = rows.len();
        let mut det = |idx: &mut i32, hits: &[usize], ok: &dyn Fn(usize) -> bool| {
            if *idx < 0 {
                if let Some(i) = find(hits, &mut detected, r, ok) {
                    *idx = i as i32;
                }
            }
        };
        let any = |_| true;
        det(&mut self.index_hex, &c.hex, &any);
        det(&mut self.index_utc, &c.utc, &any);
        det(&mut self.index_type, &c.typ, &any);
        det(&mut self.index_lat, &c.lat, &any);
        det(&mut self.index_lon, &c.lon, &any);
        det(&mut self.index_alt, &c.alt, &|i| c.mean[i] > 5000.0);
        det(&mut self.index_track, &c.track, &|i| c.max[i] <= 360.0);
        det(&mut self.index_speed, &c.speed, &|i| c.max[i] > 360.0);
        det(&mut self.index_reg, &c.reg, &any);
        for (name, idx) in [
            ("Lat", self.index_lat),
            ("Lon", self.index_lon),
            ("Alt", self.index_alt),
            ("Track", self.index_track),
            ("Speed", self.index_speed),
        ] {
            if idx < 0 {
                return Err(FeedError::Parse(format!(
                    "[FlexJSON]: Cannot get planes, {name} index not found/not set!"
                )));
            }
        }
        Ok(rows.iter().map(|r| self.plane(r, now)).collect())
    }

    fn plane(&self, r: &[String], now: DateTime<Utc>) -> FeedPlane {
        // a column is read only if its index is > 0
        let cell = |idx: i32| {
            usize::try_from(idx)
                .ok()
                .filter(|&i| i > 0)
                .and_then(|i| r.get(i))
        };
        let text = |idx: i32| cell(idx).and_then(|s| to_text(s)).unwrap_or_default();
        FeedPlane {
            hex: cell(self.index_hex)
                .and_then(|s| to_hex(s))
                .unwrap_or_default(),
            time: match cell(self.index_utc) {
                Some(s) => to_utc(s),
                None => Some(now),
            },
            lat: cell(self.index_lat).map_or(f64::MIN, |s| to_latlon(s, 90.0)),
            lon: cell(self.index_lon).map_or(f64::MIN, |s| to_latlon(s, 180.0)),
            alt_ft: cell(self.index_alt)
                .map_or(f64::MIN, |s| f64::from(to_alt(s, &self.units_alt))),
            speed_kts: f64::from(
                cell(self.index_speed).map_or(i32::MIN, |s| to_speed(s, &self.units_speed)),
            ),
            track: f64::from(cell(self.index_track).map_or(i32::MIN, |s| to_track(s))),
            call: text(self.index_call),
            type_code: text(self.index_type),
            reg: text(self.index_reg),
            ..FeedPlane::default()
        }
    }
}

impl PlaneFeed for FlexJson {
    fn name(&self) -> &'static str {
        "FlexJSON"
    }

    fn info(&self) -> &'static str {
        "Web feed with flexible JSON column assignment, for web feeds with unknown content. \
         The feed reads the JSON into rows of plane information and guesses which field is \
         in which column. You can also enter the column indices yourself."
    }

    fn disclaimer(&self) -> Option<&'static str> {
        Some(DISCLAIMER)
    }

    fn enabled_by_default(&self) -> bool {
        true
    }

    fn settings(&self) -> toml::Table {
        let mut t = toml::Table::new();
        t.insert("url".into(), self.url.clone().into());
        t.insert("timeout_s".into(), (self.timeout_s as i64).into());
        t.insert("units_alt".into(), self.units_alt.clone().into());
        t.insert("units_speed".into(), self.units_speed.clone().into());
        t.insert("min_elements".into(), i64::from(self.min_elements).into());
        t.insert("min_planes".into(), i64::from(self.min_planes).into());
        let names = [
            "index_hex",
            "index_utc",
            "index_call",
            "index_type",
            "index_lat",
            "index_lon",
            "index_alt",
            "index_track",
            "index_speed",
            "index_reg",
        ];
        for (k, v) in names.into_iter().zip(self.indices()) {
            t.insert(k.into(), i64::from(v).into());
        }
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        set_str(t, "url", &mut self.url)?;
        set_u64(t, "timeout_s", &mut self.timeout_s)?;
        set_str(t, "units_alt", &mut self.units_alt)?;
        set_str(t, "units_speed", &mut self.units_speed)?;
        set_i32(t, "min_elements", &mut self.min_elements)?;
        set_i32(t, "min_planes", &mut self.min_planes)?;
        set_i32(t, "index_hex", &mut self.index_hex)?;
        set_i32(t, "index_utc", &mut self.index_utc)?;
        set_i32(t, "index_call", &mut self.index_call)?;
        set_i32(t, "index_type", &mut self.index_type)?;
        set_i32(t, "index_lat", &mut self.index_lat)?;
        set_i32(t, "index_lon", &mut self.index_lon)?;
        set_i32(t, "index_alt", &mut self.index_alt)?;
        set_i32(t, "index_track", &mut self.index_track)?;
        set_i32(t, "index_speed", &mut self.index_speed)?;
        set_i32(t, "index_reg", &mut self.index_reg)
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let now = Utc::now();
        let url = vars::substitute(&self.url, args, now, "")?;
        if url.is_empty() {
            return Err(FeedError::Config("The URL is empty.".into()));
        }
        let json = http_get(&url, self.timeout_s, None)?;
        self.planes(&json, now)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}
