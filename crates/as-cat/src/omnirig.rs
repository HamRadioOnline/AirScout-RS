//! OmniRig rig semantics (AirScout.CAT `OmniRigX.cs`) over a thin `OmniDispatch` seam.
//!
//! The COM backend (Windows only) implements `OmniDispatch`; everything here is
//! plain logic and runs, and is tested, on every OS.

use crate::rig::Rig;
use crate::{Doppler, RigIdent, RigMode, RigRit, RigSplit, RigStatus};

// OMNIRIGSTATUS (OmniRigX.cs:15-22)
pub const ST_NOTCONFIGURED: i32 = 0x0000_0000;
pub const ST_DISABLED: i32 = 0x0000_0001;
pub const ST_PORTBUSY: i32 = 0x0000_0002;
pub const ST_NOTRESPONDING: i32 = 0x0000_0003;
pub const ST_ONLINE: i32 = 0x0000_0004;

// OMNIRIGPARAM (OmniRigX.cs:24-58)
pub const PM_UNKNOWN: i32 = 0x0000_0001;
pub const PM_FREQ: i32 = 0x0000_0002;
pub const PM_FREQA: i32 = 0x0000_0004;
pub const PM_FREQB: i32 = 0x0000_0008;
pub const PM_PITCH: i32 = 0x0000_0010;
pub const PM_RITOFFSET: i32 = 0x0000_0020;
pub const PM_RIT0: i32 = 0x0000_0040;
pub const PM_VFOAA: i32 = 0x0000_0080;
pub const PM_VFOAB: i32 = 0x0000_0100;
pub const PM_VFOBA: i32 = 0x0000_0200;
pub const PM_VFOBB: i32 = 0x0000_0400;
pub const PM_VFOA: i32 = 0x0000_0800;
pub const PM_VFOB: i32 = 0x0000_1000;
pub const PM_VFOEQUAL: i32 = 0x0000_2000;
pub const PM_VFOSWAP: i32 = 0x0000_4000;
pub const PM_SPLITON: i32 = 0x0000_8000;
pub const PM_SPLITOFF: i32 = 0x0001_0000;
pub const PM_RITON: i32 = 0x0002_0000;
pub const PM_RITOFF: i32 = 0x0004_0000;
pub const PM_XITON: i32 = 0x0008_0000;
pub const PM_XITOFF: i32 = 0x0010_0000;
pub const PM_RX: i32 = 0x0020_0000;
pub const PM_TX: i32 = 0x0040_0000;
pub const PM_CW_U: i32 = 0x0080_0000;
pub const PM_CW_L: i32 = 0x0100_0000;
pub const PM_SSB_U: i32 = 0x0200_0000;
pub const PM_SSB_L: i32 = 0x0400_0000;
pub const PM_DIG_U: i32 = 0x0800_0000;
pub const PM_DIG_L: i32 = 0x1000_0000;
pub const PM_AM: i32 = 0x2000_0000;
pub const PM_FM: i32 = 0x4000_0000;

/// Name of an OMNIRIGPARAM value, `None` when it is not one.
pub fn param_name(v: i32) -> Option<&'static str> {
    Some(match v {
        PM_UNKNOWN => "PM_UNKNOWN",
        PM_FREQ => "PM_FREQ",
        PM_FREQA => "PM_FREQA",
        PM_FREQB => "PM_FREQB",
        PM_PITCH => "PM_PITCH",
        PM_RITOFFSET => "PM_RITOFFSET",
        PM_RIT0 => "PM_RIT0",
        PM_VFOAA => "PM_VFOAA",
        PM_VFOAB => "PM_VFOAB",
        PM_VFOBA => "PM_VFOBA",
        PM_VFOBB => "PM_VFOBB",
        PM_VFOA => "PM_VFOA",
        PM_VFOB => "PM_VFOB",
        PM_VFOEQUAL => "PM_VFOEQUAL",
        PM_VFOSWAP => "PM_VFOSWAP",
        PM_SPLITON => "PM_SPLITON",
        PM_SPLITOFF => "PM_SPLITOFF",
        PM_RITON => "PM_RITON",
        PM_RITOFF => "PM_RITOFF",
        PM_XITON => "PM_XITON",
        PM_XITOFF => "PM_XITOFF",
        PM_RX => "PM_RX",
        PM_TX => "PM_TX",
        PM_CW_U => "PM_CW_U",
        PM_CW_L => "PM_CW_L",
        PM_SSB_U => "PM_SSB_U",
        PM_SSB_L => "PM_SSB_L",
        PM_DIG_U => "PM_DIG_U",
        PM_DIG_L => "PM_DIG_L",
        PM_AM => "PM_AM",
        PM_FM => "PM_FM",
        _ => return None,
    })
}

