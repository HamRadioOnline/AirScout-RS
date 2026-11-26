//! URL template variables (`VarConverter` and its setup in `ADSBOne.cs:457-486` /
//! `VRSWebServer.cs:557`).

use crate::{FeedArgs, FeedError};
use chrono::{DateTime, Utc};

/// `double.ToString(CultureInfo.InvariantCulture)` of .NET Core 3.0+: the shortest string
/// that round-trips, positional for decimal exponents -4..=14, otherwise `d.dddE+XX`.
pub fn format_double(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    // Rust's `{:e}` yields the shortest round-trip digits: "-1.2345e-7"
    let sci = format!("{x:e}");
    let (mant, exp) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let neg = mant.starts_with('-');
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let sign = if neg { "-" } else { "" };
    if digits.chars().all(|c| c == '0') {
        return format!("{sign}0");
    }
    if !(-4..15).contains(&exp) {
        let (first, rest) = digits.split_at(1);
        let frac = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let esign = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{first}{frac}E{esign}{:02}", exp.abs());
    }
    let point = exp + 1; // digits before the decimal point
    let s = if point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}", "0".repeat(point as usize - digits.len()))
    } else {
        let (a, b) = digits.split_at(point as usize);
        format!("{a}.{b}")
    };
    format!("{sign}{s}")
}

/// `LatLon.Distance` of the feed plugins: haversine on a 6371 km sphere.
fn distance_km(mylat: f64, mylon: f64, lat: f64, lon: f64) -> f64 {
    let r = 6371.0;
    let dlat = mylat - lat;
    let dlon = mylon - lon;
    let rad = std::f64::consts::PI;
    let a = (dlat / 180.0 * rad / 2.0).sin() * (dlat / 180.0 * rad / 2.0).sin()
        + (dlon / 180.0 * rad / 2.0).sin()
            * (dlon / 180.0 * rad / 2.0).sin()
            * (mylat / 180.0 * rad).cos()
            * (lat / 180.0 * rad).cos();
    r * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}

fn value(name: &str, a: &FeedArgs, now: DateTime<Utc>, last_dv: &str) -> Option<String> {
    let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
    let ft = |m: i32| ((f64::from(m)) * 3.28084) as i32;
    Some(match name {
        "APPDIR" => path(&a.app_dir),
        "DATADIR" => path(&a.data_dir),
        "LOGDIR" => path(&a.log_dir),
        "DATABASEDIR" => path(&a.database_dir),
        "MINLAT" => format_double(a.min_lat),
        "MAXLAT" => format_double(a.max_lat),
        "MINLON" => format_double(a.min_lon),
        "MAXLON" => format_double(a.max_lon),
        "MINALTM" => a.min_alt_m.to_string(),
        "MAXALTM" => a.max_alt_m.to_string(),
        "MINALTFT" => ft(a.min_alt_m).to_string(),
        "MAXALTFT" => ft(a.max_alt_m).to_string(),
        "UNIXTIME" => now.timestamp().to_string(),
        "MYLAT" => format_double(a.my_lat),
        "MYLON" => format_double(a.my_lon),
        "MINDISTKM" => format_double(0.0),
        "MAXDISTKM" => {
            let mut m: f64 = 0.0;
            for (lat, lon) in [
                (a.min_lat, a.min_lon),
                (a.max_lat, a.min_lon),
                (a.min_lat, a.max_lon),
                (a.max_lat, a.max_lon),
            ] {
                m = m.max(distance_km(a.my_lat, a.my_lon, lat, lon));
            }
            format_double(m)
        }
        "LASTDV" => last_dv.to_string(),
        _ => return None,
    })
}

/// `VarConverter.ReplaceAllVars`: replace every `%NAME%` (case-insensitive name) in
/// `template`. An odd number of `%` or an unknown name is an error, as in C#.
/// `MINDISTKM` is always 0 (C# never computes it).
pub fn substitute(
    template: &str,
    args: &FeedArgs,
    now: DateTime<Utc>,
    last_dv: &str,
) -> Result<String, FeedError> {
    if !template.contains('%') {
        return Ok(template.to_string());
    }
    let parts: Vec<&str> = template.split('%').collect();
    if parts.len().is_multiple_of(2) {
        return Err(FeedError::Config(format!(
            "Number of separators is not an even number: {template}"
        )));
    }
    let mut s = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i % 2 == 0 {
            s.push_str(p);
        } else {
            let name = p.to_uppercase();
            let v = value(&name, args, now, last_dv)
                .ok_or_else(|| FeedError::Config(format!("Var identifier not found: {name}")))?;
            s.push_str(&v);
        }
    }
    Ok(s)
}
