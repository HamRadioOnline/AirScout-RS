//! CAT rig control: rig types, the `Backend` seam and the C# rig behaviour.
#![cfg_attr(not(any(feature = "hamlib", windows)), forbid(unsafe_code))]
#![cfg_attr(any(feature = "hamlib", windows), deny(unsafe_code))]

pub mod backend;
pub mod config;
#[cfg(feature = "settings")]
pub mod from_settings;
#[cfg(all(feature = "hamlib", any(target_os = "linux", target_os = "macos")))]
#[allow(unsafe_code)]
mod hamlib;
pub mod hamlib_conf;
pub mod modes;
pub mod omnirig;
#[cfg(windows)]
#[allow(unsafe_code)]
mod omnirig_com;
pub mod rig;
pub mod rigctld;
pub mod rigctld_proto;
pub mod worker;

pub use backend::{Backend, BackendError};
pub use config::{
    factory, hamlib_available, make_rig, omnirig_available, CatConfig, Engine, FlowControl, Parity,
    SerialConfig, StopBits,
};
#[cfg(all(feature = "hamlib", any(target_os = "linux", target_os = "macos")))]
pub use hamlib::NativeHamlib;
pub use modes::{mode_from_hamlib, mode_to_hamlib};
pub use omnirig::{OmniDispatch, OmniError, OmniOpen, OmniProp, OmniRig};
#[cfg(windows)]
pub use omnirig_com::opener as omnirig_opener;
pub use rig::{CatRig, DopplerSession, Rig, Saved};
pub use rigctld::{rigctld_rig, Rigctld, RigctldConfig};
pub use worker::{
    run, CatEvent, CatWorker, DopplerCell, DopplerTarget, RigFactory, RigInfo, NO_RIG_WAIT,
};

/// Highest frequency [Hz] AirScout accepts from a rig or the Options dial field (100 GHz).
pub const MAX_FREQ_HZ: i64 = 100_000_000_000;

/// Rig connection state (AirScout.CAT `RigStatus`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RigStatus {
    #[default]
    None,
    NoCat,
    NoPort,
    NoRig,
    NotSuitable,
    Offline,
    Online,
    Error,
}

impl RigStatus {
    pub fn name(self) -> &'static str {
        match self {
            RigStatus::None => "NONE",
            RigStatus::NoCat => "NOCAT",
            RigStatus::NoPort => "NOPORT",
            RigStatus::NoRig => "NORIG",
            RigStatus::NotSuitable => "NOTSUITABLE",
            RigStatus::Offline => "OFFLINE",
            RigStatus::Online => "ONLINE",
            RigStatus::Error => "ERROR",
        }
    }

    pub fn value(self) -> u8 {
        match self {
            RigStatus::None => 0,
            RigStatus::NoCat => 1,
            RigStatus::NoPort => 2,
            RigStatus::NoRig => 4,
            RigStatus::NotSuitable => 8,
            RigStatus::Offline => 16,
            RigStatus::Online => 64,
            RigStatus::Error => 128,
        }
    }
}

/// Rig operating mode (AirScout.CAT `RigMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RigMode {
    #[default]
    None,
    Cw,
    CwR,
    Lsb,
    Usb,
    Dig,
    DigR,
    Am,
    Fm,
    Other,
}

impl RigMode {
    pub fn name(self) -> &'static str {
        match self {
            RigMode::None => "NONE",
            RigMode::Cw => "CW",
            RigMode::CwR => "CW_R",
            RigMode::Lsb => "LSB",
            RigMode::Usb => "USB",
            RigMode::Dig => "DIG",
            RigMode::DigR => "DIG_R",
            RigMode::Am => "AM",
            RigMode::Fm => "FM",
            RigMode::Other => "OTHER",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RigSplit {
    #[default]
    None,
    SplitOff,
    SplitOn,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RigRit {
    #[default]
    None,
    RitOn,
    RitOff,
}

/// Doppler strategy (which frequency the Doppler shift is applied to).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Doppler {
    #[default]
    None,
    A,
    B,
    C,
    D,
}

impl Doppler {
    pub fn value(self) -> u8 {
        match self {
            Doppler::None => 0,
            Doppler::A => 1,
            Doppler::B => 2,
            Doppler::C => 4,
            Doppler::D => 8,
        }
    }

    pub fn is_active(self) -> bool {
        self != Doppler::None
    }
}

/// Identification of a rig engine (shown in the UI).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RigIdent {
    pub cat_version: String,
    pub rig_type: String,
}
