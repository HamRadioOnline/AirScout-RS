//! GS-232A serial rotator output of `bw_Track` (`MapDlg.cs:7897-7962`, port setup `:8036-8047`).
//! Each step asks the controller for its position (`C` or `C2`), checks the reply and then sends
//! the set command (`M` or `W`) without waiting for an answer.
//!
//! Serial input is untrusted: a reply line is read byte by byte, at most [`MAX_REPLY`] bytes, and
//! never panics.

use crate::track_outputs::{OutputError, OutputGroup, TrackOutput};
use as_wintest::dotnet::{clamp_elevation, wrap_azimuth, zero3};
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Longest reply line accepted; a longer one is an error instead of an unbounded buffer.
pub const MAX_REPLY: usize = 256;

/// `ReadTimeout` and `WriteTimeout` of the C# port (`:8044-8045`).
pub const IO_TIMEOUT: Duration = Duration::from_millis(1000);

/// An open serial port, or a test double.
pub trait SerialIo: Read + Write + Send {}

impl<T: Read + Write + Send> SerialIo for T {}

/// Opens the port `name` at `baud`.
pub type SerialOpen = Arc<dyn Fn(&str, u32) -> io::Result<Box<dyn SerialIo>> + Send + Sync>;

/// The real opener (`:8036-8047`): 8 data bits, no parity, one stop bit, no handshake, 1 s
/// timeouts, DTR and RTS low (the .NET defaults). serial2 opens `COM10` and above through the
/// `\\.\` device namespace itself.
pub fn serial2_open() -> SerialOpen {
    Arc::new(|name, baud| {
        // a `u32` baud rate selects raw mode, 8N1 and no flow control
        let mut p = serial2::SerialPort::open(name, baud)?;
        p.set_read_timeout(IO_TIMEOUT)?;
        p.set_write_timeout(IO_TIMEOUT)?;
        p.set_dtr(false)?;
        p.set_rts(false)?;
        Ok(Box::new(p) as Box<dyn SerialIo>)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gs232 {
    Az,
    AzEl,
}

/// The position query (`:7918`, `:7943`).
pub fn query(k: Gs232) -> &'static str {
    match k {
        Gs232::Az => "C",
        Gs232::AzEl => "C2",
    }
}

/// The set command (`:7932`, `:7957`); `ToString("000")` is `zero3`. Round to a whole degree,
/// then wrap azimuth mod 360 and clamp elevation 0..=90 before formatting (`#236`), so a real
/// GS-232A rotator never receives `360` or a negative elevation (non-finite inputs become 0).
pub fn set_command(k: Gs232, az: f64, el: f64) -> String {
    match k {
        Gs232::Az => format!("M{}", zero3(wrap_azimuth(az))),
        Gs232::AzEl => format!(
            "W{} {}",
            zero3(wrap_azimuth(az)),
            zero3(clamp_elevation(el))
        ),
    }
}

/// `s.StartsWith("+0")` and `Convert.ToDouble(s.Substring(2, 3), Invariant)` succeeds.
pub fn reply_ok(s: &str) -> bool {
    if !s.starts_with("+0") {
        return false;
    }
    let field: String = s.chars().skip(2).take(3).collect();
    field.chars().count() == 3 && parse_dotnet_float(&field).is_some()
}

/// .NET white space: space and U+0009 to U+000D.
fn is_white(c: char) -> bool {
    c == ' ' || ('\t'..='\r').contains(&c)
}

/// `Convert.ToDouble(s, InvariantCulture)` = `NumberStyles.Float | AllowThousands`: white space
/// around, an optional sign, digits with `,` group separators after a digit in the integral part,
/// an optional `.` fraction, an optional exponent, at least one digit. The symbols `NaN` and
/// `Infinity` are accepted too, as in .NET. `None` is the `FormatException`.
pub fn parse_dotnet_float(s: &str) -> Option<f64> {
    let t = s.trim_matches(is_white);
    let (neg, body) = match t.as_bytes().first()? {
        b'+' => (false, &t[1..]),
        b'-' => (true, &t[1..]),
        _ => (false, t),
    };
    if body.eq_ignore_ascii_case("infinity") {
        return Some(if neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    if t.eq_ignore_ascii_case("nan") {
        return Some(f64::NAN);
    }
    let b = body.as_bytes();
    let mut i = 0;
    let mut clean = String::with_capacity(b.len());
    let mut digits = 0;
    while i < b.len() {
        match b[i] {
            c if c.is_ascii_digit() => {
                clean.push(c as char);
                digits += 1;
            }
            b',' if digits > 0 => {}
            _ => break,
        }
        i += 1;
    }
    if b.get(i) == Some(&b'.') {
        clean.push('.');
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            clean.push(b[i] as char);
            digits += 1;
            i += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        clean.push('e');
        i += 1;
        if let Some(&c @ (b'+' | b'-')) = b.get(i) {
            clean.push(c as char);
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            clean.push(b[i] as char);
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    let v: f64 = clean.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// `SerialPort.ReadLine` with `NewLine = "\r"`, then `Replace("\n", "")` (`:7907-7908`). Reads
/// byte by byte up to `\r` (not returned), drops `\n`, decodes bytes above 0x7F as `?` (ASCII
/// encoding). More than [`MAX_REPLY`] bytes give `InvalidData`; a read that times out after
/// `deadline` gives `TimedOut`.
pub fn read_reply(r: &mut dyn Read, deadline: Instant) -> io::Result<String> {
    let mut line = String::new();
    let mut seen = 0usize;
    let mut b = [0u8; 1];
    loop {
        match r.read(&mut b) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                if Instant::now() >= deadline {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(e) => return Err(e),
        }
        if Instant::now() >= deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        match b[0] {
            b'\r' => return Ok(line),
            c => {
                seen += 1;
                if seen > MAX_REPLY {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "reply too long"));
                }
                match c {
                    b'\n' => {}
                    0..=0x7F => line.push(c as char),
                    _ => line.push('?'),
                }
            }
        }
    }
}

/// Writes `cmd` and the `\r` line end (`WriteLine`, `NewLine = "\r"`).
fn write_line(w: &mut dyn Write, cmd: &str) -> io::Result<()> {
    w.write_all(format!("{cmd}\r").as_bytes())?;
    w.flush()
}

pub struct Gs232Output {
    kind: Gs232,
    port: String,
    baud: u32,
    open: SerialOpen,
    io: Option<Box<dyn SerialIo>>,
    /// Why the last open failed, shown by the next `send`.
    open_error: String,
}

impl Gs232Output {
    pub fn new(kind: Gs232, port: String, baud: u32, open: SerialOpen) -> Self {
        Self {
            kind,
            port,
            baud,
            open,
            io: None,
            open_error: String::new(),
        }
    }
}

impl TrackOutput for Gs232Output {
    fn group(&self) -> OutputGroup {
        OutputGroup::Serial
    }

    fn connect(&mut self) -> Result<(), String> {
        match (self.open)(&self.port, self.baud) {
            Ok(p) => {
                self.io = Some(p);
                Ok(())
            }
            Err(e) => {
                self.open_error = e.to_string();
                Err(self.open_error.clone())
            }
        }
    }

    fn is_connected(&self) -> bool {
        self.io.is_some()
    }

    fn send(&mut self, az: f64, el: f64) -> Result<(), OutputError> {
        let Some(io) = self.io.as_mut() else {
            // the C# outer catch (`:8318-8321`): shown, not counted
            return Err(OutputError::Failed(format!(
                "Track error: {}",
                self.open_error
            )));
        };
        // Serial timeouts and I/O errors (`WriteLine`/`ReadLine`) are counted toward the
        // switch-off (`#239`), unlike the open failure above (`#238`).
        let fail = |e: io::Error| OutputError::SerialIo(e.to_string());
        write_line(io.as_mut(), query(self.kind)).map_err(fail)?;
        let reply = read_reply(io.as_mut(), Instant::now() + IO_TIMEOUT).map_err(fail)?;
        if !reply_ok(&reply) {
            return Err(OutputError::NotConnected(
                "[Serial]: Wrong serial data format.".into(),
            ));
        }
        write_line(io.as_mut(), &set_command(self.kind, az, el)).map_err(fail)
    }

    fn close(&mut self) {
        self.io = None;
    }
}
