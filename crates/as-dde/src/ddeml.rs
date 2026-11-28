//! The DDEML binding (`user32.dll`): a client-only instance with one conversation, as NDde's
//! `DdeClient` (`NDde/Internal/Client/DdemlClient.cs`, `TryConnect` :224-263, `TryPoke` :605-672)
//! uses it. The only module of the crate with `unsafe`.
//!
//! NDde runs DDEML on its own thread with a message loop (`DdeThread`,
//! `NDde/Public/Advanced/DdeContext.cs:629-757`), which delivers `XTYP_DISCONNECT`
//! (`DdemlClient.cs:1353`) and turns `IsConnected` false when the server goes away. The port has
//! no such loop: it drains the calling thread's queue before every query and transaction, and asks
//! DDEML for the state of the conversation itself (`DdeQueryConvInfo`). A dead conversation is
//! dropped by `is_connected`, which keeps the instance; the instance is torn down only by
//! `disconnect` or on drop. The caller calls `disconnect` once `is_connected` turns false (the
//! Track output does so in its idle path, `DdeOutput::idle`, and on close), so no hidden DDEML
//! window outlives the conversation unpumped.

use crate::DdeLink;
use std::cell::Cell;
use std::ptr::null_mut;
use std::thread::ThreadId;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::System::DataExchange::{
    DdeClientTransaction, DdeConnect, DdeCreateStringHandleW, DdeDisconnect, DdeFreeStringHandle,
    DdeGetLastError, DdeInitializeW, DdeQueryConvInfo, DdeUninitialize, APPCMD_CLIENTONLY,
    CONVINFO, CP_WINUNICODE, DMLERR_NO_CONV_ESTABLISHED, DMLERR_NO_ERROR, HCONV, HDDEDATA, HSZ,
    QID_SYNC, ST_CONNECTED, XTYP_POKE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

/// `TIMEOUT_ASYNC` (`0xFFFF_FFFF`) would turn a poke asynchronous; a synchronous timeout stays below.
const MAX_SYNC_TIMEOUT_MS: u32 = u32::MAX - 1;

/// At most this many queued messages are dispatched per drain, so a flood cannot stall a step.
const MAX_PUMP: usize = 1000;

/// The DDEML callback of a client that only pokes: no transaction reaches it that needs an answer.
/// `XTYP_DISCONNECT` arrives here too; the port reads the state with `DdeQueryConvInfo` instead.
unsafe extern "system" fn no_callback(
    _wtype: u32,
    _wfmt: u32,
    _hconv: HCONV,
    _hsz1: HSZ,
    _hsz2: HSZ,
    _hdata: HDDEDATA,
    _dwdata1: usize,
    _dwdata2: usize,
) -> HDDEDATA {
    HDDEDATA(null_mut())
}

/// A NUL-terminated UTF-16 copy of `s` for `DdeCreateStringHandleW`.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Dispatches the messages queued for the calling thread (the hidden DDEML windows live there).
fn pump() {
    let mut msg = MSG::default();
    for _ in 0..MAX_PUMP {
        // SAFETY: `msg` is a valid, writable `MSG`; a null window filter takes every message of
        // the calling thread; `PM_REMOVE` takes it off the queue.
        let got = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) };
        if !got.as_bool() {
            break;
        }
        // SAFETY: `msg` was just filled by `PeekMessageW` and is only read by both calls.
        unsafe {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}

/// DDEML client. `new` makes no call. `connect` initialises the instance on the calling thread,
/// and every later call must come from that thread (DDEML instances are bound to the thread that
/// created them); `disconnect` and a failed `connect` uninitialise it again.
pub struct Ddeml {
    /// DDEML instance identifier; 0 while there is no instance.
    inst: u32,
    /// The conversation; null without one. A `Cell` so that `is_connected` can drop a dead one.
    hconv: Cell<HCONV>,
    /// The thread that initialised `inst`.
    owner: Option<ThreadId>,
}

// SAFETY: the DDEML instance and the conversation handle are thread-affine. A `Ddeml` is created
// by `build_outputs`, which `TrackService::start` hands to `start_with` as the closure that runs
// inside the Track worker's thread (`track_service.rs`, `run`); that thread then owns it for its
// whole life: it calls, idles, closes and drops it on that one thread. `Send` is only needed
// because `TrackOutput: Send`; no `Ddeml` is ever used on a second thread (checked in debug
// builds by `owner`).
unsafe impl Send for Ddeml {}

impl Ddeml {
    pub fn new() -> Self {
        Self {
            inst: 0,
            hconv: Cell::new(HCONV(null_mut())),
            owner: None,
        }
    }

    fn check_thread(&self) {
        debug_assert!(
            self.owner.is_none() || self.owner == Some(std::thread::current().id()),
            "a Ddeml is used on a second thread"
        );
    }

    /// A string handle of `s` on this instance; null on failure.
    fn string_handle(&self, s: &str) -> HSZ {
        let w = wide(s);
        // SAFETY: `self.inst` is a live instance (callers check `inst != 0`); `w` is a
        // NUL-terminated UTF-16 buffer that outlives the call, which copies it.
        unsafe { DdeCreateStringHandleW(self.inst, PCWSTR(w.as_ptr()), CP_WINUNICODE) }
    }

    fn free_string_handle(&self, h: HSZ) {
        if !h.0.is_null() {
            // SAFETY: `h` was created by `string_handle` on this live instance and is freed once.
            unsafe {
                let _ = DdeFreeStringHandle(self.inst, h);
            }
        }
    }

    fn last_error(&self) -> u32 {
        // SAFETY: `DdeGetLastError` only reads the error state of the instance `self.inst`.
        unsafe { DdeGetLastError(self.inst) }
    }

    /// Ends the conversation, if any; the instance stays.
    fn drop_conversation(&self) {
        let h = self.hconv.replace(HCONV(null_mut()));
        if !h.0.is_null() {
            // SAFETY: `h` is a conversation of this instance; it was taken out of `hconv` above,
            // so it is ended only once.
            unsafe {
                let _ = DdeDisconnect(h);
            }
        }
    }

    /// Ends the conversation and the instance, so no DDEML window is left.
    fn shut_down(&mut self) {
        self.drop_conversation();
        if self.inst != 0 {
            // SAFETY: `self.inst` is the live instance of this value; its conversation and
            // string handles are gone; it is cleared at once, so it is uninitialised once.
            unsafe {
                let _ = DdeUninitialize(self.inst);
            }
            self.inst = 0;
            self.owner = None;
        }
    }

    /// True while DDEML reports the conversation as connected.
    fn alive(&self, h: HCONV) -> bool {
        let mut ci = CONVINFO {
            cb: std::mem::size_of::<CONVINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `h` is a conversation of this instance; `ci` is writable and its `cb` holds its
        // size, as `DdeQueryConvInfo` requires.
        let n = unsafe { DdeQueryConvInfo(h, QID_SYNC, &mut ci) };
        n != 0 && ci.wStatus.0 & ST_CONNECTED.0 != 0
    }
}

impl Default for Ddeml {
    fn default() -> Self {
        Self::new()
    }
}

impl DdeLink for Ddeml {
    fn connect(&mut self, service: &str, topic: &str) -> Result<(), String> {
        self.check_thread();
        if self.is_connected() {
            return Ok(());
        }
        if self.inst == 0 {
            let mut inst = 0u32;
            // SAFETY: `inst` is a valid out pointer for the call; `no_callback` has the
            // `PFNCALLBACK` signature and is a plain function that lives for the whole program.
            let r =
                unsafe { DdeInitializeW(&mut inst, Some(no_callback), APPCMD_CLIENTONLY, None) };
            if r != DMLERR_NO_ERROR {
                return Err(format!("DDE: {service}/{topic}: error {r}"));
            }
            self.inst = inst;
            self.owner = Some(std::thread::current().id());
        }
        let hs = self.string_handle(service);
        let ht = self.string_handle(topic);
        let hconv = if hs.0.is_null() || ht.0.is_null() {
            // a null name would connect to any server
            HCONV(null_mut())
        } else {
            // SAFETY: both string handles are live on `self.inst`; a null `CONVCONTEXT` asks for
            // the default context.
            unsafe { DdeConnect(self.inst, hs, ht, None) }
        };
        let err = self.last_error();
        self.free_string_handle(hs);
        self.free_string_handle(ht);
        if hconv.0.is_null() {
            // no instance is kept without a conversation
            self.shut_down();
            return Err(format!("DDE: {service}/{topic}: error {err}"));
        }
        self.hconv.set(hconv);
        Ok(())
    }

    /// Drains the thread's queue whenever an instance exists, then asks DDEML; a conversation the
    /// server has ended is dropped (the caller then `disconnect`s, which ends the instance).
    fn is_connected(&self) -> bool {
        if self.inst == 0 {
            return false;
        }
        self.check_thread();
        pump();
        let h = self.hconv.get();
        if h.0.is_null() {
            return false;
        }
        if self.alive(h) {
            return true;
        }
        self.drop_conversation();
        false
    }

    fn poke(&mut self, item: &str, data: &[u8], format: u32, timeout: Duration) -> bool {
        let Ok(len) = u32::try_from(data.len()) else {
            return false;
        };
        let ms = u32::try_from(timeout.as_millis())
            .unwrap_or(u32::MAX)
            .min(MAX_SYNC_TIMEOUT_MS);
        // NDde refuses a timeout <= 0 (`DdemlClient.cs:627-630`)
        if !self.is_connected() || ms == 0 {
            return false;
        }
        let hi = self.string_handle(item);
        if hi.0.is_null() {
            return false;
        }
        // SAFETY: `data` is valid for `len` bytes during the call, which copies them; the
        // conversation is live on `self.inst` (checked above) and `hi` a live string handle on it;
        // the result flags are not asked for. The call is synchronous: it returns within `ms`.
        let r = unsafe {
            DdeClientTransaction(
                Some(data.as_ptr()),
                len,
                self.hconv.get(),
                Some(hi),
                format,
                XTYP_POKE,
                ms,
                None,
            )
        };
        let ok = !r.0.is_null();
        let err = if ok {
            DMLERR_NO_ERROR
        } else {
            self.last_error()
        };
        self.free_string_handle(hi);
        if err == DMLERR_NO_CONV_ESTABLISHED {
            // the server is gone: the next step reconnects or counts the error
            self.drop_conversation();
        }
        ok
    }

    /// Ends the conversation and the instance.
    fn disconnect(&mut self) {
        self.check_thread();
        self.shut_down();
    }
}

impl Drop for Ddeml {
    fn drop(&mut self) {
        self.shut_down();
    }
}
