//! Antenna tracking and Doppler settings (stage 3c). Defaults from `Settings.settings:239-245, :670-715`.
//! The outputs (UDP, file, serial and DDE) live here.
//! Serial defaults from `Settings.settings:275-281, :299`, DDE from `:305`.
//! UDP defaults from `Settings.settings:272, :296, :302, :311, :314`, file from `:287, :290, :308`.

/// Tracking values.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Track {
    pub activate: bool,
    /// Manual azimuth [deg].
    pub set_az: f64,
    /// Manual elevation [deg].
    pub set_el: f64,
    /// Time offset [s].
    pub offset_s: i64,
    /// Update interval [ms].
    pub update_ms: u64,
    /// UDP output. C# keeps three bools (`Track_UDP_*`); one value is equivalent.
    pub udp: TrackUdp,
    /// `Track_UDP_WinTest_Port`.
    pub udp_wintest_port: u16,
    /// `Track_UDP_AirScout_Port`.
    pub udp_airscout_port: u16,
    /// File output. C# keeps three bools (`Track_File_*`); one value is equivalent.
    pub file: TrackFile,
    /// Serial rotator output. C# keeps three bools (`Track_Serial_*`); one value is equivalent.
    pub serial: TrackSerial,
    /// `Track_Serial_Port`.
    pub serial_port: String,
    /// `Track_Serial_Baudrate`.
    pub serial_baudrate: u32,
    /// DDE output. C# keeps two bools (`Track_DDE_*`); one value is equivalent.
    pub dde: TrackDde,
}

/// Serial track output (GS-232A rotator controller).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackSerial {
    #[default]
    None,
    Gs232Az,
    Gs232AzEl,
}

/// DDE track output (Ham Radio Deluxe).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackDde {
    #[default]
    None,
    Hrd,
}

/// UDP track output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackUdp {
    #[default]
    None,
    WinTest,
    AirScout,
}

/// File track output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackFile {
    #[default]
    None,
    Native,
    Wsjt,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            activate: false,
            set_az: 0.0,
            set_el: 0.0,
            offset_s: 0,
            update_ms: 1000,
            udp: TrackUdp::None,
            udp_wintest_port: 9871,
            udp_airscout_port: 9872,
            file: TrackFile::None,
            serial: TrackSerial::None,
            serial_port: "COM13".into(),
            serial_baudrate: 4800,
            dde: TrackDde::None,
        }
    }
}

/// Doppler strategy. C# keeps five bools and the first true of A..D wins; one value is equivalent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DopplerStrategy {
    #[default]
    None,
    A,
    B,
    C,
    D,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Doppler {
    /// Dial frequency [Hz].
    pub dial_freq_hz: i64,
    pub strategy: DopplerStrategy,
}
