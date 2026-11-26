//! CAT engine configuration and rig construction (AirScout.CAT Enums.cs).

use std::time::Duration;

use crate::rigctld::{rigctld_rig, RigctldConfig};
use crate::worker::RigFactory;
use crate::Rig;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Parity {
    #[default]
    None,
    Odd,
    Even,
    Mark,
    Space,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StopBits {
    #[default]
    One,
    One5,
    Two,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlowControl {
    #[default]
    Low,
    High,
    Handshake,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerialConfig {
    pub port_name: String,
    pub baudrate: u32,
    pub data_bits: u8,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub rts: FlowControl,
    pub dtr: FlowControl,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Engine {
    None,
    Rigctld {
        host: String,
        port: u16,
    },
    Hamlib {
        model: u32,
        serial: SerialConfig,
    },
    /// OmniRig rig 1-4 (`OmniRigX`, Windows only).
    OmniRig {
        rig: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatConfig {
    pub engine: Engine,
    /// Pause between two polling passes (`CAT_Update`).
    pub update: Duration,
    /// Per-command timeout of the backend.
    pub timeout: Duration,
}

/// Whether the native hamlib backend is compiled in (Linux and macOS only).
pub fn hamlib_available() -> bool {
    cfg!(all(
        feature = "hamlib",
        any(target_os = "linux", target_os = "macos")
    ))
}

/// Whether the OmniRig COM backend is compiled in (Windows only).
pub fn omnirig_available() -> bool {
    cfg!(windows)
}

/// Build a rig for the configured engine, or `None` (reported as NoRig).
pub fn make_rig(c: &CatConfig) -> Option<Box<dyn Rig>> {
    match &c.engine {
        Engine::None => None,
        Engine::Rigctld { host, port } => Some(Box::new(rigctld_rig(RigctldConfig {
            host: host.clone(),
            port: *port,
            timeout: c.timeout,
        }))),
        #[cfg(all(feature = "hamlib", any(target_os = "linux", target_os = "macos")))]
        Engine::Hamlib { model, serial } => Some(Box::new(crate::CatRig::new(
            crate::hamlib::NativeHamlib::new(*model, serial.clone(), c.timeout),
        ))),
        #[cfg(not(all(feature = "hamlib", any(target_os = "linux", target_os = "macos"))))]
        Engine::Hamlib { .. } => None,
        #[cfg(windows)]
        Engine::OmniRig { rig } => Some(Box::new(crate::OmniRig::new(
            crate::omnirig_com::opener(),
            *rig,
        ))),
        #[cfg(not(windows))]
        Engine::OmniRig { .. } => None,
    }
}

/// A rig factory for the worker.
pub fn factory(c: CatConfig) -> RigFactory {
    Box::new(move || make_rig(&c))
}
