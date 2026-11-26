//! `catcheck`: CAT test bench over `as-cat` (the C# `CATCheck` tool, as a command line).
//!
//! It reads the rig state, sets frequency, mode, split and RIT (`CATCheck/MainDlg.cs:688-699`,
//! `:817-845`), lists the serial ports (`:391-398`) and runs a short Doppler test that restores
//! the rig. It has no INI engine and writes no log, so the log folder button (`:676-686`, which
//! starts `explorer.exe`) has no counterpart.
//!
//! Privacy: from the settings file only the rig ident is printed (`[rigctld] host:port`,
//! `[OmniRig] Rig n`, the hamlib model). A settings parse error names the path, the line and
//! the column, never the text.
#![forbid(unsafe_code)]

use as_cat::from_settings::cat_config;
use as_cat::{
    factory, hamlib_available, make_rig, omnirig_available, CatConfig, CatEvent, CatWorker,
    Doppler, DopplerTarget, Engine, Rig, RigInfo, RigMode, RigRit, RigSplit, RigStatus,
    MAX_FREQ_HZ,
};
use as_settings::{CatEngine, Settings};
use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// Longest sleep step (the waits are cut so a Ctrl-C ends them quickly).
const STEP: Duration = Duration::from_millis(250);

#[derive(Parser)]
#[command(
    name = "catcheck",
    version,
    about = "AirScout-RS CAT test bench: rig status, set commands and a Doppler test"
)]
struct Cli {
    #[command(flatten)]
    flags: Flags,
    #[command(subcommand)]
    cmd: Cmd,
}

/// Global flags; they override the settings file.
#[derive(Args)]
struct Flags {
    /// Settings file (default: the app's settings.toml). A missing file gives the defaults.
    #[arg(long, global = true)]
    settings: Option<PathBuf>,
    /// CAT engine (overrides settings [cat] engine).
    #[arg(long, global = true, value_enum)]
    engine: Option<EngineArg>,
    /// rigctld host.
    #[arg(long, global = true)]
    host: Option<String>,
    /// rigctld port.
    #[arg(long, global = true)]
    port: Option<u16>,
    /// Hamlib rig model number.
    #[arg(long, global = true)]
    model: Option<u32>,
    /// Serial port of the hamlib engine (e.g. /dev/ttyUSB0 or COM3).
    #[arg(long, global = true)]
    serial_port: Option<String>,
    /// Serial baud rate of the hamlib engine.
    #[arg(long, global = true)]
    baud: Option<u32>,
    /// OmniRig rig number.
    #[arg(long, global = true, value_parser = clap::value_parser!(u8).range(1..=4))]
    rig: Option<u8>,
    /// Per-command timeout [ms] (clamped to 100-10000 like the app).
    #[arg(long, global = true)]
    timeout_ms: Option<u64>,
}

#[derive(Clone, Copy, ValueEnum)]
enum EngineArg {
    None,
    Rigctld,
    Hamlib,
    Omnirig,
}

/// `on` or `off` (any case) as `true` or `false`.
fn on_off() -> impl TypedValueParser<Value = bool> {
    PossibleValuesParser::new(["on", "off"]).map(|s| s.eq_ignore_ascii_case("on"))
}

/// A Doppler strategy `A` to `D` (any case).
fn strategy() -> impl TypedValueParser<Value = Doppler> {
    PossibleValuesParser::new(["A", "B", "C", "D"]).map(|s| match s.to_ascii_uppercase().as_str() {
        "B" => Doppler::B,
        "C" => Doppler::C,
        "D" => Doppler::D,
        _ => Doppler::A,
    })
}

#[derive(Subcommand)]
enum Cmd {
    /// Poll the rig and print its state.
    Status {
        /// Number of polls (0: until Ctrl-C).
        #[arg(long, default_value_t = 0)]
        count: u64,
        /// Pause between two polls [ms] (default: settings [cat] update_ms).
        #[arg(long, value_parser = clap::value_parser!(u64).range(100..=10_000))]
        interval_ms: Option<u64>,
    },
    /// Set the RX frequency, or the TX frequency with --tx [Hz].
    SetFreq {
        #[arg(allow_negative_numbers = true)]
        hz: i64,
        #[arg(long)]
        tx: bool,
    },
    /// Set the mode (CW, CW_R, LSB, USB, DIG, DIG_R, AM, FM; any case).
    SetMode { mode: String },
    /// Switch split on or off.
    Split {
        #[arg(action = clap::ArgAction::Set, ignore_case = true, value_parser = on_off())]
        on: bool,
    },
    /// Switch RIT on or off.
    Rit {
        #[arg(action = clap::ArgAction::Set, ignore_case = true, value_parser = on_off())]
        on: bool,
    },
    /// Shift RX up and TX down in steps through the CAT worker, then restore the rig.
    DopplerTest(DopplerTest),
    /// List the serial ports.
    ListPorts,
}

