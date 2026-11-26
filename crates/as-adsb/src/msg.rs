//! Port of `Decoder.GenericDecoder` and the extended-squitter message classes
//! (`IdentificationMsg`, `AirbornePositionMsg`, `SurfacePositionMsg`, `VelocityOverGroundMsg`,
//! `AirspeedHeadingMsg`).

use crate::{AdsbError, ModeSReply};

/// Aircraft identification (TC 1-4).
#[derive(Debug, Clone, PartialEq)]
pub struct Identification {
    pub icao24: u32,
    pub tc: u8,
    pub emitter_category: u8,
    /// Eight characters, not trimmed (C# `getIdentity`).
    pub call: String,
}

/// Airborne position (TC 9-18 barometric, 20-22 GNSS height).
#[derive(Debug, Clone, PartialEq)]
pub struct AirbornePosition {
    pub icao24: u32,
    pub tc: u8,
    pub odd: bool,
    pub cpr_lat: u32,
    pub cpr_lon: u32,
    pub has_position: bool,
    /// `Altitude` when `HasValidAltitude`. TC 20-22 report 0 (GNSS height is not decoded).
    pub alt_ft: Option<i32>,
    pub surveillance_status: u8,
    pub nic_suppl_b: bool,
    pub utc: bool,
}

/// Surface position (TC 5-8).
#[derive(Debug, Clone, PartialEq)]
pub struct SurfacePosition {
    pub icao24: u32,
    pub tc: u8,
    pub odd: bool,
    pub cpr_lat: u32,
    pub cpr_lon: u32,
    pub has_position: bool,
    /// Movement field (0..127).
    pub movement: u8,
    pub heading_valid: bool,
    /// Ground track field (0..127).
    pub ground_track: u8,
}

/// Velocity over ground (TC 19, subtype 1/2).
#[derive(Debug, Clone, PartialEq)]
pub struct VelocityOverGround {
    pub icao24: u32,
    pub supersonic: bool,
    pub velocity_valid: bool,
    /// `Velocity` [kts].
    pub speed_kts: i32,
    /// `EastToWestVelocity` (west positive).
    pub ew: i32,
    /// `NorthToSouthVelocity` (south positive).
    pub ns: i32,
    /// `Heading` [deg].
    pub heading_deg: i32,
    pub vrate_valid: bool,
    /// `VerticalRate`: ft/min × 0.00508, truncated (m/s).
    pub vrate_ms: i32,
    pub geo_minus_baro_valid: bool,
    pub geo_minus_baro_ft: i32,
}

/// Airspeed and heading (TC 19, subtype 3/4).
#[derive(Debug, Clone, PartialEq)]
pub struct AirspeedHeading {
    pub icao24: u32,
    pub supersonic: bool,
    /// Always false: C# never sets it (logic-findings #34).
    pub airspeed_valid: bool,
    pub airspeed_kts: i32,
    pub true_airspeed: bool,
    pub heading_valid: bool,
    pub heading_deg: i32,
    pub vrate_valid: bool,
    /// `VerticalRate` in ft/min (unlike [`VelocityOverGround::vrate_ms`]).
    pub vrate_ftmin: i32,
    pub geo_minus_baro_valid: bool,
    pub geo_minus_baro_ft: i32,
}

/// A decoded reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Identification(Identification),
    AirbornePosition(AirbornePosition),
    SurfacePosition(SurfacePosition),
    VelocityOverGround(VelocityOverGround),
    AirspeedHeading(AirspeedHeading),
    /// Anything else: a reply failing the parity check, a DF other than 17/18 (`tc: None`), or
    /// an extended squitter of an unsupported type (`tc: Some(..)`).
    Other {
        df: u8,
        tc: Option<u8>,
        icao24: u32,
    },
}

fn map_char(d: u8) -> char {
    match d {
        1..=26 => (b'A' + d - 1) as char,
        48..=57 => (b'0' + d - 48) as char,
        _ => ' ',
    }
}

fn identification(icao24: u32, tc: u8, m: &[u8; 7]) -> Identification {
    let mut id = [0u8; 8];
    for i in (1..=8usize).rev() {
        let mut byte_off = (i * 6) / 8;
        let bit_off = (i * 6) % 8;
        if bit_off == 0 {
            id[i - 1] = m[byte_off] & 0x3F;
        } else {
            byte_off += 1;
            id[i - 1] = (m[byte_off] >> (8 - bit_off)) & (0x3F >> (6 - bit_off));
            if bit_off < 6 {
                id[i - 1] |= (m[byte_off - 1] << bit_off) & 0x3F;
            }
        }
    }
    Identification {
        icao24,
        tc,
        emitter_category: m[0] & 0x7,
        call: id.iter().map(|&d| map_char(d)).collect(),
    }
}

fn gray_to_bin(gray: i32, bitlength: u32) -> i32 {
    let mut result = 0;
    for i in (0..bitlength).rev() {
        result |= (((1 << (i + 1)) & result) >> 1) ^ ((1 << i) & gray);
    }
    result
}