/// The properties of an OmniRig rig object that AirScout reads or writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OmniProp {
    Status,
    Mode,
    Freq,
    FreqA,
    FreqB,
    Vfo,
    Split,
    Rit,
}

impl OmniProp {
    /// The COM property name.
    pub fn name(self) -> &'static str {
        match self {
            OmniProp::Status => "Status",
            OmniProp::Mode => "Mode",
            OmniProp::Freq => "Freq",
            OmniProp::FreqA => "FreqA",
            OmniProp::FreqB => "FreqB",
            OmniProp::Vfo => "Vfo",
            OmniProp::Split => "Split",
            OmniProp::Rit => "Rit",
        }
    }
}

/// A failed OmniRig call (a COM error in the real backend).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OmniError(pub String);

/// The OmniRig engine and its rig objects, one call per COM access.
pub trait OmniDispatch: Send {
    fn interface_version(&mut self) -> Result<i32, OmniError>;
    fn has_rig(&mut self, n: u8) -> bool;
    fn get(&mut self, n: u8, p: OmniProp) -> Result<i32, OmniError>;
    fn set(&mut self, n: u8, p: OmniProp, v: i32) -> Result<(), OmniError>;
    fn is_readable(&mut self, n: u8, param: i32) -> Result<bool, OmniError>;
    fn is_writeable(&mut self, n: u8, param: i32) -> Result<bool, OmniError>;
    /// Calls a method by name. Exists only so the C# call of `IsWritable` (OmniRig has
    /// `IsParamWriteable`, no `IsWritable`) fails the way the late binding does.
    fn call_missing(&mut self, n: u8, name: &str, param: i32) -> Result<bool, OmniError>;
}

/// Creates the engine (COM `CreateInstance` in the real backend).
pub type OmniOpen = Box<dyn FnMut() -> Result<Box<dyn OmniDispatch>, String> + Send>;

/// PM_* value written for a mode (L17: the correct value, not the RIGMODE number).
fn mode_param(m: RigMode) -> Option<i32> {
    match m {
        RigMode::Cw => Some(PM_CW_L),
        RigMode::CwR => Some(PM_CW_U),
        RigMode::Lsb => Some(PM_SSB_L),
        RigMode::Usb => Some(PM_SSB_U),
        RigMode::Dig => Some(PM_DIG_L),
        RigMode::DigR => Some(PM_DIG_U),
        RigMode::Am => Some(PM_AM),
        RigMode::Fm => Some(PM_FM),
        RigMode::None | RigMode::Other => None,
    }
}

fn mode_from_param(v: i32) -> RigMode {
    match v {
        PM_CW_L => RigMode::Cw,
        PM_CW_U => RigMode::CwR,
        PM_SSB_L => RigMode::Lsb,
        PM_SSB_U => RigMode::Usb,
        PM_DIG_L => RigMode::Dig,
        PM_DIG_U => RigMode::DigR,
        PM_AM => RigMode::Am,
        PM_FM => RigMode::Fm,
        _ => RigMode::None,
    }
}

/// OmniRig takes 32-bit frequencies (OmniRigX.cs:873); the port also never writes <= 0.
fn freq_ok(hz: i64) -> bool {
    hz > 0 && hz <= i64::from(i32::MAX)
}

/// One rig object of the engine, with the C# `dynamic rig` calls.
struct RigRef<'a> {
    d: &'a mut dyn OmniDispatch,
    n: u8,
}

