//! `airscout-planeserver [--headless] [--config <path>] [--bind <ip>] [--port <n>]
//! [--data <dir>]`: the daemon (`--headless`), or the window with a tray icon.
use airscout_planeserver::config::{default_path, PlaneServerConfig};
use airscout_planeserver::service::{make_feeds, Service, Status};
use airscout_planeserver::start_lines;
use clap::Parser;
use std::io::Write;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// AirScout PlaneServer: collects plane feeds and serves /planes.json.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Run as a daemon without a window.
    #[arg(long)]
    headless: bool,
    /// Config file (default: the config dir of the OS).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Listen on this address for this run only (not saved).
    #[arg(long, value_name = "IP")]
    bind: Option<IpAddr>,
    /// Listen on this port for this run only (not saved).
    #[arg(long, value_name = "N")]
    port: Option<u16>,
    /// Data directory for this run only (not saved).
    #[arg(long, value_name = "DIR")]
    data: Option<PathBuf>,
    /// Stop headless mode after this many seconds (for tests).
    #[arg(long, hide = true, value_name = "S", value_parser = clap::value_parser!(u64).range(1..=3600))]
    run_for: Option<u64>,
}

/// Polls for stop requests and status changes at this rate.
const TICK: Duration = Duration::from_millis(100);
/// Status is compared once a second.
const STATUS_EVERY: Duration = Duration::from_secs(1);
/// How long the start lines wait for the server to bind.
const BIND_WAIT: Duration = Duration::from_secs(2);

fn main() -> ExitCode {
    let cli = Cli::parse();
    let path = cli.config.clone().unwrap_or_else(default_path);
    let mut cfg = match PlaneServerConfig::load_or_default(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("airscout-planeserver: {e}");
            return ExitCode::from(2);
        }
    };
    for w in cfg.validate() {
        eprintln!("warning: {w}");
    }
    // Overrides come after `validate` and are never saved.
    if let Some(b) = cli.bind {
        cfg.bind = b;
    }
    if let Some(p) = cli.port {
        cfg.port = p;
    }
    if let Some(d) = cli.data {
        cfg.data_dir = Some(d);
    }
    if !cli.headless {
        return match airscout_planeserver::gui::run(cfg, path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("airscout-planeserver: {e}");
                ExitCode::from(1)
            }
        };
    }
    run_headless(&cfg, &path, cli.run_for)
}

/// The status as printable lines, one per item; compared between ticks. Names and state or
/// error texts only: no feed settings, no URLs.
fn status_lines(s: &Status) -> Vec<String> {
    let mut v: Vec<String> = s
        .feeds
        .iter()
        .map(|f| match &f.last_error {
            Some(e) => format!("feed {}: {} ({e})", f.name, f.state),
            None => format!("feed {}: {}", f.name, f.state),
        })
        .collect();
    v.push(format!("server: {}", s.server));
    if let Some(e) = &s.db_error {
        v.push(format!("database: {e}"));
    }
    v
}

fn run_headless(cfg: &PlaneServerConfig, path: &std::path::Path, run_for: Option<u64>) -> ExitCode {
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        if let Err(e) = ctrlc::set_handler(move || stop.store(true, Ordering::SeqCst)) {
            eprintln!("airscout-planeserver: cannot install the signal handler: {e}");
            return ExitCode::from(1);
        }
    }
    let svc = match Service::start(cfg, make_feeds(cfg)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("airscout-planeserver: {e}");
            return ExitCode::from(1);
        }
    };
    // The server binds on its own thread: wait a moment for the real address.
    let wait_end = Instant::now() + BIND_WAIT;
    let mut bound = svc.local_addr();
    while cfg.server_active && bound.is_none() && Instant::now() < wait_end {
        std::thread::sleep(Duration::from_millis(10));
        bound = svc.local_addr();
    }
    if cfg.server_active {
        for l in start_lines(cfg, bound, &svc.status().server) {
            eprintln!("{l}");
        }
    }
    if cfg.feeds.iter().all(|f| f.is_empty() || f == "[none]") {
        eprintln!(
            "no feed configured; edit {} or start without --headless",
            path.display()
        );
    }
    let mut shown: Vec<String> = Vec::new();
    let mut print_changes = |svc: &Service| {
        let now = status_lines(&svc.status());
        for l in now.iter().filter(|l| !shown.contains(l)) {
            eprintln!("{l}");
        }
        shown = now;
    };
    print_changes(&svc);
    let _ = std::io::stderr().flush();

    let deadline = run_for.map(|s| Instant::now() + Duration::from_secs(s));
    let mut next_status = Instant::now() + STATUS_EVERY;
    while !stop.load(Ordering::SeqCst) && deadline.is_none_or(|d| Instant::now() < d) {
        std::thread::sleep(TICK);
        if Instant::now() >= next_status {
            next_status += STATUS_EVERY;
            print_changes(&svc);
        }
    }
    svc.stop();
    ExitCode::SUCCESS
}
