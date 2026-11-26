//! Port of `ScoutBase.Core/Band.cs`: amateur radio bands handled by AirScout.

/// Band; the discriminant is the frequency in MHz as in the original `BAND` enum.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[repr(i32)]
pub enum Band {
    None = 0,
    B50M = 50,
    B70M = 70,
    B144M = 144,
    B432M = 432,
    B1_2G = 1296,
    B2_3G = 2320,
    B3_4G = 3400,
    B5_7G = 5760,
    B10G = 10368,
    B24G = 24048,
    B47G = 47088,
    B76G = 76032,
    All = 999_999_999,
}

const REAL_BANDS: [Band; 12] = [
    Band::B50M,
    Band::B70M,
    Band::B144M,
    Band::B432M,
    Band::B1_2G,
    Band::B2_3G,
    Band::B3_4G,
    Band::B5_7G,
    Band::B10G,
    Band::B24G,
    Band::B47G,
    Band::B76G,
];

impl Band {
    /// Display name (`StringCustomAttribute` in the original).
    pub const fn name(&self) -> &'static str {
        match self {
            Band::None => "None",
            Band::B50M => "50M",
            Band::B70M => "70M",
            Band::B144M => "144M",
            Band::B432M => "432M",
            Band::B1_2G => "1.2G",
            Band::B2_3G => "2.3G",
            Band::B3_4G => "3.4G",
            Band::B5_7G => "5.7G",
            Band::B10G => "10G",
            Band::B24G => "24G",
            Band::B47G => "47G",
            Band::B76G => "76G",
            Band::All => "All",
        }
    }

    /// `Bands.ParseStringValue`: case-insensitive; unknown strings give [`Band::None`].
    pub fn parse(s: &str) -> Band {
        let t = s.trim();
        if t.eq_ignore_ascii_case("NONE") {
            return Band::None;
        }
        if t.eq_ignore_ascii_case("ALL") {
            return Band::All;
        }
        REAL_BANDS
            .iter()
            .copied()
            .find(|b| b.name().eq_ignore_ascii_case(s))
            .unwrap_or(Band::None)
    }

    /// Enum value (MHz, as stored in the original databases and JSON) to band.
    pub fn from_i32(v: i32) -> Option<Band> {
        match v {
            0 => Some(Band::None),
            999_999_999 => Some(Band::All),
            _ => REAL_BANDS.iter().copied().find(|b| *b as i32 == v),
        }
    }

    /// All real bands (without `None` and `All`), ascending.
    pub fn all_except_none_and_all() -> &'static [Band] {
        &REAL_BANDS
    }

    /// `Bands.Next`: next band, clamped at the top.
    pub fn next(self) -> Band {
        let n = REAL_BANDS.len();
        let i = match REAL_BANDS.iter().position(|b| *b == self) {
            None => 0,
            Some(i) if i > n - 2 => n - 2,
            Some(i) => i,
        };
        REAL_BANDS[i + 1]
    }

    /// `Bands.Previous`: previous band, clamped at the bottom.
    pub fn previous(self) -> Band {
        let n = REAL_BANDS.len();
        let i = match REAL_BANDS.iter().position(|b| *b == self) {
            None => 1,
            Some(i) if i < 1 => 1,
            Some(i) if i > n - 1 => n - 1,
            Some(i) => i,
        };
        REAL_BANDS[i - 1]
    }

    /// Frequency in MHz.
    pub fn to_mhz(self) -> f64 {
        self as i32 as f64
    }
    /// Frequency in GHz.
    pub fn to_ghz(self) -> f64 {
        self as i32 as f64 / 1000.0
    }
    /// Frequency in Hz.
    pub fn to_hz(self) -> f64 {
        self as i32 as f64 * 1_000_000.0
    }
    /// Frequency in kHz.
    pub fn to_khz(self) -> f64 {
        self as i32 as f64 * 1000.0
    }
}