/// `AirbornePositionMsg.Altitude` for the 12-bit altitude field.
pub fn altitude(tc: u8, enc: i32) -> i32 {
    if tc >= 20 {
        return 0;
    }
    if enc & 0x10 != 0 {
        let n = (enc & 0xF) | ((enc & 0xFE0) >> 1);
        return (f64::from(25 * n - 1000) + 0.5) as i32;
    }
    let bit = |mask: i32, shift: u32| (enc & mask) >> shift;
    let (c1, a1, c2, a2, c4, a4) = (
        bit(0x800, 11),
        bit(0x400, 10),
        bit(0x200, 9),
        bit(0x100, 8),
        bit(0x080, 7),
        bit(0x040, 6),
    );
    let (b1, b2, d2, b4, d4) = (
        bit(0x020, 5),
        bit(0x008, 3),
        bit(0x004, 2),
        bit(0x002, 1),
        enc & 0x001,
    );
    let n500 = gray_to_bin(
        d2 << 7 | d4 << 6 | a1 << 5 | a2 << 4 | a4 << 3 | b1 << 2 | b2 << 1 | b4,
        8,
    );
    let mut n100 = gray_to_bin(c1 << 2 | c2 << 1 | c4, 3) - 1;
    if n100 == 6 {
        n100 = 4;
    }
    if n500 % 2 != 0 {
        n100 = 4 - n100;
    }
    (f64::from(-1200 + n500 * 500 + n100 * 100) + 0.5) as i32
}

fn cpr_fields(m: &[u8; 7]) -> (bool, u32, u32) {
    let odd = (m[2] >> 2) & 1 == 1;
    let lat =
        ((u32::from(m[2] & 0x3) << 15) | (u32::from(m[3]) << 7) | u32::from(m[4] >> 1)) & 0x1FFFF;
    let lon = ((u32::from(m[4] & 0x1) << 16) | (u32::from(m[5]) << 8) | u32::from(m[6])) & 0x1FFFF;
    (odd, lat, lon)
}

fn airborne(icao24: u32, tc: u8, m: &[u8; 7]) -> AirbornePosition {
    let enc = ((i32::from(m[1]) << 4) | ((i32::from(m[2]) >> 4) & 0xF)) & 0xFFF;
    let (odd, cpr_lat, cpr_lon) = cpr_fields(m);
    AirbornePosition {
        icao24,
        tc,
        odd,
        cpr_lat,
        cpr_lon,
        has_position: tc != 0,
        alt_ft: (enc != 0).then(|| altitude(tc, enc)),
        surveillance_status: (m[0] >> 1) & 0x3,
        nic_suppl_b: m[0] & 0x1 == 1,
        utc: (m[2] >> 3) & 1 == 1,
    }
}

fn surface(icao24: u32, tc: u8, m: &[u8; 7]) -> SurfacePosition {
    let (odd, cpr_lat, cpr_lon) = cpr_fields(m);
    SurfacePosition {
        icao24,
        tc,
        odd,
        cpr_lat,
        cpr_lon,
        has_position: tc != 0,
        movement: (((m[0] & 0x7) << 4) | ((m[1] & 0xF0) >> 4)) & 0x7F,
        heading_valid: m[1] & 0x8 != 0,
        ground_track: (((m[1] & 0x7) << 4) | ((m[2] & 0xF0) >> 4)) & 0x7F,
    }
}

impl SurfacePosition {
    /// `HasGroundSpeed`.
    pub fn has_ground_speed(&self) -> bool {
        (1..=124).contains(&self.movement)
    }

    /// `GroundSpeed` [m/s]. Movement 0 and 125-127 are an error, as in C#.
    pub fn ground_speed(&self) -> Result<f64, AdsbError> {
        let m = f64::from(self.movement);
        let speed = match self.movement {
            1 => 0.0,
            2..=8 => 0.125 + (m - 2.0) * 0.125,
            9..=12 => 1.0 + (m - 9.0) * 0.25,
            13..=38 => 2.0 + (m - 13.0) * 0.5,
            39..=93 => 15.0 + (m - 39.0),
            94..=108 => 70.0 + (m - 94.0) * 2.0,
            109..=123 => 100.0 + (m - 109.0) * 5.0,
            124 => 175.0,
            _ => {
                return Err(AdsbError::MissingInformation(
                    "Ground speed info not available!".into(),
                ))
            }
        };
        Ok(speed * 0.514444)
    }

    /// `Heading` [deg].
    pub fn heading(&self) -> f64 {
        f64::from(self.ground_track) * 360.0 / 128.0
    }
}

/// Shared vertical-rate and geo-minus-baro fields of TC 19. The "not available" sentinels
/// (-1) can never occur after the shifts and factors (logic-findings #35).
fn vrate_geo(m: &[u8; 7]) -> (bool, i32, bool, i32, bool) {
    let down = m[4] & 0x08 != 0;
    let raw = i32::from(m[4] & 0x07) << 6 | i32::from(m[5] >> 2 & 0x3F);
    let vrate = i32::from(((raw - 1) << 6) as i16);
    let mut geo = i32::from((((i32::from(m[6] & 0x7F)) - 1) * 25) as i16);
    let geo_valid = geo != -1;
    if m[6] & 0x80 != 0 {
        geo = -geo;
    }
    (vrate != -1, vrate, geo_valid, geo, down)
}

