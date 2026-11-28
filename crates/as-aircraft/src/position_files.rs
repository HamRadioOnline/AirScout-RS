//! Position file formats (JSON, CSV, `.hist`), ported from `MapDlg.cs`
//! (`Analysis_Planes_Save_*`, `Analysis_Planes_Load_*`) and `AirScout.Core/HistSON.cs`.
//!
//! Every reader is total: a bad record is skipped and counted, never a panic.
//! The FR24 web lookup of the original `.hist` loader is not ported (finding).

use crate::positions::AircraftPosition;
use crate::HistJson;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::{json, Value};
use std::io::{self, Write};

/// Result of a file import: the rows read and the number of records dropped.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ImportStats {
    pub imported: Vec<AircraftPosition>,
    pub skipped: usize,
}

const CSV_HEADER: &str = "time[utc];hex;call;lat[deg];lon[deg];alt[ft];track[deg];speed[kts]";

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// Writes a JSON array of position objects (`MapDlg.cs:9505-9532`). Unlike the original, the
/// field is `LastUpdated`, which the reader accepts, and no trailing comma is left.
pub fn write_json(w: &mut dyn Write, rows: &[AircraftPosition]) -> io::Result<()> {
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "Hex": r.hex, "Call": r.call, "Lat": r.lat, "Lon": r.lon, "Alt": r.alt_ft,
                "Track": r.track, "Speed": r.speed_kts,
                "LastUpdated": r.last_updated.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            })
        })
        .collect();
    serde_json::to_writer(&mut *w, &items).map_err(io::Error::from)?;
    writeln!(w)
}

/// Writes the `;`-separated CSV (`MapDlg.cs:9534-9562`).
pub fn write_csv(w: &mut dyn Write, rows: &[AircraftPosition]) -> io::Result<()> {
    writeln!(w, "{CSV_HEADER}")?;
    let clean = |s: &str| s.replace([';', '\r', '\n'], " ");
    for r in rows {
        writeln!(
            w,
            "{};{};{};{:.8};{:.8};{:.8};{:.8};{:.8}",
            r.last_updated.format("%Y-%m-%d %H:%M:%SZ"),
            clean(&r.hex),
            clean(&r.call),
            r.lat,
            r.lon,
            r.alt_ft,
            r.track,
            r.speed_kts
        )?;
    }
    Ok(())
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    for f in ["%Y-%m-%d %H:%M:%SZ", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(s, f) {
            return Some(n.and_utc());
        }
    }
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Number from a JSON number or numeric string; `None` if absent, null or not finite.
fn num(v: Option<&Value>) -> Option<f64> {
    let f = match v? {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    f.is_finite().then_some(f)
}

fn json_row(v: &Value) -> Option<AircraftPosition> {
    let o = v.as_object()?;
    let text = |k: &str| o.get(k).and_then(Value::as_str);
    let hex = text("Hex").filter(|h| !h.is_empty())?;
    let time = parse_time(text("LastUpdated").or_else(|| text("Time"))?)?;
    let n = |k: &str| num(o.get(k)).unwrap_or(0.0);
    Some(AircraftPosition {
        hex: hex.to_uppercase(),
        call: text("Call").unwrap_or("").to_uppercase(),
        lat: num(o.get("Lat"))?,
        lon: num(o.get("Lon"))?,
        alt_ft: n("Alt"),
        track: n("Track"),
        speed_kts: n("Speed"),
        last_updated: time,
    })
}

/// Splits the body of a JSON array/object (text after the opening bracket) into its top-level
/// elements, so a record that does not parse costs only itself. Stops at the matching close.
/// An empty body yields no elements; a blank element between commas is kept (counted as bad).
fn split_top_level(body: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0usize, 0usize);
    let (mut in_str, mut esc) = (false, false);
    let mut end = body.len();
    for (i, c) in body.char_indices() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '[' => depth += 1,
            '}' | ']' if depth == 0 => {
                end = i;
                break;
            }
            '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&body[start..end]);
    if out.len() == 1 && out[0].trim().is_empty() {
        out.clear();
    }
    out
}

/// Reads a JSON array (`MapDlg.cs:9606-9653`). Accepts `LastUpdated` or the C# `Time`.
pub fn read_json(text: &str) -> Result<ImportStats, String> {
    let text = strip_bom(text).trim_start();
    let mut st = ImportStats::default();
    if text.is_empty() {
        return Ok(st);
    }
    if !text.starts_with('[') {
        return Err("not a JSON array of positions".into());
    }
    for item in split_top_level(&text[1..]) {
        match serde_json::from_str::<Value>(item)
            .ok()
            .as_ref()
            .and_then(json_row)
        {
            Some(r) => st.imported.push(r),
            None => st.skipped += 1,
        }
    }
    Ok(st)
}

/// Reads the CSV format (`MapDlg.cs:9739-9796`). Columns are found by name.
pub fn read_csv(text: &str) -> Result<ImportStats, String> {
    let mut st = ImportStats::default();
    let mut lines = strip_bom(text).lines();
    let Some(header) = lines.next() else {
        return Ok(st);
    };
    let names: Vec<String> = header
        .split(';')
        .map(|c| {
            let c = c.trim().to_lowercase();
            c.split('[').next().unwrap_or("").to_string()
        })
        .collect();
    let idx = |n: &str| names.iter().position(|c| c == n);
    let need = |n: &str| idx(n).ok_or_else(|| format!("missing column '{n}'"));
    let (time_i, hex_i, lat_i, lon_i) = (need("time")?, need("hex")?, need("lat")?, need("lon")?);
    let (call_i, alt_i, track_i, speed_i) = (idx("call"), idx("alt"), idx("track"), idx("speed"));
    for line in lines {
        if !line.contains(';') {
            continue;
        }
        let f: Vec<&str> = line.split(';').collect();
        let get = |i: usize| f.get(i).map(|s| s.trim());
        let float = |i: usize| {
            get(i)
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|x| x.is_finite())
        };
        let opt = |i: Option<usize>| match i {
            None => Some(0.0),
            Some(i) => float(i),
        };
        let row = (|| {
            let hex = get(hex_i).filter(|h| !h.is_empty())?.to_uppercase();
            Some(AircraftPosition {
                hex,
                call: call_i.and_then(get).unwrap_or("").to_uppercase(),
                lat: float(lat_i)?,
                lon: float(lon_i)?,
                alt_ft: opt(alt_i)?,
                track: opt(track_i)?,
                speed_kts: opt(speed_i)?,
                last_updated: parse_time(get(time_i)?)?,
            })
        })();
        match row {
            Some(r) => st.imported.push(r),
            None => st.skipped += 1,
        }
    }
    Ok(st)
}

