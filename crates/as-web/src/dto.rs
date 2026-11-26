//! The C# objects the web routes serialise, with their JSON member names, order and types.
//!
//! Newtonsoft writes the public fields of a class first, then its public properties,
//! then the properties of the base class (`SQLiteEntry.LastUpdated`,
//! `SQLiteDatabase/SQLiteDatabase/SQLiteEntry.cs:64`). Static members and `[JsonIgnore]`
//! members are left out. Every C# `double` is [`Num`], `short` is `i16`, `int` and enums
//! are `i32`, and `DateTime` is written by [`crate::json::date`].

use crate::json::Num;
use chrono::{DateTime, Utc};
use serde::Serialize;

/// `LocationDesignator` (`ScoutBase/ScoutBase.Stations/Location.cs:45-63`).
/// `GeoLocation` is `[JsonIgnore]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocationJson {
    #[serde(rename = "Elevation")]
    pub elevation: i16,
    #[serde(rename = "BestCaseElevation")]
    pub best_case_elevation: bool,
    #[serde(rename = "Call")]
    pub call: String,
    #[serde(rename = "Loc")]
    pub loc: String,
    #[serde(rename = "Lat")]
    pub lat: Num,
    #[serde(rename = "Lon")]
    pub lon: Num,
    /// `GEOSOURCE`.
    #[serde(rename = "Source")]
    pub source: i32,
    #[serde(rename = "Hits")]
    pub hits: i32,
    #[serde(rename = "LastUpdated", with = "crate::json::date")]
    pub last_updated: DateTime<Utc>,
}

/// `QRVDesignator` (`ScoutBase/ScoutBase.Stations/QRV.cs:49-54`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct QrvJson {
    #[serde(rename = "Call")]
    pub call: String,
    #[serde(rename = "Loc")]
    pub loc: String,
    /// `BAND`, the MHz value (`144`, `1296`).
    #[serde(rename = "Band")]
    pub band: i32,
    #[serde(rename = "AntennaHeight")]
    pub antenna_height: Num,
    #[serde(rename = "AntennaGain")]
    pub antenna_gain: Num,
    #[serde(rename = "Power")]
    pub power: Num,
    #[serde(rename = "LastUpdated", with = "crate::json::date")]
    pub last_updated: DateTime<Utc>,
}

/// `ElevationPathDesignator` (`ScoutBase/ScoutBase.Elevation/ElevationPath.cs:46-92`):
/// the fields `Valid` … `QRV2` (:84-92), then the properties (:46-80).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ElevationPathJson {
    #[serde(rename = "Valid")]
    pub valid: bool,
    #[serde(rename = "Selected")]
    pub selected: bool,
    #[serde(rename = "Location1")]
    pub location1: Option<LocationJson>,
    #[serde(rename = "Location2")]
    pub location2: Option<LocationJson>,
    #[serde(rename = "QRV1")]
    pub qrv1: Option<QrvJson>,
    #[serde(rename = "QRV2")]
    pub qrv2: Option<QrvJson>,
    #[serde(rename = "Bearing12")]
    pub bearing12: Num,
    #[serde(rename = "Bearing21")]
    pub bearing21: Num,
    #[serde(rename = "Distance")]
    pub distance: Num,
    #[serde(rename = "Lat1")]
    pub lat1: Num,
    #[serde(rename = "Lon1")]
    pub lon1: Num,
    #[serde(rename = "Lat2")]
    pub lat2: Num,
    #[serde(rename = "Lon2")]
    pub lon2: Num,
    #[serde(rename = "StepWidth")]
    pub step_width: Num,
    #[serde(rename = "Count")]
    pub count: i32,
    #[serde(rename = "Path")]
    pub path: Vec<i16>,
    #[serde(rename = "LastUpdated", with = "crate::json::date")]
    pub last_updated: DateTime<Utc>,
}

/// `PropagationPathDesignator` (`ScoutBase/ScoutBase.Propagation/PropagationPath.cs:40-94`):
/// the fields `Valid` … `QRV2` (:83-94), then the properties (:40-79).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PropagationPathJson {
    #[serde(rename = "Valid")]
    pub valid: bool,
    #[serde(rename = "Selected")]
    pub selected: bool,
    #[serde(rename = "LocalObstructed")]
    pub local_obstructed: bool,
    #[serde(rename = "LocalObstruction")]
    pub local_obstruction: Num,
    #[serde(rename = "Location1")]
    pub location1: Option<LocationJson>,
    #[serde(rename = "Location2")]
    pub location2: Option<LocationJson>,
    #[serde(rename = "QRV1")]
    pub qrv1: Option<QrvJson>,
    #[serde(rename = "QRV2")]
    pub qrv2: Option<QrvJson>,
    #[serde(rename = "Bearing12")]
    pub bearing12: Num,
    #[serde(rename = "Bearing21")]
    pub bearing21: Num,
    #[serde(rename = "Distance")]
    pub distance: Num,
    #[serde(rename = "Lat1")]
    pub lat1: Num,
    #[serde(rename = "Lon1")]
    pub lon1: Num,
    #[serde(rename = "h1")]
    pub h1: Num,
    #[serde(rename = "Lat2")]
    pub lat2: Num,
    #[serde(rename = "Lon2")]
    pub lon2: Num,
    #[serde(rename = "h2")]
    pub h2: Num,
    #[serde(rename = "QRG")]
    pub qrg: Num,
    #[serde(rename = "Radius")]
    pub radius: Num,
    #[serde(rename = "F1_Clearance")]
    pub f1_clearance: Num,
    #[serde(rename = "StepWidth")]
    pub step_width: Num,
    #[serde(rename = "Eps1_Min")]
    pub eps1_min: Num,
    #[serde(rename = "Eps2_Min")]
    pub eps2_min: Num,
    #[serde(rename = "LastUpdated", with = "crate::json::date")]
    pub last_updated: DateTime<Utc>,
}

