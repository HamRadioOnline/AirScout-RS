//! The C# rig behaviour (ScoutBaseRig) once, for any backend.

use crate::backend::{Backend, BackendError};
use crate::modes::{mode_from_hamlib, mode_to_hamlib};
use crate::{Doppler, RigIdent, RigMode, RigRit, RigSplit, RigStatus};

/// The `IRig` subset used by the CAT worker and Doppler enter/leave.
pub trait Rig: Send {
    fn ident(&self) -> RigIdent;
    fn get_status(&mut self, strategy: Doppler) -> RigStatus;
    /// Text of the last NoCat/NoPort/Lost error, once.
    fn take_error(&mut self) -> Option<String>;
    fn get_mode(&mut self) -> RigMode;
    fn get_split(&mut self) -> RigSplit;
    fn get_rit(&mut self) -> RigRit;
    fn get_rx_frequency(&mut self) -> i64;
    fn get_tx_frequency(&mut self) -> i64;
    fn set_mode(&mut self, m: RigMode) -> bool;
    fn set_split(&mut self, s: RigSplit) -> bool;
    fn set_rit(&mut self, r: RigRit) -> bool;
    fn set_rx_frequency(&mut self, hz: i64) -> bool;
    fn set_tx_frequency(&mut self, hz: i64) -> bool;
    fn set_simplex_mode(&mut self, hz: i64, mode: RigMode) -> bool;
    fn set_split_mode(&mut self, rx: i64, tx: i64, mode: RigMode) -> bool;
}

/// A rig on top of any `Backend`.
pub struct CatRig<B: Backend> {
    backend: B,
    error: Option<String>,
}

impl<B: Backend> CatRig<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            error: None,
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }
}

impl<B: Backend> Rig for CatRig<B> {
    fn ident(&self) -> RigIdent {
        self.backend.ident()
    }

    fn get_status(&mut self, s: Doppler) -> RigStatus {
        match self.backend.connect() {
            Ok(()) => {}
            Err(BackendError::NoCat(t)) => {
                self.error = Some(t);
                return RigStatus::NoCat;
            }
            Err(BackendError::NoPort(t)) => {
                self.error = Some(t);
                return RigStatus::NoPort;
            }
            Err(_) => return RigStatus::None,
        }
        match self.backend.get_freq() {
            Ok(_) => {
                if s.is_active() {
                    match self.backend.get_split() {
                        Ok(_) => {}
                        Err(BackendError::Lost(t)) => {
                            self.error = Some(t);
                            return RigStatus::None;
                        }
                        Err(_) => return RigStatus::NotSuitable,
                    }
                }
                RigStatus::Online
            }
            Err(BackendError::Lost(t)) => {
                self.error = Some(t);
                RigStatus::None
            }
            Err(_) => RigStatus::Offline,
        }
    }

    fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    fn get_mode(&mut self) -> RigMode {
        self.backend
            .get_mode()
            .map_or(RigMode::None, |m| mode_from_hamlib(&m))
    }

    fn get_split(&mut self) -> RigSplit {
        match self.backend.get_split() {
            Ok(true) => RigSplit::SplitOn,
            Ok(false) => RigSplit::SplitOff,
            Err(_) => RigSplit::None,
        }
    }

    fn get_rit(&mut self) -> RigRit {
        match self.backend.get_rit_on() {
            Ok(true) => RigRit::RitOn,
            Ok(false) => RigRit::RitOff,
            Err(_) => RigRit::None,
        }
    }

    fn get_rx_frequency(&mut self) -> i64 {
        self.backend.get_freq().unwrap_or(0)
    }

    fn get_tx_frequency(&mut self) -> i64 {
        let r = if self.backend.get_split() == Ok(true) {
            self.backend.get_split_freq()
        } else {
            self.backend.get_freq()
        };
        r.unwrap_or(0)
    }

    fn set_mode(&mut self, m: RigMode) -> bool {
        match mode_to_hamlib(m) {
            Some(n) => self.backend.set_mode(n).is_ok(),
            None => false,
        }
    }

    fn set_split(&mut self, s: RigSplit) -> bool {
        match s {
            RigSplit::None => false,
            RigSplit::SplitOn => self.backend.set_split(true).is_ok(),
            RigSplit::SplitOff => self.backend.set_split(false).is_ok(),
        }
    }

