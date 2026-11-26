//! Writer of `.loc` tiles in the byte layout airscout.eu serves: Newtonsoft.Json indented
//! output of `ElevationTileDesignator` (`ElevationTile.cs:56-124`) under .NET Framework,
//! written with `StreamWriter.WriteLine` (`ElevationTileGenerator/MainDlg.cs:285`).

use crate::ElevationTile;
use chrono::{DateTime, Timelike, Utc};
use std::fmt::Write as _;

/// .NET Framework `double.ToString("R", InvariantCulture)` plus Newtonsoft `EnsureDecimalPlace`.
///
/// 15 significant digits if they parse back to `x`, else 17. Trailing mantissa zeros are
/// dropped. Plain decimal when the exponent `e` of the first digit is in `-5 < e < p` (`p` the
/// digit count used), else `d[.ddd]E+XX`. `.0` is appended to a text without `.` and `E`.
/// `-0.0` gives `0.0`, as on the Framework.
///
/// The 17th digit is correctly rounded. The Framework digit generator writes it one higher in
/// some cases (189 of 9 220 fixture coordinates); the value parses to the same `f64`.
pub fn framework_number_text(x: f64) -> String {
    if x == 0.0 {
        return "0.0".to_string();
    }
    let (sci, p) = {
        let s15 = format!("{:.14e}", x);
        if s15.parse::<f64>().ok() == Some(x) {
            (s15, 15)
        } else {
            (format!("{:.16e}", x), 17)
        }
    };
    let (mantissa, exp) = sci.split_once('e').expect("LowerExp has an exponent");
    let e: i32 = exp.parse().expect("LowerExp exponent is an integer");
    let (negative, mantissa) = match mantissa.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mantissa),
    };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if -5 < e && e < p {
        if e >= 0 {
            let int_len = e as usize + 1;
            if digits.len() <= int_len {
                out.push_str(digits);
                out.extend(std::iter::repeat_n('0', int_len - digits.len()));
            } else {
                out.push_str(&digits[..int_len]);
                out.push('.');
                out.push_str(&digits[int_len..]);
            }
        } else {
            out.push_str("0.");
            out.extend(std::iter::repeat_n('0', (-e - 1) as usize));
            out.push_str(digits);
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let _ = write!(out, "E{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    }
    if !out.contains('.') && !out.contains('E') {
        out.push_str(".0");
    }
    out
}

/// Newtonsoft ISO date (`yyyy-MM-ddTHH:mm:ss.FFFFFFFK` for a UTC `DateTime`): up to 7 fraction
/// digits (100 ns ticks, truncated) without trailing zeros, none when zero, then `Z`.
pub fn loc_date_text(t: &DateTime<Utc>) -> String {
    let mut out = t.format("%Y-%m-%dT%H:%M:%S").to_string();
    // A leap second carries nanos >= 1e9; .NET has no leap seconds, so cap at the last tick.
    let ticks = (t.nanosecond() / 100).min(9_999_999);
    if ticks != 0 {
        let frac = format!("{ticks:07}");
        out.push('.');
        out.push_str(frac.trim_end_matches('0'));
    }
    out.push('Z');
    out
}

impl ElevationTile {
    /// The `.loc` text in the served layout: members `TileIndex` .. `LastUpdated` at two-space
    /// indent, `Elv` one array per column with one element per line, every line (the closing
    /// `}` too) ended by `\r\n`. No BOM, no `Bounds` member.
    pub fn to_loc_json(&self) -> String {
        const NL: &str = "\r\n";
        let mut s = String::with_capacity(400 + self.elv.len() * 12);
        s.push('{');
        s.push_str(NL);
        // A tile index is a Maidenhead locator: no character needs JSON escaping, but escape
        // anyway so that any index gives valid JSON.
        let index = serde_json::to_string(&self.index).expect("a string serialises");
        let members = [
            ("TileIndex", index),
            ("MinElv", self.min_elv.to_string()),
            ("MinLat", framework_number_text(self.min_lat)),
            ("MinLon", framework_number_text(self.min_lon)),
            ("MaxElv", self.max_elv.to_string()),
            ("MaxLat", framework_number_text(self.max_lat)),
            ("MaxLon", framework_number_text(self.max_lon)),
            ("Rows", self.rows.to_string()),
            ("Columns", self.columns.to_string()),
        ];
        for (name, value) in members {
            let _ = write!(s, "  \"{name}\": {value},{NL}");
        }
        let _ = write!(s, "  \"Elv\": [{NL}");
        let columns: Vec<&[i16]> = self.elv.chunks(self.rows.max(1)).collect();
        for (c, column) in columns.iter().enumerate() {
            let _ = write!(s, "    [{NL}");
            for (r, v) in column.iter().enumerate() {
                let sep = if r + 1 < column.len() { "," } else { "" };
                let _ = write!(s, "      {v}{sep}{NL}");
            }
            let sep = if c + 1 < columns.len() { "," } else { "" };
            let _ = write!(s, "    ]{sep}{NL}");
        }
        let _ = write!(s, "  ],{NL}");
        let date = loc_date_text(&self.last_updated);
        let _ = write!(s, "  \"LastUpdated\": \"{date}\"{NL}");
        s.push('}');
        s.push_str(NL);
        s
    }
}
