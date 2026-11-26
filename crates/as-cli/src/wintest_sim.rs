//! `airscout-cli wintest-sim`: port of the C# WinTestSimulator (`WinTestSimulator/MainDlg.cs:22-41`,
//! settings defaults `Properties/Settings.settings:5-19`). It sends one Win-Test datagram.
//! With `--listen` it then lists the datagrams that arrive on the same port.

use crate::Res;
use as_wintest::{decode, encode, WtKind, WtMessage};
use serde_json::{json, Value};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// Listed datagrams at most; more are received but not listed.
pub const MAX_RECEIVED: usize = 1000;
/// One read waits at most this long, so the deadline is seen in time.
const STEP: Duration = Duration::from_millis(250);

#[derive(clap::Args)]
pub struct WintestSimArgs {
    /// Message: a WTMESSAGES name in any case, or its decimal value.
    #[arg(long, default_value = "ASADDWATCH")]
    pub msg: String,
    /// Source field of the message.
    #[arg(long, default_value = "Test")]
    pub from: String,
    /// Destination field of the message.
    #[arg(long, default_value = "AS")]
    pub to: String,
    /// Data field of the message.
    #[arg(long, default_value = "")]
    pub data: String,
    /// UDP port.
    #[arg(long, default_value_t = 9872)]
    pub port: u16,
    /// Destination address.
    #[arg(long, default_value = "255.255.255.255")]
    pub addr: Ipv4Addr,
    /// Local address to bind.
    #[arg(long, default_value = "0.0.0.0")]
    pub bind: Ipv4Addr,
    /// After the send, list the datagrams on <bind>:<port> for this many seconds (1 to 3600).
    #[arg(long)]
    pub listen: Option<u64>,
}

/// `MainDlg.cs:25`: `ToUpper().Trim()`, then the enum parse. `None` and `Unknown` cannot be sent.
pub fn message_of(a: &WintestSimArgs) -> Result<WtMessage, String> {
    let name = a.msg.trim().to_uppercase();
    let kind = WtKind::parse(&name);
    if matches!(kind, WtKind::None | WtKind::Unknown) {
        return Err(format!(
            "unknown Win-Test message: {name}; known: STATUS, PKTRCVD, SETAZIMUTH, SETELEVATION, \
             SETAZEL, ASADDWATCH, ASREMOVEWATCH, ASWATCHLIST, ASSETPATH, ASSHOWPATH, ASNEAREST"
        ));
    }
    Ok(WtMessage {
        kind,
        src: a.from.clone(),
        dst: a.to.clone(),
        data: a.data.clone(),
        extra: String::new(),
    })
}

fn entry(from: SocketAddr, bytes: &[u8]) -> Value {
    match decode(bytes) {
        Ok(d) => json!({
            "from": from.to_string(),
            "kind": d.msg.kind.wire_name(),
            "src": d.msg.src,
            "dst": d.msg.dst,
            "data": d.msg.data,
            "checksum_ok": d.checksum_ok,
        }),
        Err(e) => json!({"from": from.to_string(), "error": e.to_string(), "len": bytes.len()}),
    }
}

fn listen(sock: &UdpSocket, port: u16, sent: &[u8], secs: u64) -> Res<Vec<Value>> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; 65_536];
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(out);
        }
        sock.set_read_timeout(Some(left.min(STEP)))?;
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                let bytes = &buf[..n];
                // Own echo of the broadcast.
                if from.port() == port && bytes == sent {
                    continue;
                }
                if out.len() < MAX_RECEIVED {
                    out.push(entry(from, bytes));
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            // Windows reports an ICMP port-unreachable of an earlier send as a reset.
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
            Err(e) => return Err(e.into()),
        }
    }
}

pub fn run(a: &WintestSimArgs) -> Res<Value> {
    if a.port == 0 {
        return Err("port must be 1-65535".into());
    }
    if matches!(a.listen, Some(s) if !(1..=3600).contains(&s)) {
        return Err("--listen must be 1 to 3600 seconds".into());
    }
    let msg = message_of(a)?;
    let bytes = encode(&msg).ok_or("message cannot be encoded")?;
    let sock = match a.listen {
        None => UdpSocket::bind((a.bind, 0))?,
        Some(_) => as_wintest::socket::bind_shared(SocketAddr::from((a.bind, a.port)))?,
    };
    sock.set_broadcast(true)?;
    sock.send_to(&bytes, (a.addr, a.port))?;
    let (body, checksum) = bytes.split_at(bytes.len() - 1);
    let mut v = json!({"sent": {
        "to": format!("{}:{}", a.addr, a.port),
        "text": String::from_utf8_lossy(body),
        "checksum": checksum[0],
        "bytes": bytes.len(),
    }});
    if let Some(secs) = a.listen {
        v["received"] = Value::Array(listen(&sock, a.port, &bytes, secs)?);
    }
    Ok(v)
}
