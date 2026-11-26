//! Port of `Location.cs` (`LocationDesignator`) and `GEOSOURCE`.

use chrono::{DateTime, Utc};

/// `GEOSOURCE`: where a station position came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(i32)]
pub enum GeoSource {
    /// `UNKONWN` (sic) in the original.
    #[default]
    Unknown = 0,
    FromLoc = 1,
    FromUser = 2,
    FromBest = 3,
}

impl GeoSource {
    /// Enum value to source; values outside the enum map to `Unknown`.
    pub fn from_i32(v: i32) -> GeoSource {
        match v {
            1 => GeoSource::FromLoc,
            2 => GeoSource::FromUser,
            3 => GeoSource::FromBest,
            _ => GeoSource::Unknown,
        }
    }
}

/// A known position of a call.
#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub call: String,
    pub loc: String,
    pub lat: f64,
    pub lon: f64,
    pub source: GeoSource,
    pub hits: i32,
    pub last_updated: DateTime<Utc>,
}

impl Location {
    /// Full constructor: the call is trimmed and upper-cased like in C#; `loc` is kept as given.
    pub fn new(
        call: &str,
        loc: &str,
        lat: f64,
        lon: f64,
        source: GeoSource,
        hits: i32,
        last_updated: DateTime<Utc>,
    ) -> Self {
        Self {
            call: normalise_call(call),
            loc: loc.to_string(),
            lat,
            lon,
            source,
            hits,
            last_updated,
        }
    }
}

pub(crate) fn normalise_call(call: &str) -> String {
    call.to_uppercase().trim().to_string()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LocationJson {
    call: String,
    loc: String,
    #[serde(deserialize_with = "crate::de_f64")]
    lat: f64,
    #[serde(deserialize_with = "crate::de_f64")]
    lon: f64,
    source: i32,
    hits: i32,
    last_updated: DateTime<Utc>,
}

/// `LocationFromJSON`: the server's `locations.json`. Like the C# deserializer, values are taken
/// as they are (no call normalisation).
pub fn locations_from_json(json: &str) -> Result<Vec<Location>, serde_json::Error> {
    let v: Vec<LocationJson> = serde_json::from_str(json)?;
    Ok(v.into_iter()
        .map(|l| Location {
            call: l.call,
            loc: l.loc,
            lat: l.lat,
            lon: l.lon,
            source: GeoSource::from_i32(l.source),
            hits: l.hits,
            last_updated: l.last_updated,
        })
        .collect())
}
