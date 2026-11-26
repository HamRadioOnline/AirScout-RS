//! binCraft decoders of the two C# plugins (`BinCraftParser.cs`, `PublicBinCraftParser.cs`),
//! mapped to [`FeedPlane`] the way each plugin's `GetPlanes` does.
//!
//! Layout as read by the C# code: header with `now` (u64 ms, read as two u32 by
//! BinCraft), element size (`stride`) at 8; records of `stride` bytes from offset `stride`
//! on; a trailing partial record is ignored. Any read past the end of the buffer fails the
//! whole buffer (C# throws and the plugin returns nothing).

use super::seconds_before;
use crate::{FeedError, FeedPlane};
use chrono::{DateTime, Utc};

/// Little-endian reads that fail instead of panicking.
struct Buf<'a>(&'a [u8]);

impl Buf<'_> {
    fn bytes<const N: usize>(&self, at: usize) -> Result<[u8; N], FeedError> {
        at.checked_add(N)
            .and_then(|end| self.0.get(at..end))
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| {
                FeedError::Parse(format!(
                    "binCraft: read of {N} bytes at {at} past the end ({} bytes)",
                    self.0.len()
                ))
            })
    }
    fn u8(&self, at: usize) -> Result<u8, FeedError> {
        Ok(self.bytes::<1>(at)?[0])
    }
    fn i16(&self, at: usize) -> Result<i16, FeedError> {
        Ok(i16::from_le_bytes(self.bytes(at)?))
    }
    fn u16(&self, at: usize) -> Result<u16, FeedError> {
        Ok(u16::from_le_bytes(self.bytes(at)?))
    }
    fn i32(&self, at: usize) -> Result<i32, FeedError> {
        Ok(i32::from_le_bytes(self.bytes(at)?))
    }
    fn u32(&self, at: usize) -> Result<u32, FeedError> {
        Ok(u32::from_le_bytes(self.bytes(at)?))
    }
    fn i64(&self, at: usize) -> Result<i64, FeedError> {
        Ok(i64::from_le_bytes(self.bytes(at)?))
    }
    /// `ReadAsciiString(buffer, start, length)` of the PublicBinCraft plugin: up to the
    /// first NUL, printable ASCII (space included) only, trimmed.
    fn ascii(&self, at: usize, len: usize) -> Result<String, FeedError> {
        let mut s = String::new();
        for i in at..at + len {
            let b = self.u8(i)?;
            if b == 0 {
                break;
            }
            if (32..127).contains(&b) {
                s.push(char::from(b));
            }
        }
        Ok(s.trim().to_string())
    }
}

/// Record offsets `stride, 2*stride, ...` while a whole record fits.
fn records(len: usize, stride: usize) -> impl Iterator<Item = usize> {
    (1..)
        .map(move |i| i * stride)
        .take_while(move |o| o.checked_add(stride).is_some_and(|end| end <= len))
}

fn hex(raw: u32) -> String {
    format!("{:06X}", raw & 0xFF_FFFF)
}

/// `BinCraftParser.Parse` + `BinCraft.GetPlanes`. Reproduces the suspect record offsets of
/// the C# parser: track at 24, ground speed at 26, seen_pos at 54 (logic-findings).
pub fn parse_bincraft(data: &[u8]) -> Result<Vec<FeedPlane>, FeedError> {
    let b = Buf(data);
    let now_low = b.u32(0)?;
    let now_high = b.u32(4)?;
    let stride = b.u32(8)? as i32;
    // header fields read (and range-checked) by the C# parser, unused here
    b.u32(12)?;
    b.u32(16)?;
    for at in [20, 22, 24, 26] {
        b.i16(at)?;
    }
    if stride <= 0 {
        // C# loops forever on stride 0 and reads before the buffer on a negative one
        return Err(FeedError::Parse(format!(
            "binCraft: bad element size {stride}"
        )));
    }
    let now = f64::from(now_low) / 1000.0 + f64::from(now_high) * 4_294_967.296;
    let filetime = DateTime::<Utc>::from_timestamp_millis((now * 1000.0) as i64)
        .ok_or_else(|| FeedError::Parse(format!("binCraft: bad time {now}")))?;
    let mut planes = Vec::new();
    for o in records(data.len(), stride as usize) {
        let icao = b.i32(o)? as u32;
        b.u16(o + 4)?;
        b.u16(o + 6)?;
        let lon = f64::from(b.i32(o + 8)?) / 1e6;
        let lat = f64::from(b.i32(o + 12)?) / 1e6;
        let alt = i32::from(b.i16(o + 20)?) * 25;
        b.i16(o + 22)?;
        let track = f64::from(b.i16(o + 24)?) / 90.0;
        let gs = f64::from(b.i16(o + 26)?) / 10.0;
        let seen_pos = f64::from(b.u16(o + 54)?) / 10.0;
        // the per-plane try/catch in C# drops a plane whose time cannot be computed
        let Some(time) = seconds_before(filetime, seen_pos) else {
            continue;
        };
        planes.push(FeedPlane {
            time: Some(time),
            hex: hex(icao),
            lat,
            lon,
            alt_ft: f64::from(alt),
            track,
            speed_kts: gs,
            ..FeedPlane::default()
        });
    }
    Ok(planes)
}

