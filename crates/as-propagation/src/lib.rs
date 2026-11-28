//! Propagation path, plane intersection and horizon, ported from `ScoutBase.Propagation`.
#![forbid(unsafe_code)]

mod engine;
mod horizon;
mod path;
mod point;

pub use engine::{PropagationEngine, PropagationError};
pub use horizon::{epsilon_max_from_path, HorizonParams, HorizonPoint, PropagationHorizon};
pub use path::{PathParams, PropagationPath};
pub use point::{IntersectionPoint, PropagationPoint};