fn velocity(icao24: u32, subtype: u8, m: &[u8; 7]) -> VelocityOverGround {
    let mut valid = true;
    let west = m[1] & 0x4 != 0;
    let mut ew = i32::from(((i32::from(m[1] & 0x3) << 8 | i32::from(m[2])) - 1) as i16);
    if ew == -1 {
        valid = false;
    }
    if subtype == 2 {
        ew <<= 2;
    }
    let south = m[3] & 0x80 != 0;
    let mut ns =
        i32::from(((i32::from(m[3] & 0x7F) << 3 | i32::from(m[4] >> 5 & 0x07)) - 1) as i16);
    if ns == -1 {
        valid = false;
    }
    if subtype == 2 {
        ns <<= 2;
    }
    let (vrate_valid, vrate, geo_valid, geo, down) = vrate_geo(m);
    // `(int)(x + 0.5)` truncates toward zero, so negative components come out one too small in
    // magnitude (logic-findings #33).
    let ew_out = (f64::from(if west { ew } else { -ew }) + 0.5) as i32;
    let ns_out = (f64::from(if south { ns } else { -ns }) + 0.5) as i32;
    let angle =
        (f64::from(-ew_out).atan2(f64::from(-ns_out)) * 180.0 / std::f64::consts::PI) as i32;
    VelocityOverGround {
        icao24,
        supersonic: subtype == 2,
        velocity_valid: valid,
        speed_kts: f64::from(ns * ns + ew * ew).sqrt() as i32,
        ew: ew_out,
        ns: ns_out,
        heading_deg: if angle < 0 { 360 + angle } else { angle },
        vrate_valid,
        vrate_ms: (f64::from(if down { -vrate } else { vrate }) * 0.00508) as i32,
        geo_minus_baro_valid: geo_valid,
        geo_minus_baro_ft: geo,
    }
}

fn airspeed(icao24: u32, subtype: u8, m: &[u8; 7]) -> AirspeedHeading {
    let mut airspeed =
        i32::from(((i32::from(m[3] & 0x7F) << 3 | i32::from(m[4] >> 5 & 0x07)) - 1) as i16);
    // C# tests subtype 2 here, which never reaches this message type (logic-findings #34).
    if subtype == 2 {
        airspeed <<= 2;
    }
    let (vrate_valid, vrate, geo_valid, geo, _) = vrate_geo(m);
    AirspeedHeading {
        icao24,
        supersonic: subtype == 4,
        airspeed_valid: false,
        airspeed_kts: airspeed,
        true_airspeed: m[3] & 0x80 != 0,
        heading_valid: m[1] & 0x4 != 0,
        heading_deg: (f64::from(i32::from(m[1] & 0x3) << 8 | i32::from(m[2])) * 360.0 / 1024.0)
            as i32,
        vrate_valid,
        vrate_ftmin: vrate,
        geo_minus_baro_valid: geo_valid,
        geo_minus_baro_ft: geo,
    }
}

/// `Decoder.GenericDecoder`: parse `raw` and decode DF17/18 replies that pass the parity check
/// (or carry an all-zero parity field).
pub fn decode(raw: &str) -> Result<Msg, AdsbError> {
    let r = ModeSReply::from_hex(raw)?;
    let other = Msg::Other {
        df: r.df,
        tc: None,
        icao24: r.icao24,
    };
    if !(r.is_zero_parity() || r.check_parity()) || !(r.df == 17 || r.df == 18) {
        return Ok(other);
    }
    // `new ExtendedSquitter` reads payload[3..10]; a 14-character DF17 throws
    // IndexOutOfRangeException in C#.
    let m: [u8; 7] = r
        .payload
        .get(3..10)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| AdsbError::BadFormat(format!("Extended squitter too short: {raw}")))?;
    let tc = (m[0] >> 3) & 0x1F;
    let icao = r.icao24;
    Ok(match tc {
        1..=4 => Msg::Identification(identification(icao, tc, &m)),
        5..=8 => Msg::SurfacePosition(surface(icao, tc, &m)),
        9..=18 | 20..=22 => Msg::AirbornePosition(airborne(icao, tc, &m)),
        19 => match m[0] & 0x7 {
            st @ (1 | 2) => Msg::VelocityOverGround(velocity(icao, st, &m)),
            st @ (3 | 4) => Msg::AirspeedHeading(airspeed(icao, st, &m)),
            _ => Msg::Other {
                df: r.df,
                tc: Some(tc),
                icao24: icao,
            },
        },
        _ => Msg::Other {
            df: r.df,
            tc: Some(tc),
            icao24: icao,
        },
    })
}
