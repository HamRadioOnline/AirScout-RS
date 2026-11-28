//! CAT rig control settings (stage 4b). Defaults from `AirScout.CAT/Properties/Settings.Designer.cs`
//! (line numbers below). The serial enums mirror `as-cat`'s own; `as-settings` does not depend
//! on `as-cat`, and the mapping lives in `as_cat::from_settings`.

use serde::{Deserialize, Deserializer, Serialize};

/// CAT settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cat {
    /// `CAT_Activate` (:28).
    pub activate: bool,
    /// Pause between two polling passes [ms] (`CAT_Update`, :40).
    pub update_ms: u64,
    /// Rig engine. An unknown value (for example from a later build) or a value that is not
    /// text loads as `None`; the name matches in any letter case.
    #[serde(deserialize_with = "lenient_engine")]
    pub engine: CatEngine,
    pub rigctld_host: String,
    pub rigctld_port: u16,
    /// Hamlib rig model number (1: Hamlib Dummy).
    pub hamlib_model: u32,
    /// OmniRig rig number 1-4 (`[OmniRig] Rig n`); the app clamps it.
    pub omnirig_rig: u8,
    /// `CAT_PortName` (:76).
    pub port_name: String,
    /// `CAT_Baudrate` (:88).
    pub baudrate: u32,
    /// `CAT_DataBits` (:100).
    pub data_bits: u8,
    /// `CAT_Parity` (:112).
    pub parity: Parity,
    /// `CAT_StopBits` (:124).
    pub stop_bits: StopBits,
    /// `CAT_RTS` (:136).
    pub rts: FlowControl,
    /// `CAT_DTR` (:148).
    pub dtr: FlowControl,
    /// Per-command timeout [ms] (`CAT_Timeout`, :172).
    pub timeout_ms: u64,
}

impl Default for Cat {
    fn default() -> Self {
        Self {
            activate: false,
            update_ms: 500,
            engine: CatEngine::None,
            rigctld_host: "localhost".into(),
            rigctld_port: 4532,
            hamlib_model: 1,
            omnirig_rig: 1,
            port_name: String::new(),
            baudrate: 9600,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            rts: FlowControl::Low,
            dtr: FlowControl::Low,
            timeout_ms: 500,
        }
    }
}

/// CAT engine. 4c adds OmniRig.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CatEngine {
    #[default]
    None,
    Rigctld,
    Hamlib,
    /// OmniRig COM server (Windows only).
    OmniRig,
}

/// Any TOML value: a known engine name in any letter case gives that engine; another string or
/// a value that is not a string gives `None`, so a file of another build loads.
fn lenient_engine<'de, D: Deserializer<'de>>(d: D) -> Result<CatEngine, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        S(String),
        Other(serde::de::IgnoredAny),
    }
    let Raw::S(s) = Raw::deserialize(d)? else {
        return Ok(CatEngine::None);
    };
    Ok(match s.to_ascii_lowercase().as_str() {
        "rigctld" => CatEngine::Rigctld,
        "hamlib" => CatEngine::Hamlib,
        "omnirig" => CatEngine::OmniRig,
        _ => CatEngine::None,
    })
}

/// Serial parity (`PARITY`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Parity {
    #[default]
    None,
    Odd,
    Even,
    Mark,
    Space,
}

/// Serial stop bits (`STOPBITS`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StopBits {
    #[default]
    One,
    One5,
    Two,
}

/// RTS/DTR line state (`FLOWCONTROL`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowControl {
    #[default]
    Low,
    High,
    Handshake,
}