#[derive(Args)]
struct DopplerTest {
    #[arg(long, default_value = "A", ignore_case = true, value_parser = strategy())]
    strategy: Doppler,
    /// Shift per step [Hz].
    #[arg(long, default_value_t = 1000, allow_negative_numbers = true)]
    shift_hz: i64,
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=100))]
    steps: u32,
    /// Pause after each step [ms].
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u64).range(100..=10_000))]
    step_ms: u64,
    /// Base frequency [Hz] (default: the RX frequency read from the rig).
    #[arg(long, allow_negative_numbers = true)]
    base_hz: Option<i64>,
}

/// A failed command: the exit code, with a message for stderr.
enum Fail {
    /// Bad input, a rejected set or a failed restore (exit 1).
    Input(String),
    /// The rig is not ONLINE (exit 2; the status line is already printed).
    NotOnline,
}

impl From<String> for Fail {
    fn from(e: String) -> Self {
        Fail::Input(e)
    }
}

/// A frequency that may be written to a rig: 1 Hz to [`MAX_FREQ_HZ`].
pub fn check_freq(hz: i64) -> Result<i64, String> {
    if (1..=MAX_FREQ_HZ).contains(&hz) {
        Ok(hz)
    } else {
        Err(format!("frequency must be 1 to {MAX_FREQ_HZ} Hz"))
    }
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            // clap exits with 2 on a usage error; 2 means "rig not online" here
            let _ = e.print();
            return ExitCode::from(if e.use_stderr() { 1 } else { 0 });
        }
    };
    let r = match cli.cmd {
        Cmd::ListPorts => list_ports(),
        cmd => rig_cmd(&cli.flags, cmd),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(Fail::Input(e)) => {
            eprintln!("catcheck: {e}");
            ExitCode::from(1)
        }
        Err(Fail::NotOnline) => ExitCode::from(2),
    }
}

/// A command on the rig: settings, flags, engine checks, then the command.
fn rig_cmd(flags: &Flags, cmd: Cmd) -> Result<(), Fail> {
    let path = flags.settings.clone();
    let path = path.unwrap_or_else(Settings::default_path);
    let mut s = Settings::load_or_default(&path).map_err(|e| e.to_string())?;
    apply_flags(flags, &mut s);
    let cfg = cat_config(&s);
    check_engine(&cfg)?;
    match cmd {
        Cmd::Status { count, interval_ms } => {
            let interval = interval_ms.map_or(cfg.update, Duration::from_millis);
            status(&cfg, count, interval)
        }
        Cmd::SetFreq { hz, tx } => {
            let hz = check_freq(hz)?;
            set(&cfg, "frequency", |r| {
                if tx {
                    r.set_tx_frequency(hz)
                } else {
                    r.set_rx_frequency(hz)
                }
            })
        }
        Cmd::SetMode { mode } => {
            let m = parse_mode(&mode)?;
            set(&cfg, "mode", |r| r.set_mode(m))
        }
        Cmd::Split { on } => {
            let v = if on {
                RigSplit::SplitOn
            } else {
                RigSplit::SplitOff
            };
            set(&cfg, "split", |r| r.set_split(v))
        }
        Cmd::Rit { on } => {
            let v = if on { RigRit::RitOn } else { RigRit::RitOff };
            set(&cfg, "RIT", |r| r.set_rit(v))
        }
        Cmd::DopplerTest(t) => doppler_test(&cfg, &t),
        Cmd::ListPorts => list_ports(),
    }
}

/// The command-line flags override the settings file; `cat_config` then clamps as in the app.
fn apply_flags(cli: &Flags, s: &mut Settings) {
    let c = &mut s.cat;
    c.engine = cli.engine.map_or(c.engine, |e| match e {
        EngineArg::None => CatEngine::None,
        EngineArg::Rigctld => CatEngine::Rigctld,
        EngineArg::Hamlib => CatEngine::Hamlib,
        EngineArg::Omnirig => CatEngine::OmniRig,
    });
    if let Some(h) = &cli.host {
        c.rigctld_host.clone_from(h);
    }
    if let Some(p) = &cli.serial_port {
        c.port_name.clone_from(p);
    }
    c.rigctld_port = cli.port.unwrap_or(c.rigctld_port);
    c.hamlib_model = cli.model.unwrap_or(c.hamlib_model);
    c.baudrate = cli.baud.unwrap_or(c.baudrate);
    c.omnirig_rig = cli.rig.unwrap_or(c.omnirig_rig);
    c.timeout_ms = cli.timeout_ms.unwrap_or(c.timeout_ms);
}

