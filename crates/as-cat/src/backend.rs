//! Hamlib-level commands; one implementation per transport.

use crate::RigIdent;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    /// The CAT engine is not available (no daemon, no library).
    #[error("{0}")]
    NoCat(String),
    /// The port or device cannot be used.
    #[error("{0}")]
    NoPort(String),
    #[error("the rig rejected the command (RPRT {0})")]
    Rejected(i32),
    #[error("no answer from the rig")]
    Timeout,
    /// The connection is gone or closed.
    #[error("{0}")]
    Lost(String),
}

/// Rig commands at hamlib level. Every operation except `connect` returns
/// `Lost("not connected")` while the backend is closed.
pub trait Backend: Send {
    fn ident(&self) -> RigIdent;
    /// `Ok` at once when already connected.
    fn connect(&mut self) -> Result<(), BackendError>;
    fn close(&mut self);
    fn get_freq(&mut self) -> Result<i64, BackendError>;
    fn set_freq(&mut self, hz: i64) -> Result<(), BackendError>;
    fn get_mode(&mut self) -> Result<String, BackendError>;
    fn set_mode(&mut self, mode: &str) -> Result<(), BackendError>;
    /// TX VFO is B when on, A when off.
    fn get_split(&mut self) -> Result<bool, BackendError>;
    fn set_split(&mut self, on: bool) -> Result<(), BackendError>;
    fn get_split_freq(&mut self) -> Result<i64, BackendError>;
    fn set_split_freq(&mut self, hz: i64) -> Result<(), BackendError>;
    fn set_split_mode(&mut self, mode: &str) -> Result<(), BackendError>;
    fn get_rit_on(&mut self) -> Result<bool, BackendError>;
    fn set_rit_on(&mut self, on: bool) -> Result<(), BackendError>;
}
