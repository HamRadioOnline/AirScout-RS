//! Mode-S / ADS-B decoding, ported from `LibADSB` (itself a port of the Java library by
//! Matthias Schäfer). Bugs of the original are kept and listed in `docs/logic-findings.md`.
#![forbid(unsafe_code)]

pub mod cpr;
pub mod crc;
pub mod decoder;
pub mod framing;
pub mod modes;
pub mod msg;

pub use modes::ModeSReply;
pub use msg::{
    decode, AirbornePosition, AirspeedHeading, Identification, Msg, SurfacePosition,
    VelocityOverGround,
};

/// Decoding errors. The variants mirror the C# exception types.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum AdsbError {
    /// `BadFormatException`: wrong length, wrong type code, wrong pairing.
    #[error("bad format: {0}")]
    BadFormat(String),
    /// `FormatException` of `Convert.ToByte`: a non-hex character where C# does not swallow it.
    #[error("not a hex string: {0}")]
    BadHex(String),
    /// `MissingInformationException`.
    #[error("missing information: {0}")]
    MissingInformation(String),
    /// `PositionStraddleException`: the even/odd pair straddles a latitude zone border.
    #[error("positions straddle a transition latitude")]
    PositionStraddle,
    /// `IllegalArgumentException`: the two messages come from different transmitters.
    #[error("illegal argument: {0}")]
    IllegalArgument(String),
}
