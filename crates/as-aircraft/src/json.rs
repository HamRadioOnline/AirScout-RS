//! JSON shapes: `AirScout.Core/PlaneJSON.cs` (AirScoutServer `planes.json`) and `HistSON.cs`.

use serde::{Deserialize, Serialize};

/// `PlaneJSON`: PascalCase properties, all nullable. `Time` is unix seconds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaneJson {
    pub hex: Option<String>,
    pub call: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Altitude [ft].
    pub alt: Option<f64>,
    pub track: Option<f64>,
    /// Speed [kts].
    pub speed: Option<f64>,
    #[serde(rename = "Type")]
    pub type_code: Option<String>,
    pub category: Option<i32>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub reg: Option<String>,
    pub time: Option<i32>,
}

/// `HistJSON`: FR24-style history record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistJson {
    pub id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub track: f64,
    pub altitude: f64,
    pub speed: f64,
    pub icon: i32,
    pub status: i32,
    pub timestamp: i32,
    pub on_ground: bool,
    pub callsign: String,
    pub squawk: Option<String>,
    pub data_source: i32,
    pub schedule: Option<ScheduleInfo>,
    pub ems: Option<EmsInfo>,
}

/// `ScheduleInfo`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduleInfo {
    pub std: Option<String>,
    pub etd: Option<String>,
    pub atd: Option<String>,
    pub sta: Option<String>,
    pub eta: Option<String>,
    pub ata: Option<String>,
}

/// `EMSInfo` (untyped values in the original).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmsInfo {
    pub agps: serde_json::Value,
    pub ias: serde_json::Value,
    pub mach: serde_json::Value,
    pub oat: serde_json::Value,
    pub tas: serde_json::Value,
    pub wind_speed: serde_json::Value,
    pub wind_direction: serde_json::Value,
}