/// The engine checks before any connect.
fn check_engine(cfg: &CatConfig) -> Result<(), String> {
    match cfg.engine {
        Engine::None => Err("no CAT engine selected (settings [cat] engine or --engine)".into()),
        Engine::Hamlib { .. } if !hamlib_available() => Err(
            "the hamlib engine is not compiled in; build catcheck with --features hamlib (Linux and macOS)"
                .into(),
        ),
        Engine::OmniRig { .. } if !omnirig_available() => {
            Err("OmniRig is available on Windows only".into())
        }
        _ => Ok(()),
    }
}

/// A mode by its name in any case; NONE and OTHER cannot be set.
fn parse_mode(x: &str) -> Result<RigMode, String> {
    use RigMode::{Am, Cw, CwR, Dig, DigR, Fm, Lsb, Usb};
    [Cw, CwR, Lsb, Usb, Dig, DigR, Am, Fm]
        .into_iter()
        .find(|m| m.name().eq_ignore_ascii_case(x.trim()))
        .ok_or_else(|| format!("unknown mode: {x} (CW, CW_R, LSB, USB, DIG, DIG_R, AM, FM)"))
}

/// The state of an ONLINE rig, read in the worker's order.
fn read(r: &mut dyn Rig) -> RigInfo {
    let (mode, split, rit) = (r.get_mode(), r.get_split(), r.get_rit());
    let (rx_hz, tx_hz) = (r.get_rx_frequency(), r.get_tx_frequency());
    RigInfo {
        rx_hz,
        tx_hz,
        mode,
        split,
        rit,
        doppler_active: false,
    }
}

/// `rx=… tx=… mode=… split=… rit=…`.
fn show(i: &RigInfo) -> String {
    let split = match i.split {
        RigSplit::SplitOn => "ON",
        RigSplit::SplitOff => "OFF",
        RigSplit::None => "NONE",
    };
    let rit = match i.rit {
        RigRit::RitOn => "ON",
        RigRit::RitOff => "OFF",
        RigRit::None => "NONE",
    };
    let (rx, tx, mode) = (i.rx_hz, i.tx_hz, i.mode.name());
    format!("rx={rx} tx={tx} mode={mode} split={split} rit={rit}")
}

/// A rig of the checked config (`check_engine` passed, so the engine is compiled in).
fn open(cfg: &CatConfig) -> Result<Box<dyn Rig>, String> {
    make_rig(cfg).ok_or_else(|| "no rig for this engine".to_string())
}

/// The state when `st` is ONLINE; otherwise the status line (`NAME`, plus the error text when
/// there is one) is printed.
fn online_state(r: &mut dyn Rig, st: RigStatus) -> Option<RigInfo> {
    let err = r.take_error();
    if st == RigStatus::Online {
        return Some(read(r));
    }
    let err = err.map(|e| format!(" {e}")).unwrap_or_default();
    println!("{}{err}", st.name());
    None
}

/// The first poll of a fresh rig: the `rig:` line, then as [`online_state`]. The ident is
/// printed after the connect, because an engine can learn its ident there (OmniRig).
fn first_poll(r: &mut dyn Rig) -> Option<RigInfo> {
    let st = r.get_status(Doppler::None);
    println!("rig: {}", r.ident().rig_type);
    online_state(r, st)
}

/// Print the ONLINE line of a poll result.
fn print_online(s: Option<RigInfo>) -> Option<RigInfo> {
    if let Some(i) = &s {
        println!("ONLINE {}", show(i));
    }
    s
}

/// One later poll; it prints the status line in every case.
fn poll(r: &mut dyn Rig) -> Option<RigInfo> {
    let st = r.get_status(Doppler::None);
    print_online(online_state(r, st))
}

/// Sleep `d` in steps of at most 250 ms.
fn sleep(d: Duration) {
    let end = Instant::now() + d;
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        std::thread::sleep(left.min(STEP));
    }
}

fn status(cfg: &CatConfig, count: u64, interval: Duration) -> Result<(), Fail> {
    let mut r = open(cfg)?;
    let mut online = print_online(first_poll(r.as_mut())).is_some();
    let mut n = 1;
    while count == 0 || n < count {
        sleep(interval);
        online = poll(r.as_mut()).is_some();
        n += 1;
    }
    online.then_some(()).ok_or(Fail::NotOnline)
}

/// A set command: the rig must be ONLINE first (setters never reconnect); then the state is
/// printed again.
fn set(cfg: &CatConfig, what: &str, f: impl FnOnce(&mut dyn Rig) -> bool) -> Result<(), Fail> {
    let mut r = open(cfg)?;
    if print_online(first_poll(r.as_mut())).is_none() {
        return Err(Fail::NotOnline);
    }
    if !f(r.as_mut()) {
        return Err(format!("the rig rejected the {what}").into());
    }
    poll(r.as_mut()).map(|_| ()).ok_or(Fail::NotOnline)
}

