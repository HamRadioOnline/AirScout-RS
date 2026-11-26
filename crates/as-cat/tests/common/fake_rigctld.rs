//! In-process fake rigctld on 127.0.0.1 (port 0). Shared by test crates via
//! `#[path]`, so each includer leaves some items unused.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct RigState {
    pub freq: i64,
    pub split_freq: i64,
    pub mode: String,
    pub split: bool,
    pub rit: bool,
}

impl Default for RigState {
    fn default() -> Self {
        Self {
            freq: 144_300_000,
            split_freq: 144_300_000,
            mode: "USB".into(),
            split: false,
            rit: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Behaviour {
    Normal,
    /// Read commands, never answer.
    Silent,
    /// Read one command, then close.
    CloseAfterRead,
    /// Answer every command with these bytes.
    Fixed(Vec<u8>),
}

struct Inner {
    state: RigState,
    behaviour: Behaviour,
    lines: Vec<String>,
    raw: Vec<u8>,
    accepts: usize,
}

pub struct FakeRigctld {
    pub addr: SocketAddr,
    inner: Arc<Mutex<Inner>>,
    stop: Arc<AtomicBool>,
    accept: Option<thread::JoinHandle<()>>,
}

fn lock(m: &Mutex<Inner>) -> std::sync::MutexGuard<'_, Inner> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl FakeRigctld {
    pub fn start(state: RigState) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        listener.set_nonblocking(true).expect("nonblocking");
        let inner = Arc::new(Mutex::new(Inner {
            state,
            behaviour: Behaviour::Normal,
            lines: Vec::new(),
            raw: Vec::new(),
            accepts: 0,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (i2, s2) = (inner.clone(), stop.clone());
        let accept = thread::spawn(move || {
            let mut conns = Vec::new();
            while !s2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((s, _)) => {
                        lock(&i2).accepts += 1;
                        let (i3, s3) = (i2.clone(), s2.clone());
                        conns.push(thread::spawn(move || serve(s, i3, s3)));
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
            for c in conns {
                let _ = c.join();
            }
        });
        Self {
            addr,
            inner,
            stop,
            accept: Some(accept),
        }
    }

    pub fn set_behaviour(&self, b: Behaviour) {
        lock(&self.inner).behaviour = b;
    }

    pub fn lines(&self) -> Vec<String> {
        lock(&self.inner).lines.clone()
    }

    pub fn raw(&self) -> Vec<u8> {
        lock(&self.inner).raw.clone()
    }

    pub fn clear(&self) {
        let mut g = lock(&self.inner);
        g.lines.clear();
        g.raw.clear();
    }

    pub fn accepts(&self) -> usize {
        lock(&self.inner).accepts
    }

    pub fn state(&self) -> RigState {
        lock(&self.inner).state.clone()
    }

    /// Returns early once the line has arrived.
    pub fn wait_for_line(&self, l: &str, max: Duration) -> bool {
        let end = Instant::now() + max;
        loop {
            if self.lines().iter().any(|x| x == l) {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for FakeRigctld {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
    }
}

fn serve(stream: TcpStream, inner: Arc<Mutex<Inner>>, stop: Arc<AtomicBool>) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    let mut rd = BufReader::new(stream);
    let mut buf: Vec<u8> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        match rd.read_until(b'\n', &mut buf) {
            Ok(0) => return,
            Ok(_) if buf.last() == Some(&b'\n') => {}
            Ok(_) => continue,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(_) => return,
        }
        let line = String::from_utf8_lossy(&buf[..buf.len() - 1]).into_owned();
        let reply = {
            let mut g = lock(&inner);
            g.raw.extend_from_slice(&buf);
            g.lines.push(line.clone());
            match g.behaviour.clone() {
                Behaviour::Normal => answer(&mut g.state, &line),
                Behaviour::Silent => Vec::new(),
                Behaviour::CloseAfterRead => return,
                Behaviour::Fixed(b) => b,
            }
        };
        buf.clear();
        if !reply.is_empty() && out.write_all(&reply).is_err() {
            return;
        }
    }
}

fn answer(s: &mut RigState, line: &str) -> Vec<u8> {
    let ok = b"RPRT 0\n".to_vec();
    let bad = b"RPRT -11\n".to_vec();
    let parts: Vec<&str> = line.split(' ').collect();
    match parts.as_slice() {
        ["f"] => format!("{}\n", s.freq).into_bytes(),
        ["F", h] => match h.parse() {
            Ok(v) => {
                s.freq = v;
                ok
            }
            Err(_) => bad,
        },
        ["m"] => format!("{}\n2400\n", s.mode).into_bytes(),
        ["M", m, _] => {
            s.mode = (*m).to_string();
            ok
        }
        ["s"] => format!(
            "{}\n{}\n",
            u8::from(s.split),
            if s.split { "VFOB" } else { "VFOA" }
        )
        .into_bytes(),
        ["S", f, _] => {
            s.split = *f == "1";
            ok
        }
        ["i"] => format!("{}\n", s.split_freq).into_bytes(),
        ["I", h] => match h.parse() {
            Ok(v) => {
                s.split_freq = v;
                ok
            }
            Err(_) => bad,
        },
        ["X", _, _] => ok,
        ["u", "RIT"] => format!("{}\n", u8::from(s.rit)).into_bytes(),
        ["U", "RIT", v] => {
            s.rit = *v == "1";
            ok
        }
        _ => bad,
    }
}