/// Reads a `.hist` file, a JSON object of FR24-style records (`MapDlg.cs:9655-9737`,
/// `HistSON.cs:20-56`). `find_by_call` maps a call sign to a known hex; the web lookup of the
/// original is not ported.
pub fn read_hist(
    text: &str,
    find_by_call: &dyn Fn(&str) -> Option<String>,
) -> Result<ImportStats, String> {
    let text = strip_bom(text).trim_start();
    let mut st = ImportStats::default();
    if text.is_empty() {
        return Ok(st);
    }
    if !text.starts_with('{') {
        return Err("not a JSON object of history records".into());
    }
    for entry in split_top_level(&text[1..]) {
        // Each entry is `"key": record`; parse it as a one-entry object.
        let rec = serde_json::from_str::<serde_json::Map<String, Value>>(&format!("{{{entry}}}"))
            .ok()
            .and_then(|m| m.into_values().next())
            .and_then(|v| serde_json::from_value::<HistJson>(v).ok());
        let Some(h) = rec else {
            st.skipped += 1;
            continue;
        };
        let call = if h.callsign.is_empty() {
            "[unknown]".to_string()
        } else {
            h.callsign
        };
        let hex = find_by_call(&call).unwrap_or(h.id);
        let Some(time) = DateTime::from_timestamp(i64::from(h.timestamp), 0) else {
            st.skipped += 1;
            continue;
        };
        if hex.is_empty()
            || ![h.latitude, h.longitude, h.track, h.altitude, h.speed]
                .iter()
                .all(|x| x.is_finite())
        {
            st.skipped += 1;
            continue;
        }
        st.imported.push(AircraftPosition {
            hex,
            call,
            lat: h.latitude,
            lon: h.longitude,
            alt_ft: h.altitude,
            track: h.track,
            speed_kts: h.speed,
            last_updated: time,
        });
    }
    Ok(st)
}
