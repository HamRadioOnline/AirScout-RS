//! JSON text as Newtonsoft.Json 13.0.3 writes it on .NET 8 with the web route settings
//! (`AirScout/Webserver.cs:455-460`, `SQLiteDatabase/SQLiteDatabase/SQLiteEntry.cs:190-199`):
//! `DateTimeZoneHandling.Utc`, `FloatFormatHandling.String`, `Formatting.Indented`.
//!
//! The golden cases in `tests/golden/web_json.json` pin every rule here.

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Serialize, Serializer};
use serde_json::ser::{CompactFormatter, Formatter, PrettyFormatter};
use std::io;

/// A C# `double`. A finite value is a JSON number in the Newtonsoft layout
/// ([`number_text`]). NaN and the infinities are the strings `"NaN"`, `"Infinity"` and
/// `"-Infinity"` (`FloatFormatHandling.String`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Num(pub f64);

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let x = self.0;
        if x.is_nan() {
            s.serialize_str("NaN")
        } else if x == f64::INFINITY {
            s.serialize_str("Infinity")
        } else if x == f64::NEG_INFINITY {
            s.serialize_str("-Infinity")
        } else {
            s.serialize_f64(x)
        }
    }
}

/// The text of a finite double: the shortest round-trip digits (.NET `"R"`), in exponent
/// form `d[.ddd]E+XX` / `E-XX` when the decimal exponent of the first digit is 17 or more
/// or -5 or less, else plain. Newtonsoft then appends `.0` when the text has neither `.`
/// nor `E` (`EnsureDecimalPlace`). `-0.0` gives `-0.0`. NaN and the infinities give the
/// .NET texts `NaN`, `Infinity` and `-Infinity` (unquoted; [`Num`] writes them quoted).
pub fn number_text(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    // `{:e}` gives the shortest round-trip digits, e.g. "-1.2345e-5", "1e15", "-0e0".
    let sci = format!("{x:e}");
    let (mantissa, exp) = sci.split_once('e').expect("`{:e}` has an exponent");
    let exp: i32 = exp.parse().expect("`{:e}` exponent is an integer");
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(m) => ("-", m),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();

    let mut out = String::from(sign);
    if !(-4..17).contains(&exp) {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let exp_sign = if exp < 0 { '-' } else { '+' };
        out.push_str(&format!("E{exp_sign}{:02}", exp.unsigned_abs()));
    } else if exp < 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-exp - 1) as usize));
        out.push_str(&digits);
    } else {
        let int_len = exp as usize + 1;
        if digits.len() <= int_len {
            out.push_str(&digits);
            out.extend(std::iter::repeat_n('0', int_len - digits.len()));
        } else {
            out.push_str(&digits[..int_len]);
            out.push('.');
            out.push_str(&digits[int_len..]);
        }
    }
    if !out.contains('.') && !out.contains('E') {
        out.push_str(".0");
    }
    out
}

/// A C# `DateTime` written with `DateTimeZoneHandling.Utc`: `yyyy-MM-ddTHH:mm:ss`, then
/// `.` and up to 7 fraction digits (100 ns ticks, truncated) with trailing zeros removed
/// (none when zero), then `Z`.
pub fn date_text(t: &DateTime<Utc>) -> String {
    let mut s = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        t.year(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    );
    // a leap second carries nanos >= 1e9; .NET has no leap seconds
    let ticks = t.nanosecond().min(999_999_999) / 100;
    if ticks != 0 {
        let frac = format!("{ticks:07}");
        s.push('.');
        s.push_str(frac.trim_end_matches('0'));
    }
    s.push('Z');
    s
}

/// `#[serde(with = "as_web::json::date")]` for a `DateTime<Utc>` member.
pub mod date {
    use chrono::{DateTime, Utc};
    use serde::Serializer;

    pub fn serialize<S: Serializer>(t: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::date_text(t))
    }
}

/// `#[serde(with = "as_web::json::date_or_min")]` for an `Option<DateTime<Utc>>` member.
/// `None` is C# `DateTime.MinValue`, written as `0001-01-01T00:00:00Z`.
pub mod date_or_min {
    use chrono::{DateTime, Utc};
    use serde::Serializer;

    /// The text of C# `DateTime.MinValue` under `DateTimeZoneHandling.Utc`.
    pub const MIN_TEXT: &str = "0001-01-01T00:00:00Z";

    pub fn serialize<S: Serializer>(t: &Option<DateTime<Utc>>, s: S) -> Result<S::Ok, S::Error> {
        match t {
            Some(t) => s.serialize_str(&super::date_text(t)),
            None => s.serialize_str(MIN_TEXT),
        }
    }
}

/// `Formatting.Indented`: two spaces, `"key": value`, `\n` line ends.
pub fn to_indented<T: Serialize + ?Sized>(v: &T) -> String {
    write(v, DotNetFormatter(PrettyFormatter::with_indent(b"  ")))
}

/// `Formatting.None`: no whitespace.
pub fn to_compact<T: Serialize + ?Sized>(v: &T) -> String {
    write(v, DotNetFormatter(CompactFormatter))
}

fn write<T: Serialize + ?Sized, F: Formatter>(v: &T, f: DotNetFormatter<F>) -> String {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, f);
    v.serialize(&mut ser)
        .expect("serialising a DTO into memory cannot fail");
    String::from_utf8(buf).expect("serde_json writes UTF-8")
}

/// The serde_json layout `F` with the Newtonsoft number text and string escapes.
struct DotNetFormatter<F>(F);

impl<F: Formatter> Formatter for DotNetFormatter<F> {
    fn write_f64<W: ?Sized + io::Write>(&mut self, w: &mut W, x: f64) -> io::Result<()> {
        w.write_all(number_text(x).as_bytes())
    }

    fn write_f32<W: ?Sized + io::Write>(&mut self, w: &mut W, x: f32) -> io::Result<()> {
        self.write_f64(w, f64::from(x))
    }

    /// Newtonsoft (`StringEscapeHandling.Default`) also escapes U+0085, U+2028 and U+2029.
    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        let mut rest = fragment;
        while let Some(i) = rest.find(['\u{85}', '\u{2028}', '\u{2029}']) {
            w.write_all(&rest.as_bytes()[..i])?;
            let c = rest[i..].chars().next().expect("found a char at i");
            write!(w, "\\u{:04x}", c as u32)?;
            rest = &rest[i + c.len_utf8()..];
        }
        w.write_all(rest.as_bytes())
    }

    fn begin_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.begin_array(w)
    }

    fn end_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_array(w)
    }

    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.0.begin_array_value(w, first)
    }

    fn end_array_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_array_value(w)
    }

    fn begin_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.begin_object(w)
    }

    fn end_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_object(w)
    }

    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.0.begin_object_key(w, first)
    }

    fn end_object_key<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_object_key(w)
    }

    fn begin_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.begin_object_value(w)
    }

    fn end_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_object_value(w)
    }
}
