//! Decode the packed 112-byte readsb BinCraft layouts supported by both feeds.

use crate::{FeedError, FeedPlane};
use as_aircraft::AltitudeProvenance;
use chrono::{DateTime, Duration, Utc};

const RECORD_BYTES: usize = 112;
const VERSION_OFFSET: usize = 40;
const SUPPORTED_VERSIONS: [u32; 2] = [20240218, 20250403];
const OFF_LON: usize = 8;
const OFF_LAT: usize = 12;
const OFF_BARO_RATE: usize = 16;
const OFF_BARO_ALT: usize = 20;
const OFF_GEOM_ALT: usize = 22;
const OFF_GS: usize = 34;
const OFF_TRACK: usize = 40;
const OFF_AIRGROUND: usize = 68;
const OFF_VALID0: usize = 73;
const OFF_CALLSIGN: usize = 78;
const OFF_TYPECODE: usize = 88;
const OFF_REGISTRATION: usize = 92;
const OFF_SEEN_POS: usize = 108;

/// Fixed-width reads take a complete slot, never bytes from its neighbors.
struct Slot<'a>(&'a [u8; RECORD_BYTES]);

impl Slot<'_> {
    fn bytes<const N: usize>(&self, offset: usize) -> [u8; N] {
        self.0[offset..offset + N]
            .try_into()
            .expect("field is within the supported layout")
    }

    fn i16(&self, offset: usize) -> i16 {
        i16::from_le_bytes(self.bytes(offset))
    }

    fn i32(&self, offset: usize) -> i32 {
        i32::from_le_bytes(self.bytes(offset))
    }

    fn u32(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes(offset))
    }

    fn ascii(&self, offset: usize, length: usize) -> String {
        self.0[offset..offset + length]
            .iter()
            .copied()
            .take_while(|&byte| byte != 0)
            .filter(|byte| (32..127).contains(byte))
            .map(char::from)
            .collect::<String>()
            .trim()
            .to_owned()
    }
}

fn bit(flags: u8, index: u8) -> bool {
    flags & (1 << index) != 0
}

fn observation(slot: Slot<'_>, now: DateTime<Utc>, public: bool) -> Option<FeedPlane> {
    // The packed validity flags and field offsets follow readsb's aircraft.h.
    let flags = slot.0[OFF_VALID0];
    if !bit(flags, 6) {
        return None;
    }
    let lat = f64::from(slot.i32(OFF_LAT)) / 1e6;
    let lon = f64::from(slot.i32(OFF_LON)) / 1e6;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let age = slot.i32(OFF_SEEN_POS);
    if age < 0 {
        return None;
    }
    let time = now.checked_sub_signed(Duration::milliseconds(i64::from(age) * 100))?;

    let measurement =
        |valid: bool, offset, scale| valid.then(|| f64::from(slot.i16(offset)) * scale);
    let baro_alt = measurement(bit(flags, 4), OFF_BARO_ALT, 25.0);
    let geom_alt = measurement(bit(flags, 5), OFF_GEOM_ALT, 25.0);
    let (alt_ft, altitude_provenance) = match (baro_alt, geom_alt) {
        (Some(alt), _) => (alt, AltitudeProvenance::PressureEstimate),
        (None, Some(alt)) => (alt, AltitudeProvenance::Geometric),
        (None, None) => (f64::MIN, AltitudeProvenance::Unknown),
    };
    let mut plane = FeedPlane {
        time: Some(time),
        hex: format!("{:06X}", slot.u32(0) & 0xFF_FFFF),
        lat,
        lon,
        alt_ft,
        altitude_provenance,
        track: if bit(slot.0[OFF_VALID0 + 1], 3) {
            f64::from(slot.i16(OFF_TRACK)) / 90.0
        } else {
            f64::MIN
        },
        speed_kts: if bit(flags, 7) {
            f64::from(slot.i16(OFF_GS)) / 10.0
        } else {
            f64::MIN
        },
        ground: slot.0[OFF_AIRGROUND] & 0x0f == 1,
        ..FeedPlane::default()
    };
    if public {
        if bit(flags, 3) {
            plane.call = slot.ascii(OFF_CALLSIGN, 8);
        }
        plane.reg = slot.ascii(OFF_REGISTRATION, 12);
        plane.type_code = slot.ascii(OFF_TYPECODE, 4);
        if bit(slot.0[OFF_VALID0 + 2], 0) {
            plane.vspeed = i32::from(slot.i16(OFF_BARO_RATE)) * 8;
        }
    }
    Some(plane)
}

fn parse(data: &[u8], public: bool) -> Result<Vec<FeedPlane>, FeedError> {
    let min_header_bytes = if public { 52 } else { 28 };
    let fail_header = || FeedError::Parse("incomplete BinCraft header".into());
    let header = data.get(..min_header_bytes).ok_or_else(fail_header)?;
    let now_ms = i64::from_le_bytes(header[..8].try_into().expect("complete fixed header"));
    let now = DateTime::<Utc>::from_timestamp_millis(now_ms)
        .ok_or_else(|| FeedError::Parse("BinCraft time out of range".into()))?;
    let stride = u32::from_le_bytes(header[8..12].try_into().expect("complete fixed header"));
    if public && stride == 0 {
        return Ok(Vec::new());
    }
    if stride != RECORD_BYTES as u32 {
        return Err(FeedError::Parse("unsupported BinCraft stride".into()));
    }
    let (slots, _) = data.as_chunks::<RECORD_BYTES>();
    let (header, records) = slots.split_first().ok_or_else(fail_header)?;
    if !SUPPORTED_VERSIONS.contains(&Slot(header).u32(VERSION_OFFSET)) {
        return Err(FeedError::Parse("unsupported BinCraft version".into()));
    }
    Ok(records
        .iter()
        .filter_map(|record| observation(Slot(record), now, public))
        .collect())
}

/// Decode BinCraft reports from a receiver.
pub fn parse_bincraft(data: &[u8]) -> Result<Vec<FeedPlane>, FeedError> {
    parse(data, false)
}

/// Decode PublicBinCraft reports, including identification and vertical speed.
pub fn parse_public(data: &[u8]) -> Result<Vec<FeedPlane>, FeedError> {
    parse(data, true)
}
