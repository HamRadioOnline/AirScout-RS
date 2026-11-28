//! Native libhamlib backend: a minimal hand-written binding over an opaque `RIG*`.
//! Linux and macOS only; Windows uses rigctld.

use std::ffi::{c_char, c_int, c_long, c_uint, c_void, CStr, CString};
use std::time::Duration;

use crate::backend::{Backend, BackendError};
use crate::config::SerialConfig;
use crate::hamlib_conf::{
    hamlib_conf, hamlib_conf_is_fatal, hamlib_error, RIG_CONF_END, RIG_DEBUG_ERR,
    RIG_PASSBAND_NOCHANGE, RIG_VFO_A, RIG_VFO_B, RIG_VFO_CURR,
};
use crate::RigIdent;

#[link(name = "hamlib")]
extern "C" {
    fn rig_set_debug(level: c_int);
    fn rig_init(model: c_uint) -> *mut c_void;
    fn rig_token_lookup(rig: *mut c_void, name: *const c_char) -> c_long;
    fn rig_set_conf(rig: *mut c_void, token: c_long, val: *const c_char) -> c_int;
    fn rig_open(rig: *mut c_void) -> c_int;
    fn rig_close(rig: *mut c_void) -> c_int;
    fn rig_cleanup(rig: *mut c_void) -> c_int;
    fn rig_get_freq(rig: *mut c_void, vfo: c_uint, freq: *mut f64) -> c_int;
    fn rig_set_freq(rig: *mut c_void, vfo: c_uint, freq: f64) -> c_int;
    fn rig_get_mode(rig: *mut c_void, vfo: c_uint, mode: *mut u64, width: *mut c_long) -> c_int;
    fn rig_set_mode(rig: *mut c_void, vfo: c_uint, mode: u64, width: c_long) -> c_int;
    fn rig_get_split_vfo(
        rig: *mut c_void,
        vfo: c_uint,
        split: *mut c_int,
        tx_vfo: *mut c_uint,
    ) -> c_int;
    fn rig_set_split_vfo(rig: *mut c_void, vfo: c_uint, split: c_int, tx_vfo: c_uint) -> c_int;
    fn rig_get_split_freq(rig: *mut c_void, vfo: c_uint, freq: *mut f64) -> c_int;
    fn rig_set_split_freq(rig: *mut c_void, vfo: c_uint, freq: f64) -> c_int;
    fn rig_set_split_mode(rig: *mut c_void, vfo: c_uint, mode: u64, width: c_long) -> c_int;
    fn rig_get_func(rig: *mut c_void, vfo: c_uint, func: u64, status: *mut c_int) -> c_int;
    fn rig_set_func(rig: *mut c_void, vfo: c_uint, func: u64, status: c_int) -> c_int;
    fn rig_parse_mode(s: *const c_char) -> u64;
    fn rig_strrmode(mode: u64) -> *const c_char;
    fn rig_parse_func(s: *const c_char) -> u64;
}

pub struct NativeHamlib {
    model: u32,
    serial: SerialConfig,
    timeout: Duration,
    rig: *mut c_void,
    open: bool,
}

// SAFETY: the `RIG*` is owned by this value alone and never shared or aliased;
// the CAT worker moves the backend to its thread once and uses it only there.
unsafe impl Send for NativeHamlib {}

fn check(code: c_int) -> Result<(), BackendError> {
    if code < 0 {
        Err(hamlib_error(code))
    } else {
        Ok(())
    }
}

fn parse_mode(name: &str) -> Result<u64, BackendError> {
    let c = CString::new(name).map_err(|_| BackendError::Rejected(-1))?;
    // SAFETY: `c` is a valid NUL-terminated string that outlives the call.
    let m = unsafe { rig_parse_mode(c.as_ptr()) };
    if m == 0 {
        Err(BackendError::Rejected(-1))
    } else {
        Ok(m)
    }
}

fn to_hz(f: f64) -> i64 {
    f.round() as i64
}

