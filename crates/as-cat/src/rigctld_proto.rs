//! Pure parsing of rigctld replies. Every reply is untrusted input: nothing
//! here panics, and nothing allocates in proportion to a number in a reply.

use std::io::{BufRead, ErrorKind};

use crate::{BackendError, MAX_FREQ_HZ};

/// Longest accepted reply line in bytes, without the line end.
pub const MAX_LINE: usize = 256;

fn lost(t: &str) -> BackendError {
    BackendError::Lost(t.to_string())
}

/// Read one line. A trailing `\r` is stripped. More than `MAX_LINE` bytes,
/// non-UTF-8 and EOF give `Lost`; a read timeout gives `Timeout`.
pub fn read_line(r: &mut impl BufRead) -> Result<String, BackendError> {
    let mut line: Vec<u8> = Vec::new();
    loop {
        let (used, done) = {
            let buf = match r.fill_buf() {
                Ok(b) => b,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return Err(BackendError::Timeout)
                }
                Err(e) => return Err(BackendError::Lost(format!("rigctld read: {e}"))),
            };
            if buf.is_empty() {
                return Err(lost("rigctld closed the connection"));
            }
            match buf.iter().position(|&b| b == b'\n') {
                Some(i) => {
                    line.extend_from_slice(&buf[..i]);
                    (i + 1, true)
                }
                None => {
                    line.extend_from_slice(buf);
                    (buf.len(), false)
                }
            }
        };
        r.consume(used);
        if line.len() > MAX_LINE + 1 {
            return Err(lost("rigctld reply line too long"));
        }
        if done {
            break;
        }
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    if line.len() > MAX_LINE {
        return Err(lost("rigctld reply line too long"));
    }
    String::from_utf8(line).map_err(|_| lost("rigctld reply is not UTF-8"))
}

/// `"RPRT -11"` gives `Some(-11)`.
pub fn parse_rprt(line: &str) -> Option<i32> {
    line.strip_prefix("RPRT ")?.trim().parse().ok()
}

/// An integer or a decimal Hz value, rounded half away from zero. A value above
/// `MAX_FREQ_HZ` is a protocol error.
pub fn parse_freq(line: &str) -> Result<i64, BackendError> {
    let t = line.trim();
    let bad = || BackendError::Lost(format!("rigctld bad frequency {t:?}"));
    if let Ok(v) = t.parse::<i64>() {
        return if v <= MAX_FREQ_HZ { Ok(v) } else { Err(bad()) };
    }
    match t.parse::<f64>() {
        Ok(f) if f.is_finite() && f.round() <= MAX_FREQ_HZ as f64 => Ok(f.round() as i64),
        _ => Err(bad()),
    }
}

/// `"0"` or `"1"`.
pub fn parse_flag(line: &str) -> Result<bool, BackendError> {
    match line.trim() {
        "0" => Ok(false),
        "1" => Ok(true),
        t => Err(BackendError::Lost(format!("rigctld bad flag {t:?}"))),
    }
}
