//! The OmniRig COM backend (Windows): late-bound `IDispatch` calls on the `OmniRig.OmniRigX`
//! engine and its `RigN` objects, as the C# `dynamic` calls do (`OmniRigX.cs:87-152`). The only
//! module of the crate with `unsafe`, apart from `hamlib.rs`.
//!
//! COM is initialised in the multithreaded apartment by every [`opener`] call, on the calling
//! thread (the `cat` worker), and uninitialised on that thread when the engine is dropped.

use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ptr::null_mut;
use std::thread::ThreadId;

use windows::core::{w, GUID, PCWSTR};
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch,
    CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, DISPATCH_FLAGS,
    DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS,
};
use windows::Win32::System::Ole::DISPID_PROPERTYPUT;
use windows::Win32::System::Variant::{
    VariantChangeType, VariantClear, VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0,
    VAR_CHANGE_FLAGS, VT_BOOL, VT_DISPATCH, VT_I4,
};

use crate::omnirig::{OmniDispatch, OmniError, OmniOpen, OmniProp};

/// CLSID of OmniRig V1.19, tried when the ProgID is not registered (`OmniRigX.cs:114`).
const OMNIRIG_V1_CLSID: GUID = GUID::from_u128(0x4FE359C5_A58F_459D_BE95_CA559FB4F270);

/// `LOCALE_USER_DEFAULT`, the locale of late-bound calls.
const LCID: u32 = 0x0400;

/// `IID_NULL`, required by `GetIDsOfNames` and `Invoke`.
const IID_NULL: GUID = GUID::zeroed();

/// Text of a COM error: the HRESULT and its system message.
fn text(e: &windows::core::Error) -> String {
    format!("{:?} {}", e.code(), e.message())
}

fn err(e: windows::core::Error) -> OmniError {
    OmniError(text(&e))
}

/// A NUL-terminated UTF-16 copy of `s`.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Keeps COM initialised on the thread that made it; `CoUninitialize` on drop, on that thread
/// only. Not `Send` by itself.
struct ComGuard {
    /// The thread whose `CoInitializeEx` this guard balances.
    owner: ThreadId,
    _not_send: PhantomData<*mut ()>,
}

impl ComGuard {
    fn init() -> Result<Self, String> {
        // SAFETY: the reserved pointer is null as required. S_OK and S_FALSE both count as one
        // initialisation that the guard balances; a failure (RPC_E_CHANGED_MODE) needs no
        // `CoUninitialize` and returns before a guard exists.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|e| text(&e))?;
        Ok(Self {
            owner: std::thread::current().id(),
            _not_send: PhantomData,
        })
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        let here = std::thread::current().id();
        debug_assert_eq!(self.owner, here, "COM guard dropped on another thread");
        // On another thread `CoUninitialize` would unbalance that thread's apartment; leaving
        // COM initialised on the owner thread is the lesser harm.
        if self.owner != here {
            return;
        }
        // SAFETY: balances the successful `CoInitializeEx` of `init` on this same thread (just
        // checked against `owner`), after every interface pointer of `ComOmni` has been
        // released (its field order).
        unsafe { CoUninitialize() }
    }
}

/// An owned `VARIANT`, cleared on drop (releases a returned `BSTR` or interface).
struct Var(VARIANT);

impl Var {
    fn empty() -> Self {
        Var(VARIANT::default())
    }

    fn vt(&self) -> VARENUM {
        // SAFETY: every `VARIANT` is read through `Anonymous.Anonymous`; `vt` is the first field
        // of that view and is always initialised (zero is VT_EMPTY).
        unsafe { self.0.Anonymous.Anonymous.vt }
    }

    /// A copy of the value converted to `vt` (`VariantChangeType`).
    fn coerce(&self, vt: VARENUM) -> Result<Var, OmniError> {
        let mut out = Var::empty();
        // SAFETY: both pointers refer to valid, initialised VARIANTs owned here; `out` is empty,
        // so nothing in it leaks when it is overwritten.
        unsafe { VariantChangeType(&mut out.0, &self.0, VAR_CHANGE_FLAGS(0), vt) }.map_err(err)?;
        Ok(out)
    }

    fn to_i32(&self) -> Result<i32, OmniError> {
        let v = self.coerce(VT_I4)?;
        // SAFETY: `VariantChangeType` to VT_I4 succeeded, so `lVal` is the active field.
        Ok(unsafe { v.0.Anonymous.Anonymous.Anonymous.lVal })
    }

    fn to_bool(&self) -> Result<bool, OmniError> {
        let v = self.coerce(VT_BOOL)?;
        // SAFETY: `VariantChangeType` to VT_BOOL succeeded, so `boolVal` is the active field.
        Ok(unsafe { v.0.Anonymous.Anonymous.Anonymous.boolVal }.0 != 0)
    }

    /// The object of a VT_DISPATCH value, with its own reference; `None` for any other type.
    fn to_dispatch(&self) -> Option<IDispatch> {
        if self.vt() != VT_DISPATCH {
            return None;
        }
        // SAFETY: the type is VT_DISPATCH, so `pdispVal` is the active field; `clone` adds a
        // reference, and the VARIANT keeps (and on drop releases) its own.
        unsafe { (*self.0.Anonymous.Anonymous.Anonymous.pdispVal).clone() }
    }
}

impl Drop for Var {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid VARIANT owned by this value; clearing it frees what it
        // owns and leaves VT_EMPTY.
        let _ = unsafe { VariantClear(&mut self.0) };
    }
}

/// A VT_I4 argument (owns nothing, so it needs no clear).
fn i4(v: i32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { lVal: v },
            }),
        },
    }
}