fn positive_hz(hz: i64) -> Result<f64, BackendError> {
    // No frequency <= 0 Hz is ever written to a rig.
    if hz <= 0 {
        Err(BackendError::Rejected(-1))
    } else {
        Ok(hz as f64)
    }
}

impl NativeHamlib {
    pub fn new(model: u32, serial: SerialConfig, timeout: Duration) -> Self {
        Self {
            model,
            serial,
            timeout,
            rig: std::ptr::null_mut(),
            open: false,
        }
    }

    fn handle(&self) -> Result<*mut c_void, BackendError> {
        if self.open {
            Ok(self.rig)
        } else {
            Err(BackendError::Lost("not connected".into()))
        }
    }

    fn cleanup(&mut self) {
        if !self.rig.is_null() {
            // SAFETY: `rig` came from `rig_init`, is not used after this call
            // and is nulled right away; `rig_close` only runs on an open rig.
            unsafe {
                if self.open {
                    rig_close(self.rig);
                }
                rig_cleanup(self.rig);
            }
            self.rig = std::ptr::null_mut();
        }
        self.open = false;
    }

    fn configure(&mut self) -> Result<(), BackendError> {
        for (name, value) in hamlib_conf(&self.serial, self.timeout) {
            let (Ok(n), Ok(v)) = (CString::new(name), CString::new(value)) else {
                return Err(BackendError::NoPort(format!(
                    "hamlib: {name}: invalid value"
                )));
            };
            // SAFETY: `rig` is a live handle from `rig_init`; `n` is a valid C string.
            let token = unsafe { rig_token_lookup(self.rig, n.as_ptr()) };
            if token == RIG_CONF_END {
                continue;
            }
            // SAFETY: as above; `v` is a valid C string that outlives the call.
            let code = unsafe { rig_set_conf(self.rig, token, v.as_ptr()) };
            if code < 0 && hamlib_conf_is_fatal(name) {
                return Err(BackendError::NoPort(format!(
                    "hamlib: {name}: error {code}"
                )));
            }
        }
        Ok(())
    }

    fn get_f64(
        &mut self,
        f: unsafe extern "C" fn(*mut c_void, c_uint, *mut f64) -> c_int,
    ) -> Result<i64, BackendError> {
        let rig = self.handle()?;
        let mut v = 0.0f64;
        // SAFETY: `rig` is a live open handle; `v` is a valid out pointer.
        check(unsafe { f(rig, RIG_VFO_CURR, &mut v) })?;
        Ok(to_hz(v))
    }
}

impl Backend for NativeHamlib {
    fn ident(&self) -> RigIdent {
        RigIdent {
            cat_version: "Hamlib (native)".into(),
            rig_type: format!("[Hamlib] model {}", self.model),
        }
    }

    fn connect(&mut self) -> Result<(), BackendError> {
        if self.open {
            return Ok(());
        }
        self.cleanup();
        // SAFETY: plain value argument.
        unsafe { rig_set_debug(RIG_DEBUG_ERR) };
        // SAFETY: plain value argument; null is checked below.
        let rig = unsafe { rig_init(self.model) };
        if rig.is_null() {
            return Err(BackendError::NoPort(format!(
                "hamlib: unknown rig model {}",
                self.model
            )));
        }
        self.rig = rig;
        if let Err(e) = self.configure() {
            self.cleanup();
            return Err(e);
        }
        // SAFETY: `rig` is a live handle from `rig_init`.
        let code = unsafe { rig_open(self.rig) };
        if code < 0 {
            self.cleanup();
            return Err(BackendError::NoPort(format!(
                "hamlib: cannot open the rig ({code})"
            )));
        }
        self.open = true;
        Ok(())
    }

    fn close(&mut self) {
        self.cleanup();
    }

    fn get_freq(&mut self) -> Result<i64, BackendError> {
        self.get_f64(rig_get_freq)
    }

