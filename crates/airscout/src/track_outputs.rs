//! Track outputs of `bw_Track` (`MapDlg.cs:7839-7990`, `:8156-8197`): DDE to Ham Radio Deluxe
//! (Windows), UDP (Win-Test or AirScout format), the GS-232A serial rotator and the `azel.dat` file.

use crate::net_addrs::NetAddrs;
use crate::rotator_gs232::{serial2_open, Gs232, Gs232Output, SerialOpen};
use as_dde::{DdeLink, CF_TEXT, ITEM, POKE_TIMEOUT, SERVICE, TOPIC};
use as_settings::{Settings, TrackDde, TrackFile, TrackSerial, TrackUdp};
use as_wintest::dotnet::fixed;
use as_wintest::encode;
use as_wintest::track::{set_azel, set_azimuth, set_elevation};
use chrono::{DateTime, Utc};
use std::fs::File;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};

/// Output group; the error counters and the switch-off work per group. Declared in the upstream
/// order (`:8156-8197`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputGroup {
    Dde,
    Udp,
    Serial,
    File,
}

/// Why a send failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputError {
    /// The `"[UDP]:"`-prefixed class the C# counts towards the switch-off.
    NotConnected(String),
    /// Every other exception: shown, not counted; the output reconnects on the next step.
    Failed(String),
}

pub trait TrackOutput: Send {
    fn group(&self) -> OutputGroup;
    fn connect(&mut self) -> Result<(), String>;
    fn is_connected(&self) -> bool;
    fn send(&mut self, az: f64, el: f64) -> Result<(), OutputError>;
    fn close(&mut self);
    /// Called on every pass of the Track worker, at least every 250 ms, also without a step; an
    /// output that owns thread-bound windows services them here. Default: nothing.
    fn idle(&mut self) {}
}

/// `"\r\n"` on Windows, `"\n"` elsewhere (`StreamWriter.WriteLine`, `Environment.NewLine`).
pub const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// The pokes of one step to HRD Rotator (`Track_DDE_HRD`, `MapDlg.cs:7839-7861`):
/// `SET-AZ:<az F1>` when `0 <= az <= 360`, then `SET-EL:<el F1>` when `0 <= el <= 90`. The C#
/// formats with the current culture; the port always writes `.`.
pub fn hrd_pokes(az: f64, el: f64) -> Vec<Vec<u8>> {
    let mut v = Vec::new();
    if (0.0..=360.0).contains(&az) {
        v.push(format!("SET-AZ:{}", fixed(az, 1)).into_bytes());
    }
    if (0.0..=90.0).contains(&el) {
        v.push(format!("SET-EL:{}", fixed(el, 1)).into_bytes());
    }
    v
}

/// The HRD rotator output over a [`DdeLink`] (`Track_DDE_HRD`, `MapDlg.cs:7839-7861`); no position
/// feedback. The conversation lives on the Track worker's thread only.
///
/// Every poke is synchronous and may block for [`POKE_TIMEOUT`] (10 s, 1:1), so one DDE step is
/// bounded by 2 × `POKE_TIMEOUT`. The 250 ms cancel of the Track worker holds only outside such an
/// exchange: a worker that is still blocked after an Options restart is detached, and its events
/// are dropped by their generation. `DdeConnect` has no timeout at all: it runs on every unconnected
/// step and asks every top-level window, so a hung window can block it longer (as the C#
/// `TryConnect`, 1:1).
pub struct DdeOutput {
    link: Box<dyn DdeLink>,
    /// A conversation was opened and not yet torn down.
    linked: bool,
}

impl DdeOutput {
    pub fn new(link: Box<dyn DdeLink>) -> Self {
        Self {
            link,
            linked: false,
        }
    }
}

impl TrackOutput for DdeOutput {
    fn group(&self) -> OutputGroup {
        OutputGroup::Dde
    }

    /// `new DdeClient("HRDRotator", "Position").TryConnect()` (`MapDlg.cs:8018-8023`).
    fn connect(&mut self) -> Result<(), String> {
        let r = self.link.connect(SERVICE, TOPIC);
        self.linked = r.is_ok();
        r
    }

