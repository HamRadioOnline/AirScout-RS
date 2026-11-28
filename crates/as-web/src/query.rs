//! URL decoding and query parsing with the .NET Framework rules the C# server uses.
//!
//! Every route reads its parameters as
//! `HttpUtility.ParseQueryString(raw.Substring(raw.IndexOf("?") + 1).ToUpper())`
//! (`Webserver.cs:514-515`) and numbers with
//! `double.TryParse(s, NumberStyles.Float, CultureInfo.InvariantCulture)`. The static
//! file path is `Uri.UnescapeDataString(RawUrl)` (`Webserver.cs:1122`).
//!
//! All functions take untrusted LAN input: they never panic, and their output is at
//! most a small multiple of the input in size.

/// The value of one hex digit.
fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// The byte of a `%XX` escape at `s[i..]`.
fn escape_at(s: &[u8], i: usize) -> Option<u8> {
    if s.get(i) != Some(&b'%') {
        return None;
    }
    let hi = hex(*s.get(i + 1)?)?;
    let lo = hex(*s.get(i + 2)?)?;
    Some(hi << 4 | lo)
}

/// The index of the first byte at or after `from` that is one of `set`, or `b.len()`.
fn next_of(b: &[u8], from: usize, set: &[u8]) -> usize {
    b.iter()
        .skip(from)
        .position(|c| set.contains(c))
        .map_or(b.len(), |k| from + k)
}

/// `Uri.UnescapeDataString`: decodes the `%XX` escapes whose bytes form valid UTF-8.
/// An escape that is invalid, incomplete or part of an invalid UTF-8 sequence stays
/// as written. `+` stays `+`.
pub fn unescape_data_string(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if escape_at(b, i).is_none() {
            // Copy up to the next "%" (a char boundary: "%" is ASCII).
            let end = next_of(b, i + 1, b"%");
            out.push_str(&s[i..end]);
            i = end;
            continue;
        }
        // A run of consecutive escapes; escape `k` of the run is `s[start + 3k..][..3]`.
        let start = i;
        let mut bytes = Vec::new();
        while let Some(v) = escape_at(b, i) {
            bytes.push(v);
            i += 3;
        }
        let mut pos = 0;
        while pos < bytes.len() {
            match std::str::from_utf8(&bytes[pos..]) {
                Ok(text) => {
                    out.push_str(text);
                    pos = bytes.len();
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    // `valid_up_to` marks a valid prefix, so this never falls back.
                    out.push_str(std::str::from_utf8(&bytes[pos..pos + valid]).unwrap_or(""));
                    let bad = e.error_len().unwrap_or(bytes.len() - pos - valid);
                    let from = start + 3 * (pos + valid);
                    out.push_str(&s[from..from + 3 * bad]);
                    pos += valid + bad;
                }
            }
        }
    }
    out
}

/// `HttpUtility.UrlDecode` (UTF-8): `+` is a space, `%XX` escapes are bytes, and the
/// bytes are decoded lossily (an invalid sequence becomes U+FFFD). An invalid or
/// incomplete escape stays as written. `%uXXXX` needs a lower-case `u`, which an
/// upper-cased query never has, so it is not decoded.
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut pending: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if let Some(v) = escape_at(b, i) {
            pending.push(v);
            i += 3;
            continue;
        }
        if !pending.is_empty() {
            out.push_str(&String::from_utf8_lossy(&pending));
            pending.clear();
        }
        if b[i] == b'+' {
            out.push(' ');
            i += 1;
        } else {
            let end = next_of(b, i + 1, b"%+");
            out.push_str(&s[i..end]);
            i = end;
        }
    }
    out.push_str(&String::from_utf8_lossy(&pending));
    out
}

/// The parameters of a request, as `HttpUtility.ParseQueryString` gives them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pairs: Vec<(String, String)>,
}

impl Query {
    /// Parses the text after the first `?` of `raw` (the raw request target), upper-cased
    /// before decoding. `ParseQueryString` drops one more leading `?`. Parts are split at
    /// `&`, then at their first `=`; a part without `=` has a null key and is dropped.
    pub fn from_raw_url(raw: &str) -> Query {
        let Some(q) = raw.find('?').map(|i| &raw[i + 1..]) else {
            return Query::default();
        };
        let upper = q.to_uppercase();
        let text = upper.strip_prefix('?').unwrap_or(&upper);
        let pairs = text
            .split('&')
            .filter_map(|part| part.split_once('='))
            .map(|(k, v)| (url_decode(k), url_decode(v)))
            .collect();
        Query { pairs }
    }

    /// `NameValueCollection.Get`: every value of `key`, joined with `,`, or `None`.
    /// Keys compare ignoring case, as in `HttpValueCollection`.
    pub fn get(&self, key: &str) -> Option<String> {
        let wanted = key.to_uppercase();
        let mut values = self
            .pairs
            .iter()
            .filter(|(k, _)| k.to_uppercase() == wanted)
            .map(|(_, v)| v.as_str());
        let first = values.next()?;
        Some(values.fold(first.to_string(), |mut acc, v| {
            acc.push(',');
            acc.push_str(v);
            acc
        }))
    }

    /// True when the query has no `key=value` part.
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

/// White space that .NET number parsing skips: U+0009 to U+000D and U+0020.
fn is_number_space(c: char) -> bool {
    matches!(c, '\t'..='\r' | ' ')
}

/// `double.TryParse(s, NumberStyles.Float, CultureInfo.InvariantCulture)` on .NET
/// Framework: surrounding white space, an optional sign, digits with an optional `.`
/// fraction (at least one digit in all) and an optional exponent with digits. No
/// thousands separators, no hex, no `NaN` or `Infinity` symbols (net472 matches them
/// case-sensitively and the query is upper-cased). A value out of range fails, as
/// .NET Framework does, instead of giving infinity.
pub fn parse_float(s: &str) -> Option<f64> {
    let t = s.trim_matches(is_number_space).as_bytes();
    let mut i = 0;
    if matches!(t.first(), Some(b'+' | b'-')) {
        i += 1;
    }
    let int_start = i;
    while t.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    let mut digits = i - int_start;
    if t.get(i) == Some(&b'.') {
        i += 1;
        let frac_start = i;
        while t.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        digits += i - frac_start;
    }
    if digits == 0 {
        return None;
    }
    if matches!(t.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(t.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exp_start = i;
        while t.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == exp_start {
            return None;
        }
    }
    if i != t.len() {
        return None;
    }
    // The text is ASCII and in Rust's float syntax.
    let v: f64 = std::str::from_utf8(t).ok()?.parse().ok()?;
    v.is_finite().then_some(v)
}
