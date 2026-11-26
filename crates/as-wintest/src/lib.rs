//! Win-Test UDP wire format (`WinTest/WinTest.cs`), ported 1:1.
#![forbid(unsafe_code)]

pub mod dotnet;
pub mod socket;
pub mod track;

/// Message kind; values and names match the C# enum.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WtKind {
    None = 0,
    Status = 1,
    PktRcvd = 2,
    SetAzimuth = 100,
    SetElevation = 101,
    SetAzEl = 102,
    AsAddWatch = 247,
    AsRemoveWatch = 248,
    AsWatchlist = 249,
    AsSetPath = 252,
    AsShowPath = 253,
    AsNearest = 254,
    Unknown = 255,
}

const KINDS: [WtKind; 13] = [
    WtKind::None,
    WtKind::Status,
    WtKind::PktRcvd,
    WtKind::SetAzimuth,
    WtKind::SetElevation,
    WtKind::SetAzEl,
    WtKind::AsAddWatch,
    WtKind::AsRemoveWatch,
    WtKind::AsWatchlist,
    WtKind::AsSetPath,
    WtKind::AsShowPath,
    WtKind::AsNearest,
    WtKind::Unknown,
];

impl WtKind {
    /// The C# enum member name.
    pub fn wire_name(self) -> &'static str {
        match self {
            WtKind::None => "NONE",
            WtKind::Status => "STATUS",
            WtKind::PktRcvd => "PKTRCVD",
            WtKind::SetAzimuth => "SETAZIMUTH",
            WtKind::SetElevation => "SETELEVATION",
            WtKind::SetAzEl => "SETAZEL",
            WtKind::AsAddWatch => "ASADDWATCH",
            WtKind::AsRemoveWatch => "ASREMOVEWATCH",
            WtKind::AsWatchlist => "ASWATCHLIST",
            WtKind::AsSetPath => "ASSETPATH",
            WtKind::AsShowPath => "ASSHOWPATH",
            WtKind::AsNearest => "ASNEAREST",
            WtKind::Unknown => "UNKNOWN",
        }
    }

    /// Like `Enum.Parse` (`WinTest.cs:73-80`): trimmed, case-sensitive name
    /// or a defined decimal value; anything else is `Unknown`.
    pub fn parse(name: &str) -> WtKind {
        let name = name.trim();
        if let Some(k) = KINDS.iter().find(|k| k.wire_name() == name) {
            return *k;
        }
        match name.parse::<i64>() {
            Ok(n) => KINDS
                .iter()
                .find(|k| i64::from(**k as u8) == n)
                .copied()
                .unwrap_or(WtKind::Unknown),
            Err(_) => WtKind::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WtMessage {
    pub kind: WtKind,
    pub src: String,
    pub dst: String,
    pub data: String,
    pub extra: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub msg: WtMessage,
    pub checksum: u8,
    pub checksum_ok: bool,
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum DecodeError {
    #[error("empty datagram")]
    Empty,
    #[error("no name separator")]
    NoNameSeparator,
    #[error("missing field")]
    MissingField,
}

/// Wrapping byte sum with bit 7 set.
pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)) | 0x80
}

/// `ToBytes` (`WinTest.cs:105-148`). `None` and `Unknown` cannot be sent.
pub fn encode(m: &WtMessage) -> Option<Vec<u8>> {
    if matches!(m.kind, WtKind::None | WtKind::Unknown) {
        return None;
    }
    let tail = if m.kind == WtKind::AsNearest {
        "?"
    } else {
        "??"
    };
    let text = format!(
        "{}: \"{}\" \"{}\" \"{}\"{}{}",
        m.kind.wire_name(),
        m.src,
        m.dst,
        m.data,
        m.extra,
        tail
    );
    let mut b: Vec<u8> = text
        .chars()
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect();
    let last = b.len() - 1;
    b[last] = checksum(&b[..last]);
    Some(b)
}

/// `FromBytes` (`WinTest.cs:69-103`). The checksum is reported, never enforced.
pub fn decode(bytes: &[u8]) -> Result<Decoded, DecodeError> {
    let Some(&last) = bytes.last() else {
        return Err(DecodeError::Empty);
    };
    let text: String = bytes
        .iter()
        .map(|&b| if b < 0x80 { char::from(b) } else { '?' })
        .collect();
    let (name, rest) = text.split_once(": ").ok_or(DecodeError::NoNameSeparator)?;
    let (src, rest) = rest.split_once(' ').ok_or(DecodeError::MissingField)?;
    let (dst, rest) = rest.split_once(' ').ok_or(DecodeError::MissingField)?;
    let mut chars = rest.chars();
    if chars.next_back().is_none() {
        return Err(DecodeError::Empty);
    }
    let data: String = chars.filter(|&c| c != '"').collect();
    let data = data.trim_end_matches('?').to_string();
    // `Checksum` is reported as `last | 0x80` (as `WinTest.cs:98`), but `HasChecksum` must
    // compare the raw trailing byte (`WinTest.cs:99`): masking bit 7 back on would accept a
    // checksum byte corrupted only in bit 7.
    let cs = last | 0x80;
    Ok(Decoded {
        msg: WtMessage {
            kind: WtKind::parse(name),
            src: src.replace('"', ""),
            dst: dst.replace('"', ""),
            data,
            extra: String::new(),
        },
        checksum: cs,
        checksum_ok: last == checksum(&bytes[..bytes.len() - 1]),
    })
}
