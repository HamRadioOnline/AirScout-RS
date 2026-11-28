//! Port of `QRV.cs` (`QRVDesignator`) and the per-band defaults from `StationDatabase.cs`.

use crate::location::normalise_call;
use crate::StationError;
use as_geo::band::Band;
use as_geo::latlon::GPoint;
use as_geo::maidenhead;
use chrono::{DateTime, Utc};

/// The length of a stored QRV locator key.
pub(crate) const QRV_LOC_LEN: usize = 6;

/// Station equipment on one band.
#[derive(Clone, Debug, PartialEq)]
pub struct Qrv {
    pub call: String,
    pub loc: String,
    pub band: Band,
    /// Antenna height above ground [m].
    pub antenna_height: f64,
    /// Antenna gain [dBd].
    pub antenna_gain: f64,
    /// Power [W].
    pub power: f64,
    pub last_updated: DateTime<Utc>,
}

/// C#: `loc.ToUpper().Trim().Substring(0, 6)`; throws for shorter locators. The full ASCII
/// Maidenhead locator is validated before truncation: a coarse (2/4-character) or malformed
/// locator is an error, because `Qrv::new` and the storage/delete paths need a full key.
pub(crate) fn normalise_qrv_loc(loc: &str) -> Result<String, StationError> {
    let l = loc.trim();
    if !maidenhead::check(l) || l.len() < QRV_LOC_LEN {
        return Err(StationError::InvalidLocator(loc.to_string()));
    }
    Ok(l[..QRV_LOC_LEN].to_ascii_uppercase())
}

/// The six-character key for an exact QRV lookup. A valid coarse locator (2 or 4 characters)
/// returns `Ok(None)`: it can never match a stored six-character key. Malformed text is an error.
pub(crate) fn qrv_lookup_key(loc: &str) -> Result<Option<String>, StationError> {
    let l = loc.trim();
    if !maidenhead::check(l) {
        return Err(StationError::InvalidLocator(loc.to_string()));
    }
    if l.len() < QRV_LOC_LEN {
        return Ok(None);
    }
    Ok(Some(l[..QRV_LOC_LEN].to_ascii_uppercase()))
}

/// The six-character key derived from a trusted position inside the coarse locator `coarse`, or
/// `None` when the position is invalid or outside the coarse cell. A locator midpoint, a stored
/// `FromLoc` position or a terrain-selected point is positional fallback and must not be passed.
pub(crate) fn qrv_key_from_position(coarse: &str, p: GPoint) -> Option<String> {
    let key = maidenhead::loc_from_lat_lon(p.lat, p.lon, false, 3, false)
        .ok()
        .flatten()?;
    let coarse = coarse.trim();
    if coarse.is_empty() || !key.starts_with(&coarse.to_ascii_uppercase()) {
        return None;
    }
    Some(key)
}

impl Qrv {
    /// Full constructor: call trimmed/upper-cased, locator cut to 6 characters (error if shorter).
    pub fn new(
        call: &str,
        loc: &str,
        band: Band,
        antenna_height: f64,
        antenna_gain: f64,
        power: f64,
        last_updated: DateTime<Utc>,
    ) -> Result<Self, StationError> {
        Ok(Self {
            call: normalise_call(call),
            loc: normalise_qrv_loc(loc)?,
            band,
            antenna_height,
            antenna_gain,
            power,
            last_updated,
        })
    }
}

/// `QRVGetDefaultAntennaHeight` [m].
pub fn default_antenna_height(_band: Band) -> f64 {
    10.0
}

/// `QRVGetDefaultAntennaGain` [dBd].
pub fn default_antenna_gain(band: Band) -> f64 {
    match band {
        Band::B50M => 3.0,
        Band::B70M => 6.0,
        Band::B144M => 12.0,
        Band::B432M => 15.0,
        Band::B1_2G => 18.0,
        Band::B2_3G => 20.0,
        Band::B3_4G => 22.0,
        Band::B5_7G => 25.0,
        Band::B10G => 30.0,
        Band::B24G => 40.0,
        Band::B47G => 42.0,
        Band::B76G => 45.0,
        Band::None | Band::All => 0.0,
    }
}

/// `QRVGetDefaultPower` [W].
pub fn default_power(band: Band) -> f64 {
    match band {
        Band::B50M | Band::B70M | Band::B144M | Band::B432M => 100.0,
        Band::B1_2G => 50.0,
        Band::B2_3G => 20.0,
        Band::B3_4G => 15.0,
        Band::B5_7G => 10.0,
        Band::B10G => 5.0,
        Band::B24G => 0.2,
        Band::B47G => 0.05,
        Band::B76G => 0.01,
        Band::None | Band::All => 0.0,
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct QrvJson {
    call: String,
    loc: String,
    band: i32,
    #[serde(deserialize_with = "crate::de_f64")]
    antenna_height: f64,
    #[serde(deserialize_with = "crate::de_f64")]
    antenna_gain: f64,
    #[serde(deserialize_with = "crate::de_f64")]
    power: f64,
    last_updated: DateTime<Utc>,
}

/// `QRVFromJSON`: the server's `qrv.json`. Entries with a band value outside the enum are dropped
/// (C# would keep the raw number, which no lookup can ever match).
pub fn qrvs_from_json(json: &str) -> Result<Vec<Qrv>, serde_json::Error> {
    let v: Vec<QrvJson> = serde_json::from_str(json)?;
    Ok(v.into_iter()
        .filter_map(|q| {
            Some(Qrv {
                call: q.call,
                loc: q.loc,
                band: Band::from_i32(q.band)?,
                antenna_height: q.antenna_height,
                antenna_gain: q.antenna_gain,
                power: q.power,
                last_updated: q.last_updated,
            })
        })
        .collect())
}
