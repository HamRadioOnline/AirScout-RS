//! Hamlib net protocol client (`rigctld`) as a `Backend`.

use std::io::{self, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::backend::{Backend, BackendError};
use crate::rig::CatRig;
use crate::rigctld_proto::{parse_flag, parse_freq, parse_rprt, read_line};
use crate::RigIdent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RigctldConfig {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

/// Reads with the time left until a per-command deadline, so a server that
/// trickles bytes cannot stretch one command past the timeout.
struct Limited {
    stream: TcpStream,
    deadline: Instant,
}

impl Read for Limited {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left))?;
        self.stream.read(buf)
    }
}

pub struct Rigctld {
    config: RigctldConfig,
    conn: Option<BufReader<Limited>>,
}

pub fn rigctld_rig(c: RigctldConfig) -> CatRig<Rigctld> {
    CatRig::new(Rigctld::new(c))
}

impl Rigctld {
    pub fn new(config: RigctldConfig) -> Self {
        Self { config, conn: None }
    }

    pub fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    /// Send one command and read its first reply line. Timeout and Lost
    /// close the socket so a late reply cannot answer the next command.
    fn send(&mut self, cmd: &str) -> Result<String, BackendError> {
        let timeout = self.config.timeout;
        let Some(conn) = self.conn.as_mut() else {
            return Err(BackendError::Lost("not connected".into()));
        };
        let l = conn.get_mut();
        l.deadline = Instant::now() + timeout;
        let _ = l.stream.set_write_timeout(Some(timeout));
        let r = match l.stream.write_all(cmd.as_bytes()) {
            Ok(()) => read_line(conn),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Err(BackendError::Timeout)
            }
            Err(e) => Err(BackendError::Lost(format!("rigctld write: {e}"))),
        };
        self.fail_closes(r)
    }

    fn next_line(&mut self) -> Result<String, BackendError> {
        let Some(conn) = self.conn.as_mut() else {
            return Err(BackendError::Lost("not connected".into()));
        };
        let r = read_line(conn);
        self.fail_closes(r)
    }

    fn fail_closes<T>(&mut self, r: Result<T, BackendError>) -> Result<T, BackendError> {
        if matches!(r, Err(BackendError::Timeout | BackendError::Lost(_))) {
            self.conn = None;
        }
        r
    }

    fn lost<T>(&mut self, t: &str) -> Result<T, BackendError> {
        self.conn = None;
        Err(BackendError::Lost(t.to_string()))
    }

    /// Set command: only `RPRT 0` is success.
    fn set(&mut self, cmd: String) -> Result<(), BackendError> {
        let line = self.send(&cmd)?;
        match parse_rprt(&line) {
            Some(0) => Ok(()),
            Some(n) => Err(BackendError::Rejected(n)),
            None => self.lost("rigctld bad reply to a set command"),
        }
    }

    /// Get command: first line, with `RPRT n` handled.
    fn get(&mut self, cmd: &str) -> Result<String, BackendError> {
        let line = self.send(cmd)?;
        match parse_rprt(&line) {
            Some(0) => self.lost("rigctld answered a get command with RPRT 0"),
            Some(n) => Err(BackendError::Rejected(n)),
            None => Ok(line),
        }
    }

    fn parsed<T>(&mut self, r: Result<T, BackendError>) -> Result<T, BackendError> {
        self.fail_closes(r)
    }
}

impl Backend for Rigctld {
    fn ident(&self) -> RigIdent {
        RigIdent {
            cat_version: "Hamlib rigctld (net protocol)".into(),
            rig_type: format!("[rigctld] {}:{}", self.config.host, self.config.port),
        }
    }

    fn connect(&mut self) -> Result<(), BackendError> {
        if self.conn.is_some() {
            return Ok(());
        }
        let RigctldConfig {
            host,
            port,
            timeout,
        } = self.config.clone();
        let fail =
            |e: &dyn std::fmt::Display| BackendError::NoCat(format!("rigctld {host}:{port}: {e}"));
        let addrs = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| fail(&e))?;
        // One deadline for all resolved addresses (localhost may give ::1 and
        // 127.0.0.1), so a connect never takes longer than `timeout`. Name
        // resolution itself is not bounded by it.
        let deadline = Instant::now() + timeout;
        let mut last: Option<io::Error> = None;
        for a in addrs {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match TcpStream::connect_timeout(&a, left) {
                Ok(s) => {
                    let _ = s.set_nodelay(true);
                    let _ = s.set_read_timeout(Some(timeout));
                    let _ = s.set_write_timeout(Some(timeout));
                    self.conn = Some(BufReader::new(Limited {
                        stream: s,
                        deadline: Instant::now(),
                    }));
                    return Ok(());
                }
                Err(e) => last = Some(e),
            }
        }
        Err(match last {
            Some(e) => fail(&e),
            None if Instant::now() >= deadline => fail(&"connect timed out"),
            None => fail(&"no address"),
        })
    }

    fn close(&mut self) {
        self.conn = None;
    }

    fn get_freq(&mut self) -> Result<i64, BackendError> {
        let l = self.get("f\n")?;
        let r = parse_freq(&l);
        self.parsed(r)
    }

    fn set_freq(&mut self, hz: i64) -> Result<(), BackendError> {
        self.set(format!("F {hz}\n"))
    }

    fn get_mode(&mut self) -> Result<String, BackendError> {
        let mode = self.get("m\n")?;
        // The passband line is read and ignored.
        self.next_line()?;
        Ok(mode)
    }

    fn set_mode(&mut self, mode: &str) -> Result<(), BackendError> {
        self.set(format!("M {mode} -1\n"))
    }

    fn get_split(&mut self) -> Result<bool, BackendError> {
        let l = self.get("s\n")?;
        let r = parse_flag(&l);
        let on = self.parsed(r)?;
        // TX VFO line is read and ignored.
        self.next_line()?;
        Ok(on)
    }

    fn set_split(&mut self, on: bool) -> Result<(), BackendError> {
        self.set(if on { "S 1 VFOB\n" } else { "S 0 VFOA\n" }.to_string())
    }

    fn get_split_freq(&mut self) -> Result<i64, BackendError> {
        let l = self.get("i\n")?;
        let r = parse_freq(&l);
        self.parsed(r)
    }

    fn set_split_freq(&mut self, hz: i64) -> Result<(), BackendError> {
        self.set(format!("I {hz}\n"))
    }

    fn set_split_mode(&mut self, mode: &str) -> Result<(), BackendError> {
        self.set(format!("X {mode} -1\n"))
    }

    fn get_rit_on(&mut self) -> Result<bool, BackendError> {
        let l = self.get("u RIT\n")?;
        let r = parse_flag(&l);
        self.parsed(r)
    }

    fn set_rit_on(&mut self, on: bool) -> Result<(), BackendError> {
        self.set(format!("U RIT {}\n", u8::from(on)))
    }
}