impl RigRef<'_> {
    fn w(&mut self, param: i32) -> Result<bool, OmniError> {
        self.d.is_writeable(self.n, param)
    }

    fn set(&mut self, p: OmniProp, v: i32) -> Result<(), OmniError> {
        self.d.set(self.n, p, v)
    }

    fn set_freq(&mut self, p: OmniProp, hz: i64) -> Result<(), OmniError> {
        // `freq_ok` was checked by the caller, so the cast is lossless.
        self.set(p, hz as i32)
    }

    /// `CanWriteMode` (:1090-1111).
    fn can_write_mode(&mut self) -> Result<bool, OmniError> {
        for p in [
            PM_CW_L, PM_CW_U, PM_SSB_L, PM_SSB_U, PM_DIG_L, PM_DIG_U, PM_AM, PM_FM,
        ] {
            if !self.w(p)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `if (CanWriteMode(rig) && mode != NONE) rig.Mode = mode;`
    fn write_mode(&mut self, mode: Option<i32>) -> Result<(), OmniError> {
        if self.can_write_mode()? {
            if let Some(v) = mode {
                self.set(OmniProp::Mode, v)?;
            }
        }
        Ok(())
    }

    /// `IntSetSimplexMode` (:979-1043).
    fn simplex(&mut self, f: i64, m: RigMode) -> Result<(), OmniError> {
        use OmniProp::{Freq, FreqA, FreqB, Split, Vfo};
        if self.w(PM_FREQA)? && self.w(PM_VFOAA)? {
            self.set(Vfo, PM_VFOAA)?;
            self.set_freq(FreqA, f)?;
        } else if self.w(PM_FREQA)? && self.w(PM_VFOA)? && self.w(PM_SPLITOFF)? {
            self.set(Vfo, PM_VFOA)?;
            self.set_freq(FreqA, f)?;
        } else if self.w(PM_FREQ)? && self.w(PM_VFOA)? && self.w(PM_VFOB)? {
            self.set(Vfo, PM_VFOB)?;
            self.set_freq(Freq, f)?;
            self.set(Vfo, PM_VFOA)?;
            self.set_freq(Freq, f)?;
        } else if self.w(PM_FREQ)? && self.w(PM_VFOEQUAL)? {
            self.set_freq(Freq, f)?;
            self.set(Vfo, PM_VFOEQUAL)?;
        } else if self.w(PM_FREQ)? && self.w(PM_VFOSWAP)? {
            self.set(Vfo, PM_VFOSWAP)?;
            self.set_freq(Freq, f)?;
            self.set(Vfo, PM_VFOSWAP)?;
            self.set_freq(Freq, f)?;
        } else if self.w(PM_FREQ)? && !self.w(PM_VFOA)? && self.w(PM_FREQB)? {
            self.set_freq(Freq, f)?;
            self.set_freq(FreqB, f)?;
        } else if self.w(PM_FREQ)? {
            self.set_freq(Freq, f)?;
        }
        if self.w(PM_SPLITOFF)? {
            self.set(Split, PM_SPLITOFF)?;
        }
        self.write_mode(mode_param(m))
    }

    /// `IntSetSplitMode` (:1113-1221).
    fn split_mode(&mut self, rx: i64, tx: i64, m: RigMode) -> Result<(), OmniError> {
        use OmniProp::{Freq, FreqA, FreqB, Split, Vfo};
        let mv = mode_param(m);
        if self.w(PM_VFOEQUAL)? && self.can_write_mode()? {
            if let Some(v) = mv {
                self.set(OmniProp::Mode, v)?;
                self.set(Vfo, PM_VFOEQUAL)?;
            }
        }
        if self.w(PM_FREQA)? && self.w(PM_FREQB)? && self.w(PM_VFOAB)? {
            // TS-570
            self.set(Vfo, PM_VFOAB)?;
            self.set_freq(FreqA, rx)?;
            self.set_freq(FreqB, tx)?;
            self.write_mode(mv)?;
        } else if self.w(PM_FREQ)? && self.w(PM_VFOEQUAL)? {
            // IC-746
            self.set_freq(Freq, tx)?;
            self.write_mode(mv)?;
            self.set(Vfo, PM_VFOEQUAL)?;
            self.set_freq(Freq, rx)?;
        } else if self.w(PM_VFOB)? && self.w(PM_FREQ)? && self.w(PM_VFOA)? {
            // FT-100D
            self.set(Vfo, PM_VFOB)?;
            self.set_freq(Freq, tx)?;
            self.write_mode(mv)?;
            self.set(Vfo, PM_VFOA)?;
            self.set_freq(Freq, rx)?;
            self.write_mode(mv)?;
        } else if self.w(PM_FREQ)? && self.w(PM_VFOSWAP)? {
            // FT-817
            self.set(Vfo, PM_VFOSWAP)?;
            self.set_freq(Freq, tx)?;
            self.write_mode(mv)?;
            self.set(Vfo, PM_VFOSWAP)?;
            self.set_freq(Freq, rx)?;
            self.write_mode(mv)?;
        } else if self.w(PM_FREQA)? && self.w(PM_FREQB)? && self.w(PM_VFOA)? {
            // FT-1000MP, IC-7610
            self.set(Vfo, PM_VFOA)?;
            self.set_freq(FreqA, rx)?;
            self.set_freq(FreqB, tx)?;
            self.write_mode(mv)?;
        } else if self.w(PM_FREQ)? && !self.w(PM_FREQA)? && self.w(PM_FREQB)? {
            self.set_freq(Freq, rx)?;
            self.set_freq(FreqB, tx)?;
        }
        if self.w(PM_SPLITON)? {
            self.set(Split, PM_SPLITON)?;
        }
        Ok(())
    }

    /// `IntGetRxFrequency` / `IntGetTxFrequency` (:518-595).
    fn freq(&mut self, tx: bool) -> Result<i64, OmniError> {
        use OmniProp::{Freq, FreqA, FreqB, Split};
        let n = self.n;
        if self.d.get(n, Split)? == PM_SPLITOFF {
            for p in [Freq, FreqA] {
                let v = self.d.get(n, p)?;
                if v > 0 {
                    return Ok(i64::from(v));
                }
            }
        } else {
            let v = self.d.get(n, if tx { FreqB } else { FreqA })?;
            if v > 0 {
                return Ok(i64::from(v));
            }
        }
        Ok(0)
    }

    /// `IntSetRxFrequency` / `IntSetTxFrequency` (:837-936). OmniRig has no `IsWritable`
    /// (the C# means `IsParamWriteable`), so the call errors and nothing is written.
    fn set_single(&mut self, hz: i64, tx: bool) -> Result<bool, OmniError> {
        use OmniProp::{Freq, FreqA, FreqB, Split};
        let n = self.n;
        let split_off = self.d.get(n, Split)? == PM_SPLITOFF;
        let targets: &[(i32, OmniProp)] = match (split_off, tx) {
            (true, _) => &[(PM_FREQ, Freq), (PM_FREQA, FreqA)],
            (false, false) => &[(PM_FREQA, FreqA)],
            (false, true) => &[(PM_FREQB, FreqB)],
        };
        for &(param, prop) in targets {
            if self.d.call_missing(n, "IsWritable", param)? {
                self.set_freq(prop, hz)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

const V1_TEXT: &str = "OmniRig V1.19 supports Rig 1 and Rig 2 only.";
const V2_TEXT: &str = "OmniRig V2.x supports Rig 1 to Rig 4 only.";

/// A rig behind OmniRig (`OmniRigX`).
pub struct OmniRig {
    open: OmniOpen,
    rig: u8,
    engine: Option<Box<dyn OmniDispatch>>,
    rigs: u8,
    error: Option<String>,
}

impl OmniRig {
    pub fn new(open: OmniOpen, rig: u8) -> Self {
        Self {
            open,
            rig,
            engine: None,
            rigs: 2,
            error: None,
        }
    }

    /// `StartOmniRig` (:87-152); an error text is kept for `take_error` and the
    /// open is retried on the next call.
    fn ensure_open(&mut self) -> bool {
        if self.engine.is_some() {
            return true;
        }
        let mut engine = match (self.open)() {
            Ok(e) => e,
            Err(e) => {
                self.error = Some(format!("Error while initializing OmniRig: {e}"));
                return false;
            }
        };
        if let Err(e) = engine.interface_version() {
            self.error = Some(format!("Error while initializing OmniRig: {}", e.0));
            return false;
        }
        self.rigs = if engine.has_rig(3) && engine.has_rig(4) {
            4
        } else {
            2
        };
        self.engine = Some(engine);
        true
    }

    /// Runs a call on the selected rig. `None` while the engine is closed, or when the
    /// rig number is not served (the C# falls through to the default result).
    fn with_rig<T>(
        &mut self,
        f: impl FnOnce(&mut RigRef<'_>) -> Result<T, OmniError>,
    ) -> Option<Result<T, OmniError>> {
        if self.rig == 0 || self.rig > self.rigs {
            return None;
        }
        let n = self.rig;
        let d = self.engine.as_mut()?;
        Some(f(&mut RigRef { d: d.as_mut(), n }))
    }

    fn get_param(&mut self, p: OmniProp) -> Option<i32> {
        self.with_rig(|r| r.d.get(r.n, p)).and_then(Result::ok)
    }

    fn put(&mut self, p: OmniProp, v: Option<i32>) -> bool {
        match v {
            Some(v) => matches!(self.with_rig(|r| r.set(p, v)), Some(Ok(()))),
            None => false,
        }
    }
}

impl Rig for OmniRig {
    fn ident(&self) -> RigIdent {
        if self.engine.is_none() {
            return RigIdent {
                cat_version: "OmniRig".into(),
                rig_type: "OmniRig".into(),
            };
        }
        let cat_version = if self.rigs == 4 {
            "OmniRig V2.x"
        } else {
            "OmniRig V1.19"
        };
        RigIdent {
            cat_version: cat_version.into(),
            rig_type: format!("[OmniRig] Rig {}", self.rig),
        }
    }

    /// `GetStatus` / `IntGetRigStatus` (:243-312).
    fn get_status(&mut self, s: Doppler) -> RigStatus {
        if !self.ensure_open() {
            return RigStatus::NoCat;
        }
        if self.rig == 0 || self.rig > self.rigs {
            let text = if self.rigs == 4 { V2_TEXT } else { V1_TEXT };
            self.error = Some(text.into());
            return RigStatus::NoRig;
        }
        let r = self.with_rig(|r| {
            match r.d.get(r.n, OmniProp::Status)? {
                ST_PORTBUSY => return Ok(RigStatus::NoPort),
                ST_NOTCONFIGURED | ST_DISABLED => return Ok(RigStatus::NoRig),
                ST_NOTRESPONDING => return Ok(RigStatus::Offline),
                _ => {}
            }
            if s == Doppler::A
                && !(r.d.is_readable(r.n, PM_SPLITOFF)?
                    && r.d.is_readable(r.n, PM_SPLITON)?
                    && r.w(PM_SPLITOFF)?
                    && r.w(PM_SPLITON)?
                    && r.w(PM_VFOEQUAL)?)
            {
                return Ok(RigStatus::NotSuitable);
            }
            Ok(RigStatus::Online)
        });
        match r {
            Some(Ok(st)) => st,
            _ => RigStatus::None,
        }
    }

    fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    fn get_mode(&mut self) -> RigMode {
        self.get_param(OmniProp::Mode)
            .map_or(RigMode::None, mode_from_param)
    }

    fn get_split(&mut self) -> RigSplit {
        match self.get_param(OmniProp::Split) {
            Some(PM_SPLITOFF) => RigSplit::SplitOff,
            Some(PM_SPLITON) => RigSplit::SplitOn,
            _ => RigSplit::None,
        }
    }

    fn get_rit(&mut self) -> RigRit {
        match self.get_param(OmniProp::Rit) {
            Some(PM_RITOFF) => RigRit::RitOff,
            Some(PM_RITON) => RigRit::RitOn,
            _ => RigRit::None,
        }
    }

    fn get_rx_frequency(&mut self) -> i64 {
        self.with_rig(|r| r.freq(false))
            .and_then(Result::ok)
            .unwrap_or(0)
    }

    fn get_tx_frequency(&mut self) -> i64 {
        self.with_rig(|r| r.freq(true))
            .and_then(Result::ok)
            .unwrap_or(0)
    }

    fn set_mode(&mut self, m: RigMode) -> bool {
        self.put(OmniProp::Mode, mode_param(m))
    }

    fn set_split(&mut self, s: RigSplit) -> bool {
        let v = match s {
            RigSplit::SplitOff => Some(PM_SPLITOFF),
            RigSplit::SplitOn => Some(PM_SPLITON),
            RigSplit::None => None,
        };
        self.put(OmniProp::Split, v)
    }

    fn set_rit(&mut self, r: RigRit) -> bool {
        let v = match r {
            RigRit::RitOff => Some(PM_RITOFF),
            RigRit::RitOn => Some(PM_RITON),
            RigRit::None => None,
        };
        self.put(OmniProp::Rit, v)
    }

    fn set_rx_frequency(&mut self, hz: i64) -> bool {
        freq_ok(hz) && matches!(self.with_rig(|r| r.set_single(hz, false)), Some(Ok(true)))
    }

    fn set_tx_frequency(&mut self, hz: i64) -> bool {
        freq_ok(hz) && matches!(self.with_rig(|r| r.set_single(hz, true)), Some(Ok(true)))
    }

    fn set_simplex_mode(&mut self, hz: i64, mode: RigMode) -> bool {
        freq_ok(hz) && matches!(self.with_rig(|r| r.simplex(hz, mode)), Some(Ok(())))
    }

    fn set_split_mode(&mut self, rx: i64, tx: i64, mode: RigMode) -> bool {
        freq_ok(rx)
            && freq_ok(tx)
            && matches!(self.with_rig(|r| r.split_mode(rx, tx, mode)), Some(Ok(())))
    }
}
