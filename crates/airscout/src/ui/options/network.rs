//! Network tab: the Win-Test and AirScout UDP server and the HTTP server (`OptionsDlg` Network
//! tab, `:2583-2742`, `:2591-2598`).

use super::{num, text};
use crate::web_service::WebState;
use as_settings::{Settings, TrackUdp};
use eframe::egui::{Grid, Ui};

pub fn show(ui: &mut Ui, s: &mut Settings, web: &WebState) {
    ui.group(|ui| {
        ui.strong("Activate Server");
        ui.checkbox(&mut s.server.activate, "Activate Network Server")
            .on_hover_text("Starts the Win-Test UDP server and the HTTP server.");
    });
    ui.group(|ui| {
        ui.strong("UDP Server Settings");
        Grid::new("opt_network").num_columns(2).show(ui, |ui| {
            num(
                ui,
                "AirScout UDP Server Port:",
                &mut s.server.port,
                0..=65535,
            );
            text(ui, "AirScout UDP Server Name:", &mut s.server.name);
        });
        ui.label(
            "The UDP server listens on all network interfaces (0.0.0.0) without authentication. \
             Win-Test and wtKST send to it.",
        );
    });
    ui.group(|ui| {
        ui.strong("HTTP Server Settings");
        Grid::new("opt_network_http").num_columns(2).show(ui, |ui| {
            num(ui, "AirScout HTTP Server Port:", &mut s.web.port, 0..=65535);
        });
        ui.label(
            "The HTTP server listens on all network interfaces (0.0.0.0) without authentication \
             or encryption. Anyone on your network can open it and read your call, position, DX \
             station and watchlist.",
        );
        ui.label(
            "The served plane feed URL and KST websocket URL have their username, password and \
             query string removed. A credential placed in the URL path is served as configured \
             and still reaches the network: do not put a token in the path.",
        );
        ui.label(format!(
            "Open http://localhost:{}/ in a browser on this computer.",
            s.web.port
        ));
        ui.label(web_state_text(web));
    });
}

/// What the HTTP server does now, in plain words. The error is the bind error only.
fn web_state_text(web: &WebState) -> String {
    match web {
        WebState::Off => "HTTP server: off".to_string(),
        WebState::Starting => "HTTP server: starting...".to_string(),
        WebState::Listening(port) => format!("HTTP server: listening on port {port}"),
        WebState::NotListening(e) => {
            // the server's error texts start with its own name
            let e = e.strip_prefix("Web server: ").unwrap_or(e);
            format!("HTTP server: not listening: {e}")
        }
    }
}

/// Errors that block OK: the UDP and the HTTP server need different ports.
pub fn validate(s: &Settings) -> Vec<String> {
    let mut e = Vec::new();
    if s.server.port == s.web.port {
        e.push("UDP Server and HTTP Server must have different port numbers.".to_string());
    }
    e
}

/// Warnings for two enabled sockets on one port. They do not block OK.
pub fn port_warnings(s: &Settings) -> Vec<String> {
    let mut w = Vec::new();
    if s.server.activate && s.track.activate {
        let (port, name) = match s.track.udp {
            TrackUdp::None => return w,
            TrackUdp::WinTest => (s.track.udp_wintest_port, "Win-Test"),
            TrackUdp::AirScout => (s.track.udp_airscout_port, "AirScout"),
        };
        if port == s.server.port {
            w.push(format!(
                "Network: port {port} is used by the UDP server and by the tracking output ({name})."
            ));
        }
    }
    w
}
