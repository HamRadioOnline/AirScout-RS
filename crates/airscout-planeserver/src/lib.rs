//! AirScout-RS PlaneServer: collects plane feeds and serves `/planes.json`.
#![forbid(unsafe_code)]

pub mod config;
pub mod gui;
pub mod service;
pub mod tray;

use config::PlaneServerConfig;
use std::net::SocketAddr;

/// The lines printed to stderr when the daemon starts. `bound` is the address the server
/// listens on, or `None` while it does not listen (not yet, or the bind failed). Then no
/// "serving" line is printed: `server_status` (the service's server text, which holds the
/// error) is shown instead. Names only the address, never other config values.
pub fn start_lines(
    cfg: &PlaneServerConfig,
    bound: Option<SocketAddr>,
    server_status: &str,
) -> Vec<String> {
    let addr = bound.unwrap_or_else(|| SocketAddr::new(cfg.bind, cfg.port));
    let mut lines = vec![match bound {
        Some(addr) => {
            format!(
                "airscout-planeserver: serving /planes.json on http://{addr}/ (no authentication)"
            )
        }
        None => format!("airscout-planeserver: not serving: {server_status}"),
    }];
    if !addr.ip().is_loopback() {
        lines.push("warning: port open to the whole network".to_owned());
    }
    lines
}