    fn is_connected(&self) -> bool {
        self.link.is_connected()
    }

    fn send(&mut self, az: f64, el: f64) -> Result<(), OutputError> {
        if !self.link.is_connected() {
            // the `"[DDE]:"` class counts towards the switch-off (`:7845-7846`, `:8204-8216`)
            return Err(OutputError::NotConnected(
                "[DDE]: Client not connected.".into(),
            ));
        }
        for d in hrd_pokes(az, el) {
            // `TryPoke` returns an error code the C# ignores (1:1)
            let _ = self.link.poke(ITEM, &d, CF_TEXT, POKE_TIMEOUT);
        }
        Ok(())
    }

    /// `ddeclient.Disconnect()` (`MapDlg.cs:8330-8331`).
    fn close(&mut self) {
        self.link.disconnect();
        self.linked = false;
    }

    /// Asks the link for its state: `Ddeml` then drains the thread's queue, so the hidden DDEML
    /// windows answer sent messages also while tracking is idle (NDde pumps on its own thread,
    /// `DdeContext.cs:629-757`). A conversation the server has ended is torn down here at once,
    /// instance included, so no DDE window is left that nothing pumps; nothing is counted on this
    /// path, and the next valid step reconnects (or counts) as usual.
    fn idle(&mut self) {
        if !self.link.is_connected() && self.linked {
            self.link.disconnect();
            self.linked = false;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UdpFormat {
    WinTest,
    AirScout,
}

/// The datagrams of one step (`:7877-7895`): Win-Test sends SETAZIMUTH, AirScout sends
/// SETAZIMUTH, SETELEVATION and SETAZEL in this order.
pub fn udp_datagrams(f: UdpFormat, server: &str, az: f64, el: f64) -> Vec<Vec<u8>> {
    let msgs = match f {
        UdpFormat::WinTest => vec![set_azimuth(server, az)],
        UdpFormat::AirScout => vec![
            set_azimuth(server, az),
            set_elevation(server, el),
            set_azel(server, az, el),
        ],
    };
    msgs.iter().filter_map(encode).collect()
}

pub struct UdpOutput {
    format: UdpFormat,
    server: String,
    bind_ip: Ipv4Addr,
    target: SocketAddr,
    socket: Option<UdpSocket>,
}

impl UdpOutput {
    pub fn new(f: UdpFormat, server: &str, bind_ip: Ipv4Addr, target: SocketAddr) -> Self {
        Self {
            format: f,
            server: server.into(),
            bind_ip,
            target,
            socket: None,
        }
    }
}

impl TrackOutput for UdpOutput {
    fn group(&self) -> OutputGroup {
        OutputGroup::Udp
    }

    fn connect(&mut self) -> Result<(), String> {
        let s = UdpSocket::bind((self.bind_ip, 0)).map_err(|e| e.to_string())?;
        s.set_broadcast(true).map_err(|e| e.to_string())?;
        self.socket = Some(s);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.socket.is_some()
    }

    fn send(&mut self, az: f64, el: f64) -> Result<(), OutputError> {
        let Some(s) = &self.socket else {
            return Err(OutputError::NotConnected(
                "[UDP]: Client and/or IP endpoint not initialized.".into(),
            ));
        };
        for d in udp_datagrams(self.format, &self.server, az, el) {
            s.send_to(&d, self.target)
                .map_err(|e| OutputError::Failed(e.to_string()))?;
        }
        Ok(())
    }

    fn close(&mut self) {
        self.socket = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileFormat {
    Native,
    Wsjt,
}

/// `<az F1>,<el F1>` and a newline (`:7966-7972`).
pub fn native_azel(az: f64, el: f64) -> String {
    format!("{},{}{NEWLINE}", fixed(az, 1), fixed(el, 1))
}

/// Four lines for WSJT-X (`:7974-7986`).
pub fn wsjt_azel(utc: DateTime<Utc>, az: f64, el: f64) -> String {
    let t = utc.format("%H:%M:%S");
    format!(
        "{t},0,0,Moon{NEWLINE}{t},0,0,Sun{NEWLINE}{t},{},{},Source{NEWLINE}0,  0, 0,  0, 0,Doppler, R{NEWLINE}",
        fixed(az, 1),
        fixed(el, 1)
    )
}

/// Rewrites the file in place every step.
pub struct FileOutput {
    format: FileFormat,
    path: PathBuf,
    ready: bool,
}

impl FileOutput {
    pub fn new(f: FileFormat, path: PathBuf) -> Self {
        Self {
            format: f,
            path,
            ready: false,
        }
    }
}

impl TrackOutput for FileOutput {
    fn group(&self) -> OutputGroup {
        OutputGroup::File
    }

    fn connect(&mut self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        self.ready = true;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.ready
    }

    fn send(&mut self, az: f64, el: f64) -> Result<(), OutputError> {
        let text = match self.format {
            FileFormat::Native => native_azel(az, el),
            FileFormat::Wsjt => wsjt_azel(Utc::now(), az, el),
        };
        File::create(&self.path)
            .and_then(|mut f| f.write_all(text.as_bytes()))
            .map_err(|e| OutputError::Failed(e.to_string()))
    }

    fn close(&mut self) {
        self.ready = false;
    }
}

/// What the Track worker needs from the settings.
pub struct TrackConfig {
    pub server_name: String,
    pub udp: TrackUdp,
    pub udp_wintest_port: u16,
    pub udp_airscout_port: u16,
    pub file: TrackFile,
    pub serial: TrackSerial,
    pub serial_port: String,
    pub serial_baudrate: u32,
    /// Opens the serial port; the real one, or a test double.
    pub serial_open: SerialOpen,
    pub dde: TrackDde,
    pub net: NetAddrs,
    pub azel_path: PathBuf,
}

impl TrackConfig {
    /// `azel_path` is `<data dir>/Tmp/azel.dat`.
    pub fn from_settings(s: &Settings, data_dir: &Path, net: NetAddrs) -> Self {
        Self {
            server_name: s.server.name.clone(),
            udp: s.track.udp,
            udp_wintest_port: s.track.udp_wintest_port,
            udp_airscout_port: s.track.udp_airscout_port,
            file: s.track.file,
            serial: s.track.serial,
            serial_port: s.track.serial_port.clone(),
            serial_baudrate: s.track.serial_baudrate,
            serial_open: serial2_open(),
            dde: s.track.dde,
            net,
            azel_path: data_dir.join("Tmp").join("azel.dat"),
        }
    }
}

/// The enabled outputs in the upstream order (`:8156-8197`).
pub fn build_outputs(c: &TrackConfig) -> Vec<Box<dyn TrackOutput>> {
    let mut v: Vec<Box<dyn TrackOutput>> = Vec::new();
    // DDE goes first (`:8162-8166`); it exists on Windows only
    #[cfg(windows)]
    if c.dde == TrackDde::Hrd {
        v.push(Box::new(DdeOutput::new(Box::new(as_dde::Ddeml::new()))));
    }
    let udp = match c.udp {
        TrackUdp::None => None,
        TrackUdp::WinTest => Some((UdpFormat::WinTest, c.udp_wintest_port)),
        TrackUdp::AirScout => Some((UdpFormat::AirScout, c.udp_airscout_port)),
    };
    if let Some((f, port)) = udp {
        let target = SocketAddr::from((c.net.broadcast_ip, port));
        v.push(Box::new(UdpOutput::new(
            f,
            &c.server_name,
            c.net.bind_ip,
            target,
        )));
    }
    let serial = match c.serial {
        TrackSerial::None => None,
        TrackSerial::Gs232Az => Some(Gs232::Az),
        TrackSerial::Gs232AzEl => Some(Gs232::AzEl),
    };
    if let Some(k) = serial {
        v.push(Box::new(Gs232Output::new(
            k,
            c.serial_port.clone(),
            c.serial_baudrate,
            c.serial_open.clone(),
        )));
    }
    let file = match c.file {
        TrackFile::None => None,
        TrackFile::Native => Some(FileFormat::Native),
        TrackFile::Wsjt => Some(FileFormat::Wsjt),
    };
    if let Some(f) = file {
        v.push(Box::new(FileOutput::new(f, c.azel_path.clone())));
    }
    v
}
