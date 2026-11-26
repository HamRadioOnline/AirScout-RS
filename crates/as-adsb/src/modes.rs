//! Port of `ModeSReply.cs`: the generic Mode-S reply `| DF 5 | CA 3 | payload 3/10 | PI/AP 3 |`.

use crate::{crc, AdsbError};

/// A raw Mode-S reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeSReply {
    /// Downlink format (0..31).
    pub df: u8,
    /// The three bits after the downlink format.
    pub ca: u8,
    /// ICAO address: the first three payload bytes for DF11/17/18, the address/parity field
    /// for DF0/4/5/16/20/21/24, zero otherwise.
    pub icao24: u32,
    /// 3 or 10 bytes after the DF/CA byte.
    pub payload: Vec<u8>,
    /// Parity field for DF11/17/18, zero otherwise.
    pub parity: u32,
}

fn byte(s: &str, i: usize) -> Result<u8, AdsbError> {
    let b = &s.as_bytes()[i..i + 2];
    if !b.iter().all(u8::is_ascii_hexdigit) {
        return Err(AdsbError::BadHex(s.to_string()));
    }
    // both bytes are ASCII hex digits, so this slice is valid UTF-8
    u8::from_str_radix(std::str::from_utf8(b).unwrap_or("00"), 16)
        .map_err(|_| AdsbError::BadHex(s.to_string()))
}

fn int24(b: &[u8]) -> u32 {
    (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2])
}

impl ModeSReply {
    /// Parse 14 or 28 hex characters (`new ModeSReply(raw)`).
    ///
    /// As in C#: for DF11/17/18 a non-hex pair in the payload becomes 0 (the exception is
    /// swallowed); elsewhere a non-hex pair is an error. Unknown DFs keep zero payload, address
    /// and parity. Non-ASCII input is rejected as [`AdsbError::BadHex`].
    pub fn from_hex(raw: &str) -> Result<Self, AdsbError> {
        let len = raw.len();
        if !raw.is_ascii() {
            return Err(AdsbError::BadHex(raw.to_string()));
        }
        if len != 14 && len != 28 {
            return Err(AdsbError::BadFormat(format!(
                "Raw message has invalid length: {raw}"
            )));
        }
        let first = byte(raw, 0)?;
        let ca = first & 0x7;
        let df = (first >> 3) & 0x1F;
        let mut payload = vec![0u8; (len - 8) / 2];
        let mut icao = [0u8; 3];
        let mut parity = [0u8; 3];
        match df {
            0 | 4 | 5 | 16 | 20 | 21 | 24 => {
                for (k, i) in (len - 6..len).step_by(2).enumerate() {
                    icao[k] = byte(raw, i)?;
                }
                for (k, i) in (2..len - 6).step_by(2).enumerate() {
                    payload[k] = byte(raw, i)?;
                }
            }
            11 | 17 | 18 => {
                for (k, i) in (2..len - 6).step_by(2).enumerate() {
                    payload[k] = byte(raw, i).unwrap_or(0);
                }
                icao.copy_from_slice(&payload[..3]);
                for (k, i) in (len - 6..len).step_by(2).enumerate() {
                    parity[k] = byte(raw, i)?;
                }
            }
            _ => {}
        }
        Ok(Self {
            df,
            ca,
            icao24: int24(&icao),
            payload,
            parity: int24(&parity),
        })
    }

    /// `RecalculatedParity`: CRC over the DF/CA byte and the payload.
    pub fn recalculated_parity(&self) -> u32 {
        let mut m = Vec::with_capacity(self.payload.len() + 1);
        m.push((self.df << 3) | self.ca);
        m.extend_from_slice(&self.payload);
        crc::parity(&m)
    }

    /// `CheckParity`.
    pub fn check_parity(&self) -> bool {
        self.recalculated_parity() == self.parity
    }

    /// `IsZeroParity`. The decoder accepts such replies without a CRC check
    /// (logic-findings #32).
    pub fn is_zero_parity(&self) -> bool {
        self.parity == 0
    }
}