/// Every step `(rx, tx) = (base + k·shift, base − k·shift)`, `k = 1..=steps`, checked.
fn doppler_steps(base: i64, shift: i64, steps: u32) -> Result<Vec<(i64, i64)>, String> {
    let bad = || check_freq(0).unwrap_err();
    (1..=i64::from(steps))
        .map(|k| {
            let d = k.checked_mul(shift).ok_or_else(bad)?;
            let rx = base.checked_add(d).ok_or_else(bad)?;
            let tx = base.checked_sub(d).ok_or_else(bad)?;
            Ok((check_freq(rx)?, check_freq(tx)?))
        })
        .collect()
}

fn print_events(ev: Vec<CatEvent>) {
    for e in ev {
        match e {
            CatEvent::Text(t) => println!("text: {t}"),
            CatEvent::Status(s) => println!("status: {}", s.name()),
            CatEvent::Info(i) => println!("info: {}", show(&i)),
            CatEvent::Ident(_) => {}
        }
    }
}

fn doppler_test(cfg: &CatConfig, t: &DopplerTest) -> Result<(), Fail> {
    // 1. With a given base every step is checked before any connect.
    if let Some(b) = t.base_hz {
        doppler_steps(b, t.shift_hz, t.steps)?;
    }
    // 2. Save the state first: a Ctrl-C during the steps may leave the rig shifted in split.
    // The rig is dropped at the end of the statement, so the worker connects anew.
    let saved = first_poll(open(cfg)?.as_mut()).ok_or(Fail::NotOnline)?;
    println!("saved: {}", show(&saved));
    // 3. Every step is checked before anything is set.
    let steps = doppler_steps(t.base_hz.unwrap_or(saved.rx_hz), t.shift_hz, t.steps)?;
    // 4. The steps through the CAT worker.
    let mut w = CatWorker::new(factory(cfg.clone()), cfg.update);
    for (k, &(rx_hz, tx_hz)) in steps.iter().enumerate() {
        let (ev, _) = w.pass(DopplerTarget {
            strategy: t.strategy,
            rx_hz,
            tx_hz,
        });
        print_events(ev);
        println!("step {}: rx={rx_hz} tx={tx_hz}", k + 1);
        sleep(Duration::from_millis(t.step_ms));
    }
    // 5. The leave, then the end of the worker.
    let (ev, _) = w.pass(DopplerTarget::default());
    print_events(ev);
    print_events(w.finish());
    drop(w);
    // 6. A fresh rig reads the state again.
    let mut r = open(cfg)?;
    let online = r.get_status(Doppler::None) == RigStatus::Online;
    let now = online.then(|| read(r.as_mut()));
    if now == Some(saved) {
        println!("restored: yes");
        return Ok(());
    }
    let now = now.map_or_else(|| "not online".to_string(), |i| show(&i));
    println!("restored: no (saved: {}; now: {now})", show(&saved));
    Err("the rig was not restored".to_string().into())
}

fn list_ports() -> Result<(), Fail> {
    let ports = serial2::SerialPort::available_ports()
        .map_err(|e| format!("cannot list the serial ports: {e}"))?;
    if ports.is_empty() {
        println!("no serial ports found");
    }
    for p in ports {
        println!("{}", p.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn freq_and_doppler_steps_are_checked_without_overflow() {
        assert_eq!(check_freq(1), Ok(1));
        assert_eq!(check_freq(MAX_FREQ_HZ), Ok(MAX_FREQ_HZ));
        for h in [0, -5, MAX_FREQ_HZ + 1, i64::MIN] {
            assert!(check_freq(h).is_err());
        }
        assert_eq!(
            doppler_steps(144_300_000, 1000, 2),
            Ok(vec![(144_301_000, 144_299_000), (144_302_000, 144_298_000)])
        );
        assert!(doppler_steps(1000, 1000, 2).is_err());
        assert!(doppler_steps(i64::MAX, i64::MAX, 2).is_err());
        assert!(doppler_steps(144_300_000, i64::MIN, 1).is_err());
    }

    #[test]
    fn modes_any_case_but_not_none_or_other() {
        assert_eq!(parse_mode("cw_r"), Ok(RigMode::CwR));
        assert_eq!(parse_mode("USB"), Ok(RigMode::Usb));
        for x in ["none", "OTHER", ""] {
            assert!(parse_mode(x).unwrap_err().starts_with("unknown mode: "));
        }
    }
}
