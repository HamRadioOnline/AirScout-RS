//! Scripted in-memory backend. Clones share state, so a test keeps a handle
//! while the rig owns another.
#![allow(dead_code)]

use as_cat::{Backend, BackendError, RigIdent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Connect,
    GetFreq,
    SetFreq(i64),
    GetMode,
    SetMode(String),
    GetSplit,
    SetSplit(bool),
    GetSplitFreq,
    SetSplitFreq(i64),
    SetSplitMode(String),
    GetRit,
    SetRit(bool),
}

pub struct State {
    pub freq: i64,
    pub split_freq: i64,
    pub mode: String,
    pub split: bool,
    pub rit: bool,
    pub log: Vec<Op>,
    /// Error returned by every call of the named operation ("GetFreq", ...).
    pub fail: HashMap<&'static str, BackendError>,
    /// Calls of the named operation that still succeed before `fail` applies.
    pub ok_first: HashMap<&'static str, usize>,
}

#[derive(Clone)]
pub struct FakeBackend(pub Arc<Mutex<State>>);

impl FakeBackend {
    pub fn new() -> Self {
        FakeBackend(Arc::new(Mutex::new(State {
            freq: 144_300_000,
            split_freq: 144_300_000,
            mode: "USB".into(),
            split: false,
            rit: false,
            log: Vec::new(),
            fail: HashMap::new(),
            ok_first: HashMap::new(),
        })))
    }

    pub fn ops(&self) -> Vec<Op> {
        self.0.lock().unwrap().log.clone()
    }

    pub fn clear_log(&self) {
        self.0.lock().unwrap().log.clear();
    }

    pub fn fail(&self, op: &'static str, e: BackendError) {
        self.0.lock().unwrap().fail.insert(op, e);
    }

    /// Let the first `n` calls of `op` succeed, then fail every later one.
    pub fn fail_after(&self, op: &'static str, n: usize, e: BackendError) {
        let mut s = self.0.lock().unwrap();
        s.ok_first.insert(op, n);
        s.fail.insert(op, e);
    }

    pub fn fail_all_gets(&self) {
        for op in ["GetFreq", "GetMode", "GetSplit", "GetSplitFreq", "GetRit"] {
            self.fail(op, BackendError::Rejected(-1));
        }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self.0.lock().unwrap())
    }

    fn run<T>(
        &self,
        name: &'static str,
        op: Op,
        f: impl FnOnce(&mut State) -> T,
    ) -> Result<T, BackendError> {
        let mut s = self.0.lock().unwrap();
        s.log.push(op);
        if let Some(n) = s.ok_first.get_mut(name).filter(|n| **n > 0) {
            *n -= 1;
        } else if let Some(e) = s.fail.get(name) {
            return Err(e.clone());
        }
        Ok(f(&mut s))
    }
}

impl Backend for FakeBackend {
    fn ident(&self) -> RigIdent {
        RigIdent {
            cat_version: "fake".into(),
            rig_type: "fake".into(),
        }
    }
    fn connect(&mut self) -> Result<(), BackendError> {
        self.run("Connect", Op::Connect, |_| ())
    }
    fn close(&mut self) {}
    fn get_freq(&mut self) -> Result<i64, BackendError> {
        self.run("GetFreq", Op::GetFreq, |s| s.freq)
    }
    fn set_freq(&mut self, hz: i64) -> Result<(), BackendError> {
        self.run("SetFreq", Op::SetFreq(hz), |s| s.freq = hz)
    }
    fn get_mode(&mut self) -> Result<String, BackendError> {
        self.run("GetMode", Op::GetMode, |s| s.mode.clone())
    }
    fn set_mode(&mut self, mode: &str) -> Result<(), BackendError> {
        self.run("SetMode", Op::SetMode(mode.into()), |s| {
            s.mode = mode.into()
        })
    }
    fn get_split(&mut self) -> Result<bool, BackendError> {
        self.run("GetSplit", Op::GetSplit, |s| s.split)
    }
    fn set_split(&mut self, on: bool) -> Result<(), BackendError> {
        self.run("SetSplit", Op::SetSplit(on), |s| s.split = on)
    }
    fn get_split_freq(&mut self) -> Result<i64, BackendError> {
        self.run("GetSplitFreq", Op::GetSplitFreq, |s| s.split_freq)
    }
    fn set_split_freq(&mut self, hz: i64) -> Result<(), BackendError> {
        self.run("SetSplitFreq", Op::SetSplitFreq(hz), |s| s.split_freq = hz)
    }
    fn set_split_mode(&mut self, mode: &str) -> Result<(), BackendError> {
        self.run("SetSplitMode", Op::SetSplitMode(mode.into()), |_| ())
    }
    fn get_rit_on(&mut self) -> Result<bool, BackendError> {
        self.run("GetRit", Op::GetRit, |s| s.rit)
    }
    fn set_rit_on(&mut self, on: bool) -> Result<(), BackendError> {
        self.run("SetRit", Op::SetRit(on), |s| s.rit = on)
    }
}
