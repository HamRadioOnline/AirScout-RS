//! Embedded web server settings (stage 4d). Defaults from `Settings.settings:260-261`,
//! `:493-494` and `:772-776`. The Options dialog shows only the port, as in C#.

/// The HTTP server and the values the browser page reads from `/settings.json`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Web {
    /// `Webserver_Port`. The server listens on all interfaces while `server.activate` is on.
    pub port: u16,
    /// `KST_Websocket`: the wtKST websocket the page connects to. Port and path are the
    /// upstream default; the host is `localhost`, not the upstream private address.
    pub kst_websocket: String,
    /// `KST_WebSocket_Timeout` [s].
    pub kst_websocket_timeout_s: f64,
    /// `Map_SmallMarkers`.
    pub map_small_markers: bool,
}

impl Default for Web {
    fn default() -> Self {
        Self {
            port: 9880,
            kst_websocket: "ws://localhost:9881/AirScout".into(),
            kst_websocket_timeout_s: 150.0,
            map_small_markers: true,
        }
    }
}