const OFF_SEEN: usize = 4;
const OFF_LON: usize = 8;
const OFF_LAT: usize = 12;
const OFF_BARO_RATE: usize = 16;
const OFF_BARO_ALT: usize = 20;
const OFF_GEOM_ALT: usize = 22;
const OFF_SQUAWK: usize = 32;
const OFF_GS: usize = 34;
const OFF_TRACK: usize = 40;
const OFF_MAG_HEADING: usize = 44;
const OFF_TRUE_HEADING: usize = 46;
const OFF_CATEGORY: usize = 64;
const OFF_AIRGROUND: usize = 68;
const OFF_VALID0: usize = 73;
const OFF_CALLSIGN: usize = 78;
const OFF_TYPECODE: usize = 88;
const OFF_REGISTRATION: usize = 92;
const OFF_SEEN_POS: usize = 108;

fn bit(b: u8, n: u8) -> bool {
    (b >> n) & 1 != 0
}

/// One record as `PublicBinCraftParser.ParseOne` reads it (every field is read, valid or
/// not, so a short record fails like in C#), mapped by `PublicBinCraft.GetPlanes`.
fn public_one(b: &Buf, o: usize, now: DateTime<Utc>) -> Result<Option<FeedPlane>, FeedError> {
    let v0 = b.u8(o + OFF_VALID0)?;
    let v1 = b.u8(o + OFF_VALID0 + 1)?;
    let v2 = b.u8(o + OFF_VALID0 + 2)?;
    let v3 = b.u8(o + OFF_VALID0 + 3)?;
    let position = bit(v0, 6);
    // C# reads a field only when its valid bit is set (`valid ? Read(..) : null`)
    // the scale functions repeat the C# arithmetic exactly (x / 1e6 is not x * 1e-6)
    let i16_if = |valid: bool, at: usize, f: fn(f64) -> f64| -> Result<Option<f64>, FeedError> {
        Ok(if valid {
            Some(f(f64::from(b.i16(o + at)?)))
        } else {
            None
        })
    };
    let i32_if = |valid: bool, at: usize, f: fn(f64) -> f64| -> Result<Option<f64>, FeedError> {
        Ok(if valid {
            Some(f(f64::from(b.i32(o + at)?)))
        } else {
            None
        })
    };
    let hex = hex(b.u32(o)?);
    let seen = f64::from(b.i32(o + OFF_SEEN)?) / 10.0;
    let lat = i32_if(position, OFF_LAT, |x| x / 1e6)?;
    let lon = i32_if(position, OFF_LON, |x| x / 1e6)?;
    let baro_rate = i16_if(bit(v2, 0), OFF_BARO_RATE, |x| x * 8.0)?;
    let baro_alt = i16_if(bit(v0, 4), OFF_BARO_ALT, |x| x * 25.0)?;
    let geom_alt = i16_if(bit(v0, 5), OFF_GEOM_ALT, |x| x * 25.0)?;
    if bit(v3, 2) {
        b.u16(o + OFF_SQUAWK)?;
    }
    let gs = i16_if(bit(v0, 7), OFF_GS, |x| x / 10.0)?;
    let track = i16_if(bit(v1, 3), OFF_TRACK, |x| x / 90.0)?;
    i16_if(bit(v1, 6), OFF_MAG_HEADING, |x| x)?;
    i16_if(bit(v1, 7), OFF_TRUE_HEADING, |x| x)?;
    b.u8(o + OFF_CATEGORY)?;
    let ground = b.u8(o + OFF_AIRGROUND)? & 0x0F == 1;
    let call = if bit(v0, 3) {
        b.ascii(o + OFF_CALLSIGN, 8)?
    } else {
        String::new()
    };
    let type_code = b.ascii(o + OFF_TYPECODE, 4)?;
    let reg = b.ascii(o + OFF_REGISTRATION, 12)?;
    let seen_pos = i32_if(position, OFF_SEEN_POS, |x| x / 10.0)?;
    let (Some(lat), Some(lon)) = (lat, lon) else {
        return Ok(None);
    };
    // no per-plane try/catch in C#: a time out of range fails the whole buffer
    let time = seconds_before(now, seen_pos.unwrap_or(seen))
        .ok_or_else(|| FeedError::Parse("binCraft: time out of range".into()))?;
    Ok(Some(FeedPlane {
        time: Some(time),
        hex,
        lat,
        lon,
        alt_ft: baro_alt.or(geom_alt).unwrap_or(f64::MIN),
        track: track.unwrap_or(f64::MIN),
        speed_kts: gs.unwrap_or(f64::MIN),
        call,
        reg,
        type_code,
        // `(int)double` truncates; the values are multiples of 8 anyway
        vspeed: baro_rate.map_or(0, |r| r as i32),
        ground,
        ..FeedPlane::default()
    }))
}

/// `PublicBinCraftParser.ParseHeader/ParseAircraft` + `PublicBinCraft.GetPlanes`.
/// Records without a valid position are skipped.
pub fn parse_public(data: &[u8]) -> Result<Vec<FeedPlane>, FeedError> {
    let b = Buf(data);
    let now_ms = b.i64(0)?;
    let stride = b.u32(8)?;
    b.u32(12)?;
    for at in [20, 22, 24, 26] {
        b.i16(at)?;
    }
    for at in [28, 32, 40, 44, 48] {
        b.u32(at)?;
    }
    let now = DateTime::<Utc>::from_timestamp_millis(now_ms)
        .ok_or_else(|| FeedError::Parse(format!("binCraft: bad time {now_ms}")))?;
    let stride = stride as i32;
    if stride <= 0 {
        return Ok(Vec::new());
    }
    let mut planes = Vec::new();
    for o in records(data.len(), stride as usize) {
        if let Some(p) = public_one(&b, o, now)? {
            planes.push(p);
        }
    }
    Ok(planes)
}