/// `LatLon.GPoint` (`ScoutBase/ScoutBase.Core/LatLon.cs:18-21`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GPointJson {
    #[serde(rename = "Lat")]
    pub lat: Num,
    #[serde(rename = "Lon")]
    pub lon: Num,
}

/// `PlaneInfo` (`AirScout.Core/PlaneInfo.cs:22-99`): the fields `Track` (:49) and
/// `IntPoint` … `OldSpeed` (:79-99), then the properties (:22-77).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlaneInfoJson {
    #[serde(rename = "Track")]
    pub track: Num,
    #[serde(rename = "IntPoint")]
    pub int_point: Option<GPointJson>,
    #[serde(rename = "IntQRB")]
    pub int_qrb: Num,
    #[serde(rename = "AltDiff")]
    pub alt_diff: Num,
    #[serde(rename = "Potential")]
    pub potential: i32,
    #[serde(rename = "Eps1")]
    pub eps1: Num,
    #[serde(rename = "Eps2")]
    pub eps2: Num,
    #[serde(rename = "Theta1")]
    pub theta1: Num,
    #[serde(rename = "Theta2")]
    pub theta2: Num,
    #[serde(rename = "Angle")]
    pub angle: Num,
    #[serde(rename = "Squint")]
    pub squint: Num,
    #[serde(rename = "SignalStrength")]
    pub signal_strength: Num,
    #[serde(rename = "Ambiguous")]
    pub ambiguous: bool,
    #[serde(rename = "Comment")]
    pub comment: String,
    /// `None` is `DateTime.MinValue`, the value of a plane without an old position.
    #[serde(rename = "OldTime", with = "crate::json::date_or_min")]
    pub old_time: Option<DateTime<Utc>>,
    #[serde(rename = "OldLat")]
    pub old_lat: Num,
    #[serde(rename = "OldLon")]
    pub old_lon: Num,
    #[serde(rename = "OldAlt")]
    pub old_alt: Num,
    #[serde(rename = "OldTrack")]
    pub old_track: Num,
    #[serde(rename = "OldSpeed")]
    pub old_speed: Num,
    #[serde(rename = "Time", with = "crate::json::date")]
    pub time: DateTime<Utc>,
    #[serde(rename = "Call")]
    pub call: String,
    #[serde(rename = "Reg")]
    pub reg: String,
    #[serde(rename = "Hex")]
    pub hex: String,
    #[serde(rename = "Lat")]
    pub lat: Num,
    #[serde(rename = "Lon")]
    pub lon: Num,
    /// Feet.
    #[serde(rename = "Alt")]
    pub alt: Num,
    #[serde(rename = "Alt_m")]
    pub alt_m: Num,
    /// Knots.
    #[serde(rename = "Speed")]
    pub speed: Num,
    #[serde(rename = "Speed_kmh")]
    pub speed_kmh: Num,
    #[serde(rename = "Type")]
    pub type_: String,
    #[serde(rename = "Manufacturer")]
    pub manufacturer: String,
    #[serde(rename = "Model")]
    pub model: String,
    /// `PLANECATEGORY`.
    #[serde(rename = "Category")]
    pub category: i32,
    #[serde(rename = "From")]
    pub from: String,
    #[serde(rename = "To")]
    pub to: String,
    #[serde(rename = "VSpeed")]
    pub v_speed: i32,
}

/// `AirportDesignator` (`AirScout.Aircrafts/Airport.cs:34-40`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AirportJson {
    #[serde(rename = "ICAO")]
    pub icao: String,
    #[serde(rename = "IATA")]
    pub iata: String,
    #[serde(rename = "Lat")]
    pub lat: Num,
    #[serde(rename = "Lon")]
    pub lon: Num,
    #[serde(rename = "Alt")]
    pub alt: Num,
    #[serde(rename = "Airport")]
    pub airport: String,
    #[serde(rename = "Country")]
    pub country: String,
    #[serde(rename = "LastUpdated", with = "crate::json::date")]
    pub last_updated: DateTime<Utc>,
}

/// `Band` (`ScoutBase/ScoutBase.Core/Band.cs:56-60`) for `/bandvalues.json` and
/// `PlaneCategory` (`AirScout.Core/PlaneCategory.cs:24-28`) for `/aircraftcategories.json`.
/// `Name` is the display text (`"1.2G"`, `"Medium"`), `Value` the enum value.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NameValueJson {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Value")]
    pub value: i32,
}

/// One row of the `BandSettings` `DataTable` (`AirScout.Core/BandSettings.cs:38-45`), as
/// Newtonsoft writes `Path_Band_Settings`: one object per row, the column names as keys.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BandSettingJson {
    #[serde(rename = "BAND")]
    pub band: String,
    #[serde(rename = "K-FACTOR")]
    pub k_factor: Num,
    #[serde(rename = "F1-CLEARANCE")]
    pub f1_clearance: Num,
    #[serde(rename = "GROUNDCLEARANCE")]
    pub ground_clearance: Num,
    #[serde(rename = "MAXDISTANCE")]
    pub max_distance: Num,
    #[serde(rename = "MAXSQUINT")]
    pub max_squint: Num,
    #[serde(rename = "MAXELEVATION")]
    pub max_elevation: Num,
}
