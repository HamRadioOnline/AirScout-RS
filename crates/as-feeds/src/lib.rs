//! Plane feeds, ported from the `AirScout.PlaneFeeds.Plugin.*` projects. Each feed implements
//! [`PlaneFeed`]; [`registry`] lists them.
#![forbid(unsafe_code)]

pub mod feeds;
pub mod pipeline;
mod plugin;
pub mod registry;
pub mod vars;
pub mod wake;
pub mod worker;

pub use plugin::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
