//! Port of `AirScout.Core/PlaneInfo.cs`.

use crate::units::{ft_m, kts_kmh};
use crate::PlaneCategory;
use as_geo::latlon::GPoint;
use chrono::{DateTime, Utc};

/// Complete aircraft info: report, crossing results and the previous report.
/// Altitude in ft and speed in kts, as in the original.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaneInfo {
    pub time: DateTime<Utc>,
    pub call: String,
    pub reg: String,
    pub hex: String,
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: f64,
    pub track: f64,
    pub speed_kts: f64,
    pub type_code: String,
    pub manufacturer: String,
    pub model: String,
    pub category: PlaneCategory,
    pub from: String,
    pub to: String,
    pub vspeed: i32,

    // results of the crossing prediction
    pub int_point: Option<GPoint>,
    pub int_qrb: f64,
    pub alt_diff: f64,
    pub potential: i32,
    pub eps1: f64,
    pub eps2: f64,
    pub theta1: f64,
    pub theta2: f64,
    pub angle: f64,
    pub squint: f64,
    pub signal_strength: f64,
    pub ambiguous: bool,
    pub comment: String,

    // previous report, if any (C# `DateTime.MinValue` = none)
    pub old_time: Option<DateTime<Utc>>,
    pub old_lat: f64,
    pub old_lon: f64,
    pub old_alt_ft: f64,
    pub old_track: f64,
    pub old_speed_kts: f64,
}

impl Default for PlaneInfo {
    /// C# default constructor (`Time = UtcNow`, empty strings, zeros).
    fn default() -> Self {
        Self {
            time: Utc::now(),
            call: String::new(),
            reg: String::new(),
            hex: String::new(),
            lat: 0.0,
            lon: 0.0,
            alt_ft: 0.0,
            track: 0.0,
            speed_kts: 0.0,
            type_code: String::new(),
            manufacturer: String::new(),
            model: String::new(),
            category: PlaneCategory::None,
            from: String::new(),
            to: String::new(),
            vspeed: 0,
            int_point: None,
            int_qrb: f64::MAX,
            alt_diff: 0.0,
            potential: 0,
            eps1: 0.0,
            eps2: 0.0,
            theta1: 0.0,
            theta2: 0.0,
            angle: 0.0,
            squint: 0.0,
            signal_strength: f64::MIN,
            ambiguous: false,
            comment: String::new(),
            old_time: None,
            old_lat: 0.0,
            old_lon: 0.0,
            old_alt_ft: 0.0,
            old_track: 0.0,
            old_speed_kts: 0.0,
        }
    }
}

impl PlaneInfo {
    /// The full C# constructor. `From`/`To` stay empty (C# leaves them null).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        time: DateTime<Utc>,
        call: &str,
        reg: &str,
        hex: &str,
        lat: f64,
        lon: f64,
        track: f64,
        alt_ft: f64,
        speed_kts: f64,
        type_code: &str,
        manufacturer: &str,
        model: &str,
        category: PlaneCategory,
    ) -> Self {
        Self {
            time,
            call: call.to_string(),
            reg: reg.to_string(),
            hex: hex.to_string(),
            lat,
            lon,
            alt_ft,
            track,
            speed_kts,
            type_code: type_code.to_string(),
            manufacturer: manufacturer.to_string(),
            model: model.to_string(),
            category,
            ..Self::default()
        }
    }

    /// `Alt_m`.
    pub fn alt_m(&self) -> f64 {
        ft_m(self.alt_ft)
    }

    /// `Speed_kmh`.
    pub fn speed_kmh(&self) -> f64 {
        kts_kmh(self.speed_kts)
    }

    /// The C# copy constructor `PlaneInfo(PlaneInfo)`: copies everything except `From`, `To` and
    /// `VSpeed`, which stay at their defaults.
    pub fn copied(&self) -> Self {
        Self {
            from: String::new(),
            to: String::new(),
            vspeed: 0,
            ..self.clone()
        }
    }
}