/// The DISPID of member `name`; fails when the object has no such member.
fn dispid(d: &IDispatch, name: &str) -> Result<i32, OmniError> {
    let w = wide(name);
    let names = [PCWSTR(w.as_ptr())];
    let mut id = 0i32;
    // SAFETY: `names` holds one NUL-terminated UTF-16 string that outlives the call, `cnames`
    // is 1, and `id` is room for one DISPID.
    unsafe { d.GetIDsOfNames(&IID_NULL, names.as_ptr(), 1, LCID, &mut id) }.map_err(err)?;
    Ok(id)
}

/// One late-bound call of member `name` with at most one VT_I4 argument.
fn invoke(
    d: &IDispatch,
    name: &str,
    flags: DISPATCH_FLAGS,
    arg: Option<i32>,
) -> Result<Var, OmniError> {
    let id = dispid(d, name)?;
    let put = flags == DISPATCH_PROPERTYPUT;
    let mut argv = arg.map(i4);
    let mut put_id = DISPID_PROPERTYPUT;
    let params = DISPPARAMS {
        rgvarg: argv.as_mut().map_or(null_mut(), |a| a as *mut VARIANT),
        rgdispidNamedArgs: if put {
            &mut put_id as *mut i32
        } else {
            null_mut()
        },
        cArgs: u32::from(arg.is_some()),
        cNamedArgs: u32::from(put),
    };
    let mut out = Var::empty();
    let result = if put {
        None
    } else {
        Some(&mut out.0 as *mut VARIANT)
    };
    // SAFETY: `params` points at `argv` and `put_id`, which live until the call returns, with
    // counts that match; `result` is an empty VARIANT owned by `out` (none for a put, as COM
    // requires); no EXCEPINFO is asked for, so nothing in it needs freeing.
    unsafe { d.Invoke(id, &IID_NULL, LCID, flags, &params, result, None, None) }.map_err(err)?;
    Ok(out)
}

fn prop_get(d: &IDispatch, name: &str) -> Result<Var, OmniError> {
    invoke(d, name, DISPATCH_PROPERTYGET, None)
}

/// The OmniRig engine object and the rig objects read from it.
struct ComOmni {
    /// `RigN` objects, filled by `has_rig` and the first call on a rig (at most one per `u8`).
    rigs: Vec<(u8, IDispatch)>,
    engine: IDispatch,
    /// Declared last, so it is dropped after every interface pointer above.
    _com: ComGuard,
}

// SAFETY: a `ComOmni` (with its interface pointers and the COM guard) is created by the rig
// factory inside `CatWorker::pass`, on the `cat` thread, used there only, and dropped there.
// `Send` is needed only because `OmniDispatch` is `Send`, as every rig behind the worker is.
// The guard checks that invariant on drop (debug assertion; it never uninitialises COM on a
// thread other than the one that initialised it).
unsafe impl Send for ComOmni {}

impl ComOmni {
    fn rig(&mut self, n: u8) -> Result<IDispatch, OmniError> {
        if let Some((_, d)) = self.rigs.iter().find(|(k, _)| *k == n) {
            return Ok(d.clone());
        }
        let name = format!("Rig{n}");
        let d = prop_get(&self.engine, &name)?
            .to_dispatch()
            .ok_or_else(|| OmniError(format!("{name} is not an object")))?;
        self.rigs.push((n, d.clone()));
        Ok(d)
    }

    fn call(&mut self, n: u8, name: &str, param: i32) -> Result<bool, OmniError> {
        let r = self.rig(n)?;
        invoke(&r, name, DISPATCH_METHOD, Some(param))?.to_bool()
    }
}

impl OmniDispatch for ComOmni {
    fn interface_version(&mut self) -> Result<i32, OmniError> {
        prop_get(&self.engine, "InterfaceVersion")?.to_i32()
    }

    fn has_rig(&mut self, n: u8) -> bool {
        self.rig(n).is_ok()
    }

    fn get(&mut self, n: u8, p: OmniProp) -> Result<i32, OmniError> {
        let r = self.rig(n)?;
        prop_get(&r, p.name())?.to_i32()
    }

    fn set(&mut self, n: u8, p: OmniProp, v: i32) -> Result<(), OmniError> {
        let r = self.rig(n)?;
        invoke(&r, p.name(), DISPATCH_PROPERTYPUT, Some(v)).map(drop)
    }

    fn is_readable(&mut self, n: u8, param: i32) -> Result<bool, OmniError> {
        self.call(n, "IsParamReadable", param)
    }

    fn is_writeable(&mut self, n: u8, param: i32) -> Result<bool, OmniError> {
        self.call(n, "IsParamWriteable", param)
    }
}

/// `StartOmniRig` (`OmniRigX.cs:87-152`): the ProgID `OmniRig.OmniRigX`, else the V1.19 CLSID
/// (:108-115), created out of process or in process as `IDispatch`.
fn open() -> Result<Box<dyn OmniDispatch>, String> {
    let com = ComGuard::init()?;
    // SAFETY: the argument is a NUL-terminated wide string literal.
    let clsid = unsafe { CLSIDFromProgID(w!("OmniRig.OmniRigX")) }.unwrap_or(OMNIRIG_V1_CLSID);
    // SAFETY: `clsid` lives for the call; no aggregation; COM is initialised on this thread.
    let engine: IDispatch =
        unsafe { CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER | CLSCTX_INPROC_SERVER) }
            .map_err(|e| text(&e))?;
    Ok(Box::new(ComOmni {
        rigs: Vec::new(),
        engine,
        _com: com,
    }))
}

/// The engine factory for [`crate::OmniRig`]; every call initialises COM on the calling thread.
pub fn opener() -> OmniOpen {
    Box::new(open)
}