    fn set_freq(&mut self, hz: i64) -> Result<(), BackendError> {
        let rig = self.handle()?;
        let f = positive_hz(hz)?;
        // SAFETY: `rig` is a live open handle.
        check(unsafe { rig_set_freq(rig, RIG_VFO_CURR, f) })
    }

    fn get_mode(&mut self) -> Result<String, BackendError> {
        let rig = self.handle()?;
        let (mut mode, mut width) = (0u64, 0 as c_long);
        // SAFETY: `rig` is a live open handle; both out pointers are valid.
        check(unsafe { rig_get_mode(rig, RIG_VFO_CURR, &mut mode, &mut width) })?;
        // SAFETY: plain value argument.
        let p = unsafe { rig_strrmode(mode) };
        if p.is_null() {
            return Err(BackendError::Rejected(-1));
        }
        // SAFETY: hamlib returns a NUL-terminated string with static lifetime.
        Ok(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }

    fn set_mode(&mut self, mode: &str) -> Result<(), BackendError> {
        let rig = self.handle()?;
        let m = parse_mode(mode)?;
        // SAFETY: `rig` is a live open handle.
        check(unsafe { rig_set_mode(rig, RIG_VFO_CURR, m, RIG_PASSBAND_NOCHANGE) })
    }

    fn get_split(&mut self) -> Result<bool, BackendError> {
        let rig = self.handle()?;
        let (mut split, mut tx) = (0 as c_int, 0 as c_uint);
        // SAFETY: `rig` is a live open handle; both out pointers are valid.
        check(unsafe { rig_get_split_vfo(rig, RIG_VFO_CURR, &mut split, &mut tx) })?;
        Ok(split != 0)
    }

    fn set_split(&mut self, on: bool) -> Result<(), BackendError> {
        let rig = self.handle()?;
        let tx = if on { RIG_VFO_B } else { RIG_VFO_A };
        // SAFETY: `rig` is a live open handle.
        check(unsafe { rig_set_split_vfo(rig, RIG_VFO_CURR, c_int::from(on), tx) })
    }

    fn get_split_freq(&mut self) -> Result<i64, BackendError> {
        self.get_f64(rig_get_split_freq)
    }

    fn set_split_freq(&mut self, hz: i64) -> Result<(), BackendError> {
        let rig = self.handle()?;
        let f = positive_hz(hz)?;
        // SAFETY: `rig` is a live open handle.
        check(unsafe { rig_set_split_freq(rig, RIG_VFO_CURR, f) })
    }

    fn set_split_mode(&mut self, mode: &str) -> Result<(), BackendError> {
        let rig = self.handle()?;
        let m = parse_mode(mode)?;
        // SAFETY: `rig` is a live open handle.
        check(unsafe { rig_set_split_mode(rig, RIG_VFO_CURR, m, RIG_PASSBAND_NOCHANGE) })
    }

    fn get_rit_on(&mut self) -> Result<bool, BackendError> {
        let rig = self.handle()?;
        let func = rit_func()?;
        let mut status = 0 as c_int;
        // SAFETY: `rig` is a live open handle; `status` is a valid out pointer.
        check(unsafe { rig_get_func(rig, RIG_VFO_CURR, func, &mut status) })?;
        Ok(status != 0)
    }

    fn set_rit_on(&mut self, on: bool) -> Result<(), BackendError> {
        let rig = self.handle()?;
        let func = rit_func()?;
        // SAFETY: `rig` is a live open handle.
        check(unsafe { rig_set_func(rig, RIG_VFO_CURR, func, c_int::from(on)) })
    }
}

fn rit_func() -> Result<u64, BackendError> {
    // SAFETY: the literal is a valid NUL-terminated string.
    let f = unsafe { rig_parse_func(c"RIT".as_ptr()) };
    if f == 0 {
        Err(BackendError::Rejected(-1))
    } else {
        Ok(f)
    }
}

impl Drop for NativeHamlib {
    fn drop(&mut self) {
        self.cleanup();
    }
}
