//! Scripted OmniRig engine. Clones share the log, so a test keeps a handle.
#![allow(dead_code)]

use as_cat::omnirig::{param_name, PM_RITOFF, PM_SPLITOFF, PM_SSB_U};
use as_cat::{OmniDispatch, OmniError, OmniProp};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct FakeOmni {
    pub version: i32,
    pub rigs: u8,
    pub status: i32,
    pub props: BTreeMap<OmniProp, i32>,
    pub readable: BTreeSet<i32>,
    pub writeable: BTreeSet<i32>,
    /// Property names (`"Status"`, `"FreqA"` ...) and `"interface_version"`,
    /// `"is_readable"`, `"is_writeable"` whose calls fail.
    pub fail: BTreeSet<&'static str>,
    pub log: Arc<Mutex<Vec<String>>>,
}

impl FakeOmni {
    pub fn new(rigs: u8) -> Self {
        let props = BTreeMap::from([
            (OmniProp::Freq, 144_300_000),
            (OmniProp::Mode, PM_SSB_U),
            (OmniProp::Split, PM_SPLITOFF),
            (OmniProp::Rit, PM_RITOFF),
        ]);
        Self {
            version: if rigs == 4 { 0x210 } else { 0x110 },
            rigs,
            status: 4,
            props,
            readable: BTreeSet::new(),
            writeable: BTreeSet::new(),
            fail: BTreeSet::new(),
            log: Arc::default(),
        }
    }

    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    pub fn clear_log(&self) {
        self.log.lock().unwrap().clear();
    }

    fn check(&self, what: &'static str) -> Result<(), OmniError> {
        if self.fail.contains(what) {
            Err(OmniError(format!("{what} failed")))
        } else {
            Ok(())
        }
    }
}

impl OmniDispatch for FakeOmni {
    fn interface_version(&mut self) -> Result<i32, OmniError> {
        self.check("interface_version")?;
        Ok(self.version)
    }

    fn has_rig(&mut self, n: u8) -> bool {
        n >= 1 && n <= self.rigs
    }

    fn get(&mut self, _n: u8, p: OmniProp) -> Result<i32, OmniError> {
        self.check(p.name())?;
        if p == OmniProp::Status {
            return Ok(self.status);
        }
        self.props
            .get(&p)
            .copied()
            .ok_or_else(|| OmniError(format!("{} unset", p.name())))
    }

    fn set(&mut self, _n: u8, p: OmniProp, v: i32) -> Result<(), OmniError> {
        self.check(p.name())?;
        let shown = match p {
            OmniProp::Freq | OmniProp::FreqA | OmniProp::FreqB => v.to_string(),
            _ => param_name(v).map_or_else(|| v.to_string(), str::to_string),
        };
        self.log
            .lock()
            .unwrap()
            .push(format!("{}={shown}", p.name()));
        self.props.insert(p, v);
        Ok(())
    }

    fn is_readable(&mut self, _n: u8, param: i32) -> Result<bool, OmniError> {
        self.check("is_readable")?;
        Ok(self.readable.contains(&param))
    }

    fn is_writeable(&mut self, _n: u8, param: i32) -> Result<bool, OmniError> {
        self.check("is_writeable")?;
        Ok(self.writeable.contains(&param))
    }
}
