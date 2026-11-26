//! A raw HTTP/1.1 client over `TcpStream`: the request bytes go out exactly as given (no
//! URL normalisation). Also included by `airscout` tests with `#[path]`, so each includer
//! leaves some items unused.
#![allow(dead_code)]
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// Every connect, read and write gives up after this long.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// Status, headers (as received, in order) and body of one response.
pub type Answer = (u16, Vec<(String, String)>, Vec<u8>);

/// One connection that can carry several requests (keep-alive).
pub struct Conn {
    stream: TcpStream,
    /// Bytes read past the end of the previous response.
    buf: Vec<u8>,
}

impl Conn {
    pub fn open(addr: SocketAddr) -> Conn {
        let stream = TcpStream::connect_timeout(&addr, TIMEOUT)
            .unwrap_or_else(|e| panic!("connect {addr}: {e}"));
        stream.set_read_timeout(Some(TIMEOUT)).unwrap();
        stream.set_write_timeout(Some(TIMEOUT)).unwrap();
        Conn {
            stream,
            buf: Vec::new(),
        }
    }

    /// Sends `raw` and reads one response: the head, then exactly `Content-Length` body
    /// bytes, or everything up to EOF when the header is missing.
    pub fn send(&mut self, raw: &[u8]) -> Answer {
        // The server may answer an oversized request and close before it read all of it.
        if let Err(e) = self.stream.write_all(raw) {
            assert!(
                matches!(
                    e.kind(),
                    ErrorKind::BrokenPipe
                        | ErrorKind::ConnectionReset
                        | ErrorKind::ConnectionAborted
                ),
                "write: {e}"
            );
        }
        let head_end = loop {
            if let Some(i) = find(&self.buf, b"\r\n\r\n") {
                break i;
            }
            assert!(self.fill() > 0, "connection closed before a response head");
        };
        let head = String::from_utf8(self.buf[..head_end].to_vec()).expect("head not UTF-8");
        self.buf.drain(..head_end + 4);
        let mut lines = head.split("\r\n");
        let status_line = lines.next().unwrap_or("");
        let status: u16 = status_line
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| panic!("bad status line {status_line:?}"));
        let headers: Vec<(String, String)> = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        let len = header(&headers, "content-length").map(|v| v.parse::<usize>().unwrap());
        match len {
            Some(n) => {
                while self.buf.len() < n {
                    assert!(self.fill() > 0, "connection closed inside the body");
                }
                let body = self.buf.drain(..n).collect();
                (status, headers, body)
            }
            None => {
                while self.fill() > 0 {}
                (status, headers, std::mem::take(&mut self.buf))
            }
        }
    }

    /// True when the server closed the connection (EOF or reset) within the read timeout.
    pub fn is_closed_by_peer(&mut self) -> bool {
        let mut b = [0u8; 64];
        loop {
            return match self.stream.read(&mut b) {
                Ok(0) => true,
                Ok(_) => false,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => is_closed(e.kind()),
            };
        }
    }

    /// Reads once into the buffer; 0 means EOF (a reset or abort counts as EOF).
    fn fill(&mut self) -> usize {
        let mut chunk = [0u8; 16 * 1024];
        loop {
            return match self.stream.read(&mut chunk) {
                Ok(n) => {
                    self.buf.extend_from_slice(&chunk[..n]);
                    n
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) if is_closed(e.kind()) => 0,
                Err(e) => panic!("read: {e}"),
            };
        }
    }
}

/// One request on a fresh connection.
pub fn http(addr: SocketAddr, raw_request: &[u8]) -> Answer {
    Conn::open(addr).send(raw_request)
}

/// `GET <target> HTTP/1.1` with a `Host` header, target bytes unchanged.
pub fn get(target: &[u8]) -> Vec<u8> {
    let mut r = b"GET ".to_vec();
    r.extend_from_slice(target);
    r.extend_from_slice(b" HTTP/1.1\r\nHost: test\r\n\r\n");
    r
}

/// The first value of a header, names compared ignoring case.
pub fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// A read error that means the peer closed the connection (Windows reports an abort).
fn is_closed(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
    )
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
