//! Win-Test server settings (stage 4a). Defaults from `Settings.settings:116, :140, :143`.

/// UDP server that receives Win-Test and AirScout messages.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Server {
    /// `Server_Activate`.
    pub activate: bool,
    /// `Server_Port`. Same default as the AirScout track output port (both 9872, 1:1).
    pub port: u16,
    /// `Server_Name`. Compared exactly by the receiver, so it is never trimmed.
    pub name: String,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            activate: false,
            port: 9872,
            name: "AS".into(),
        }
    }
}
