//! DDE client of the Ham Radio Deluxe rotator output (`MapDlg.cs:7839-7861`, NDde `DdeClient`).
//!
//! The [`DdeLink`] seam and the constants compile on every OS, so the output logic is tested with a
//! fake link everywhere. The DDEML binding [`Ddeml`] exists on Windows only.
#![cfg_attr(not(windows), forbid(unsafe_code))]
#![cfg_attr(windows, deny(unsafe_code))]

use std::time::Duration;

#[cfg(windows)]
#[allow(unsafe_code)]
mod ddeml;

#[cfg(windows)]
pub use ddeml::Ddeml;

/// DDE service name of HRD Rotator (`new DdeClient("HRDRotator", "Position")`, `MapDlg.cs:8020`).
pub const SERVICE: &str = "HRDRotator";
/// DDE topic name (`MapDlg.cs:8020`).
pub const TOPIC: &str = "Position";
/// Item of every poke (`client.TryPoke("PositionData", ...)`, `MapDlg.cs:7853`, `:7859`).
pub const ITEM: &str = "PositionData";
/// Clipboard format of the pokes: `1` (`CF_TEXT`), `MapDlg.cs:7853`.
pub const CF_TEXT: u32 = 1;
/// Timeout of one synchronous poke: 10000 ms (`MapDlg.cs:7853`, 1:1).
pub const POKE_TIMEOUT: Duration = Duration::from_millis(10_000);

/// A DDE client conversation. Every call blocks; a poke at most for its `timeout`.
pub trait DdeLink: Send {
    /// Opens a conversation with `service`/`topic`. The error is a text for the status line.
    fn connect(&mut self, service: &str, topic: &str) -> Result<(), String>;
    /// True while the conversation is up; turns false when the server ends it (as NDde's
    /// `IsConnected` on `XTYP_DISCONNECT`), so the output counts "[DDE]: Client not connected.".
    fn is_connected(&self) -> bool;
    /// Pokes `data` as given (no terminating NUL) to `item`; true when the server took it.
    fn poke(&mut self, item: &str, data: &[u8], format: u32, timeout: Duration) -> bool;
    /// Ends the conversation; a no-op without one.
    fn disconnect(&mut self);
}