    fn set_rit(&mut self, r: RigRit) -> bool {
        match r {
            RigRit::None => false,
            RigRit::RitOn => self.backend.set_rit_on(true).is_ok(),
            RigRit::RitOff => self.backend.set_rit_on(false).is_ok(),
        }
    }

    fn set_rx_frequency(&mut self, hz: i64) -> bool {
        hz > 0 && self.backend.set_freq(hz).is_ok()
    }

    fn set_tx_frequency(&mut self, hz: i64) -> bool {
        if hz <= 0 {
            return false;
        }
        if self.backend.get_split() == Ok(true) {
            self.backend.set_split_freq(hz).is_ok()
        } else {
            self.backend.set_freq(hz).is_ok()
        }
    }

    fn set_simplex_mode(&mut self, hz: i64, m: RigMode) -> bool {
        if hz <= 0 {
            return false;
        }
        let split = self.backend.set_split(false).is_ok();
        let freq = self.backend.set_freq(hz).is_ok();
        if let Some(n) = mode_to_hamlib(m) {
            let _ = self.backend.set_mode(n);
        }
        split && freq
    }

    fn set_split_mode(&mut self, rx: i64, tx: i64, m: RigMode) -> bool {
        if rx <= 0 || tx <= 0 {
            return false;
        }
        let split = self.backend.set_split(true).is_ok();
        let rx_ok = self.backend.set_freq(rx).is_ok();
        let tx_ok = self.backend.set_split_freq(tx).is_ok();
        if let Some(n) = mode_to_hamlib(m) {
            let _ = self.backend.set_mode(n);
            let _ = self.backend.set_split_mode(n);
        }
        split && rx_ok && tx_ok
    }
}

/// Rig state saved when Doppler starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Saved {
    pub mode: RigMode,
    pub split: RigSplit,
    pub rit: RigRit,
    pub rx_hz: i64,
    pub tx_hz: i64,
}

/// `EnterDoppler` / `LeaveDoppler` of ScoutBaseRig.
#[derive(Default)]
pub struct DopplerSession {
    saved: Option<Saved>,
}

impl DopplerSession {
    pub fn saved(&self) -> Option<Saved> {
        self.saved
    }

    /// Save the rig state (once) and switch to split with RX = TX. A save without an RX
    /// frequency (a read failed) is not stored and gives false, so the enter is retried.
    pub fn enter(&mut self, rig: &mut dyn Rig, s: Doppler) -> bool {
        if rig.get_status(s) != RigStatus::Online {
            return false;
        }
        let saved = match self.saved {
            Some(v) => v,
            None => {
                let mode = rig.get_mode();
                let split = rig.get_split();
                let rit = rig.get_rit();
                let rx_hz = rig.get_rx_frequency();
                let tx_hz = rig.get_tx_frequency();
                if rx_hz <= 0 {
                    return false;
                }
                let v = Saved {
                    mode,
                    split,
                    rit,
                    rx_hz,
                    tx_hz,
                };
                self.saved = Some(v);
                v
            }
        };
        rig.set_split_mode(saved.rx_hz, saved.rx_hz, saved.mode);
        true
    }

    /// Restore the saved state. Sends nothing without a saved state (the C#
    /// code writes 0 Hz there; the port never writes a frequency <= 0).
    pub fn leave(&mut self, rig: &mut dyn Rig) {
        let Some(v) = self.saved.take() else {
            return;
        };
        if v.mode != RigMode::None {
            rig.set_mode(v.mode);
        }
        if v.split != RigSplit::None {
            rig.set_split(v.split);
        }
        if v.rit != RigRit::None {
            rig.set_rit(v.rit);
        }
        if v.rx_hz > 0 {
            rig.set_rx_frequency(v.rx_hz);
        }
        if v.tx_hz > 0 {
            rig.set_tx_frequency(v.tx_hz);
        }
        if v.split == RigSplit::SplitOn && v.tx_hz > 0 {
            rig.set_split_mode(v.rx_hz, v.tx_hz, v.mode);
        } else {
            rig.set_simplex_mode(v.rx_hz, v.mode);
        }
    }
}
