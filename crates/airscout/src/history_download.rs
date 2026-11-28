//! History from a URL: a generic downloader for per-day archives of ADS-B snapshots
//! (`HistoryFromURLDlg`, `MapDlg.cs:8653-8920`). There is no default URL: the user supplies it.
//!
//! The archive is a zip with one `acList` JSON per step (`<date>-HHMMZ.json`). The entries are
//! read in memory straight from the zip (the C# extraction is commented out, :8730-8734).

use crate::background::strip_query;
use crate::jobs::{JobCtx, JobEvent};
use as_aircraft::db::{AircraftDb, AircraftType};
use as_aircraft::position_files::read_csv;
use as_aircraft::positions::PositionDb;
use as_aircraft::{check, PlaneCategory, PlaneInfo};
use as_settings::Area;
use as_settings::Settings;
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::time::{Duration, Instant};

/// Free space the job needs before it starts (MapDlg.cs:8875-8878).
const MIN_FREE_BYTES: u64 = 50 * 1024 * 1024 * 1024;
/// Largest uncompressed size read from one zip entry; bigger ones are skipped (zip bomb guard).
pub const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
/// No data for this long ends the download as stalled.
pub const STALL: Duration = Duration::from_secs(60);
/// Longest a transfer thread lives (a day archive is about 8 GB).
const MAX_TRANSFER: Duration = Duration::from_secs(12 * 3600);
const POLL: Duration = Duration::from_millis(100);
const CHUNK: usize = 1000;

/// `base` + `/` (if missing) + `<date>.zip` (MapDlg.cs:8880-8890); a `?query` of `base` stays
/// last.
pub fn archive_url(base: &str, date: NaiveDate) -> String {
    // a query belongs after the file name (the C# concatenates blindly)
    let (path, query) = base.split_at(base.find('?').unwrap_or(base.len()));
    let slash = if path.ends_with('/') { "" } else { "/" };
    format!("{path}{slash}{}.zip{query}", date.format("%Y-%m-%d"))
}

/// `<date>-HHMMZ.json` for every step of the day (MapDlg.cs:8740-8750).
pub fn step_entry_names(date: NaiveDate, step_min: u32) -> Vec<String> {
    (0..1440u32)
        .step_by(step_min.max(1) as usize)
        .map(|i| {
            format!(
                "{}-{:02}{:02}Z.json",
                date.format("%Y-%m-%d"),
                i / 60,
                i % 60
            )
        })
        .collect()
}

fn str_of(o: &serde_json::Map<String, Value>, k: &str) -> Option<String> {
    o.get(k).and_then(Value::as_str).map(str::to_string)
}

/// A number or numeric string. Absent or null is `Ok(None)`; any other type is an error, which
/// drops the record (the C# conversion throws and its `catch` skips the plane).
fn num_of(o: &serde_json::Map<String, Value>, k: &str) -> Result<Option<f64>, ()> {
    match o.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_f64().map(Some).ok_or(()),
        Some(Value::String(s)) => s.trim().parse::<f64>().map(Some).map_err(|_| ()),
        Some(_) => Err(()),
    }
}

/// `ReadPropertyDoubleToInt`: truncated, `int.MinValue` when absent or out of range.
fn int_of(o: &serde_json::Map<String, Value>, k: &str) -> Result<f64, ()> {
    Ok(match num_of(o, k)? {
        Some(d) if d >= i32::MIN as f64 && d <= i32::MAX as f64 => d.trunc(),
        _ => i32::MIN as f64,
    })
}

fn plane_of(
    o: &serde_json::Map<String, Value>,
    types: &dyn Fn(&str) -> Option<AircraftType>,
) -> Result<PlaneInfo, ()> {
    let type_code = str_of(o, "Type").unwrap_or_default();
    let (manufacturer, model, category) = match types(&type_code) {
        Some(t) => (t.manufacturer, t.model, t.category),
        None => ("[unknown]".into(), "[unknown]".into(), PlaneCategory::None),
    };
    let time = num_of(o, "PosTime")?
        .and_then(|ms| DateTime::from_timestamp_millis(ms as i64))
        .unwrap_or(DateTime::<Utc>::MIN_UTC);
    Ok(PlaneInfo {
        time,
        call: str_of(o, "Call").unwrap_or_default(),
        reg: str_of(o, "Reg").unwrap_or_default(),
        hex: str_of(o, "Icao").unwrap_or_default(),
        // ReadPropertyDouble: `double.MinValue` when absent
        lat: num_of(o, "Lat")?.unwrap_or(f64::MIN),
        lon: num_of(o, "Long")?.unwrap_or(f64::MIN),
        track: int_of(o, "Trak")?,
        // "GAlt", the altitude corrected by air pressure, not "Alt" (MapDlg.cs:8776)
        alt_ft: int_of(o, "GAlt")?,
        speed_kts: int_of(o, "Spd")?,
        type_code,
        manufacturer,
        model,
        category,
        ..PlaneInfo::default()
    })
}

/// The planes of `root.acList[]` (MapDlg.cs:8752-8800); the type is completed from `types`.
/// A text that is not JSON, or has no `acList` array, gives no planes.
pub fn parse_aclist(json: &str, types: &dyn Fn(&str) -> Option<AircraftType>) -> Vec<PlaneInfo> {
    let Ok(root) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let Some(list) = root.get("acList").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(Value::as_object)
        .filter_map(|o| plane_of(o, types).ok())
        .collect()
}

/// The altitude and area limits of the live feed, copied from the settings. (The job carries
/// this instead of `Settings`, whose `Debug` output holds credentials.)
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub min_alt_m: i32,
    pub max_alt_m: i32,
    pub area: Area,
}

impl Filter {
    pub fn from_settings(s: &Settings) -> Self {
        Self {
            min_alt_m: s.planes_min_alt_m,
            max_alt_m: s.planes_max_alt_m,
            area: s.area,
        }
    }

    pub fn keep(&self, p: &PlaneInfo) -> bool {
        let a = &self.area;
        check::plane(p)
            && p.alt_m() >= f64::from(self.min_alt_m)
            && p.alt_m() <= f64::from(self.max_alt_m)
            && p.lat >= a.min_lat
            && p.lat <= a.max_lat
            && p.lon >= a.min_lon
            && p.lon <= a.max_lon
    }
}

/// `check::plane`, the altitude window and the area of `s` (MapDlg.cs:8800-8815).
pub fn keep(p: &PlaneInfo, s: &Settings) -> bool {
    Filter::from_settings(s).keep(p)
}

/// Free space of the volume that holds a directory.
pub trait DiskSpace {
    fn available(&self, dir: &Path) -> io::Result<u64>;
}

/// The real disk, through `fs4`.
pub struct Fs4Disk;

impl DiskSpace for Fs4Disk {
    fn available(&self, dir: &Path) -> io::Result<u64> {
        fs4::available_space(dir)
    }
}

/// What the user asked for.
#[derive(Clone)]
pub struct DownloadRequest {
    pub url_base: String,
    pub date: NaiveDate,
    pub step_min: u32,
    /// Holds the zip, the csv and the `.part` file.
    pub dir: PathBuf,
    pub filter: Filter,
}

impl std::fmt::Debug for DownloadRequest {
    /// The URL without its query string: it can carry credentials.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadRequest")
            .field("url_base", &strip_query(&self.url_base))
            .field("date", &self.date)
            .field("step_min", &self.step_min)
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

/// Read at most `cap` bytes: `None` if the stream is longer.
pub fn read_capped(r: impl Read, cap: u64) -> io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    r.take(cap + 1).read_to_end(&mut buf)?;
    Ok((buf.len() as u64 <= cap).then_some(buf))
}

enum Msg {
    Head(Option<u64>),
    Chunk(Vec<u8>),
    End,
    Err(String),
}

/// The transfer, on its own thread so that the job can poll the cancel flag while the socket
/// blocks. It stops when the receiver is gone.
fn fetch(url: &str, tx: &SyncSender<Msg>) {
    let shown = strip_query(url);
    let get = || -> Result<(), String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(as_net::user_agent())
            .connect_timeout(Duration::from_secs(15))
            // reqwest 0.12 blocking has no read timeout: the job ends a stall itself (`stall`);
            // this total limit only bounds a transfer thread whose job is gone
            .timeout(MAX_TRANSFER)
            .build()
            .map_err(|e| {
                format!(
                    "cannot start download: {}",
                    strip_query(&e.without_url().to_string())
                )
            })?;
        let mut resp = client.get(url).send().map_err(|e| {
            format!(
                "cannot reach {shown}: {}",
                strip_query(&e.without_url().to_string())
            )
        })?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {} from {shown}", resp.status().as_u16()));
        }
        if tx.send(Msg::Head(resp.content_length())).is_err() {
            return Ok(());
        }
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = resp.read(&mut buf).map_err(|e| {
                format!(
                    "download of {shown} failed: {}",
                    strip_query(&e.to_string())
                )
            })?;
            if n == 0 {
                return Ok(());
            }
            if tx.send(Msg::Chunk(buf[..n].to_vec())).is_err() {
                return Ok(());
            }
        }
    };
    let _ = tx.send(match get() {
        Ok(()) => Msg::End,
        Err(e) => Msg::Err(e),
    });
}

fn gb(b: u64) -> f64 {
    b as f64 / 1024.0 / 1024.0 / 1024.0
}

/// `HistoryDownloader_DownloadProgressChanged` (MapDlg.cs:8655-8670).
fn progress_text(got: u64, total: Option<u64>) -> String {
    match total.filter(|t| *t > 0) {
        Some(t) => format!(
            "Downloaded {:.2} GB of {:.2} GB  ({:.2}%).",
            gb(got),
            gb(t),
            got as f64 / t as f64 * 100.0
        ),
        None => format!("Downloaded {:.2} GB.", gb(got)),
    }
}

/// Stream the archive to `part`, rename it to `zip` on success. `Ok(false)` if cancelled.
fn download(
    ctx: &JobCtx,
    url: &str,
    part: &Path,
    zip: &Path,
    stall: Duration,
) -> Result<bool, String> {
    let (tx, rx): (_, Receiver<Msg>) = sync_channel(16);
    let owned = url.to_string();
    std::thread::Builder::new()
        .name("history-download".into())
        .spawn(move || fetch(&owned, &tx))
        .map_err(|e| format!("cannot start download: {e}"))?;
    ctx.progress(format!("Downloading {}", strip_query(url)));
    let result = receive(ctx, &rx, part, stall);
    // the receiver is dropped on return: the transfer thread ends at its next chunk
    match result {
        Ok(true) => std::fs::rename(part, zip)
            .map(|()| true)
            .map_err(|e| e.to_string()),
        other => {
            let _ = std::fs::remove_file(part);
            other
        }
    }
}

fn receive(ctx: &JobCtx, rx: &Receiver<Msg>, part: &Path, stall: Duration) -> Result<bool, String> {
    let mut file =
        File::create(part).map_err(|e| format!("cannot write {}: {e}", part.display()))?;
    let (mut total, mut got) = (None, 0u64);
    let (mut last_data, mut last_report) = (Instant::now(), Instant::now());
    loop {
        if ctx.cancelled() {
            return Ok(false);
        }
        match rx.recv_timeout(POLL) {
            Ok(Msg::Head(t)) => total = t,
            Ok(Msg::Chunk(b)) => {
                file.write_all(&b)
                    .map_err(|e| format!("cannot write {}: {e}", part.display()))?;
                got += b.len() as u64;
                last_data = Instant::now();
                if last_report.elapsed() >= Duration::from_millis(250) {
                    ctx.progress(progress_text(got, total));
                    last_report = Instant::now();
                }
            }
            Ok(Msg::End) => break,
            Ok(Msg::Err(e)) => return Err(e),
            Err(RecvTimeoutError::Timeout) if last_data.elapsed() > stall => {
                return Err(format!(
                    "download stalled: no data for {} s",
                    stall.as_secs().max(1)
                ));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err("download ended unexpectedly".into()),
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    if total.is_some_and(|t| t != got) {
        return Err("download incomplete".into());
    }
    ctx.progress(progress_text(got, total));
    Ok(true)
}

/// `{time u};{call};{reg};{hex};{lat F8};{lon F8};{track};{alt};{speed};;;{type}` (MapDlg.cs:8817).
fn csv_row(p: &PlaneInfo) -> String {
    let clean = |s: &str| s.replace([';', '\r', '\n'], " ");
    format!(
        "{};{};{};{};{:.8};{:.8};{};{};{};;;{}",
        p.time.format("%Y-%m-%d %H:%M:%SZ"),
        clean(&p.call),
        clean(&p.reg),
        clean(&p.hex),
        p.lat,
        p.lon,
        p.track as i64,
        p.alt_ft as i64,
        p.speed_kts as i64,
        clean(&p.type_code)
    )
}

const CSV_HEADER: &str = "time;call;reg;hex;lat;lon;track;alt;speed;squawk;radar;type";

/// Run the job in the calling thread: download (unless the zip exists), read the steps, write
/// the csv, import it. Errors become `JobEvent::Failed`; a cancelled job reports nothing more.
pub fn run(ctx: &JobCtx, data_dir: &Path, req: &DownloadRequest, disk: &dyn DiskSpace) {
    run_with_stall(ctx, data_dir, req, disk, STALL);
}

/// `run` with a custom stall limit (no data for that long ends the download).
pub fn run_with_stall(
    ctx: &JobCtx,
    data_dir: &Path,
    req: &DownloadRequest,
    disk: &dyn DiskSpace,
    stall: Duration,
) {
    if let Err(e) = run_inner(ctx, data_dir, req, disk, stall) {
        ctx.send(JobEvent::Failed(e));
    }
}

fn run_inner(
    ctx: &JobCtx,
    data_dir: &Path,
    req: &DownloadRequest,
    disk: &dyn DiskSpace,
    stall: Duration,
) -> Result<(), String> {
    std::fs::create_dir_all(&req.dir)
        .map_err(|e| format!("cannot create {}: {e}", req.dir.display()))?;
    let free = disk
        .available(&req.dir)
        .map_err(|e| format!("cannot read the free disk space: {e}"))?;
    if free < MIN_FREE_BYTES {
        return Err("Not enough disk space to run this operation.".into());
    }
    let stem = req.date.format("%Y-%m-%d").to_string();
    let zip_path = req.dir.join(format!("{stem}.zip"));
    let mut fresh = false;
    if !zip_path.exists() {
        let part = req.dir.join(format!("{stem}.zip.part"));
        if !download(
            ctx,
            &archive_url(&req.url_base, req.date),
            &part,
            &zip_path,
            stall,
        )? {
            return Ok(());
        }
        fresh = true;
    }
    let csv_path = req.dir.join(format!("{stem}.csv"));
    let name = stem.clone() + ".zip";
    let zip = File::open(&zip_path)
        .map_err(|e| format!("cannot open {name}: {e}"))
        .and_then(|f| {
            zip::ZipArchive::new(f)
                .map_err(|e| format!("{name} is not a readable zip archive: {e}"))
        })
        .inspect_err(|_| {
            // only an archive that is itself unreadable is dropped: it would block every retry.
            // Local failures below keep the download.
            if fresh {
                let _ = std::fs::remove_file(&zip_path);
            }
        })?;
    let stats = write_csv(ctx, req, zip, &csv_path, data_dir)?;
    let Some(stats) = stats else { return Ok(()) };
    if stats.present == 0 {
        return Err(format!(
            "none of the {} step files is in the archive",
            stats.present + stats.missing
        ));
    }
    ctx.progress(format!("Importing {}", csv_path.display()));
    let text = std::fs::read_to_string(&csv_path).map_err(|e| e.to_string())?;
    let rows = read_csv(&text)?.imported;
    let db = PositionDb::open(data_dir).map_err(|e| e.to_string())?;
    let mut stored = 0;
    for chunk in rows.chunks(CHUNK) {
        if ctx.cancelled() {
            return Ok(());
        }
        stored += db.bulk_insert(chunk).map_err(|e| e.to_string())?;
    }
    ctx.progress(format!("{stored} positions stored"));
    let unusable = if stats.unusable > 0 {
        format!(", {} unusable", stats.unusable)
    } else {
        String::new()
    };
    ctx.send(JobEvent::Done(format!(
        "{} steps imported, {} missing{unusable}",
        stats.present, stats.missing
    )));
    Ok(())
}

struct StepStats {
    present: usize,
    missing: usize,
    /// Entries that are too large, unreadable or not JSON.
    unusable: usize,
}

/// Read every step entry and write the kept planes to `csv_path`. `None` if cancelled.
fn write_csv(
    ctx: &JobCtx,
    req: &DownloadRequest,
    mut zip: zip::ZipArchive<File>,
    csv_path: &Path,
    data_dir: &Path,
) -> Result<Option<StepStats>, String> {
    ctx.progress("Reading the archive");
    let aircraft = AircraftDb::open(data_dir).map_err(|e| e.to_string())?;
    let types = |t: &str| aircraft.aircraft_type_find_by_icao(t).ok().flatten();
    let mut out = BufWriter::new(
        File::create(csv_path).map_err(|e| format!("cannot write {}: {e}", csv_path.display()))?,
    );
    let io = |e: io::Error| format!("cannot write {}: {e}", csv_path.display());
    writeln!(out, "{CSV_HEADER}").map_err(io)?;
    let mut st = StepStats {
        present: 0,
        missing: 0,
        unusable: 0,
    };
    for entry in step_entry_names(req.date, req.step_min) {
        if ctx.cancelled() {
            return Ok(None);
        }
        ctx.progress(format!("Processing {entry}"));
        let Ok(zf) = zip.by_name(&entry) else {
            st.missing += 1;
            continue;
        };
        let json = match read_capped(zf, MAX_ENTRY_BYTES)
            .map(|b| b.and_then(|b| String::from_utf8(b).ok()))
        {
            Ok(Some(j)) => j,
            _ => {
                st.unusable += 1;
                continue;
            }
        };
        st.present += 1;
        for p in parse_aclist(&json, &types)
            .iter()
            .filter(|p| req.filter.keep(p))
        {
            writeln!(out, "{}", csv_row(p)).map_err(io)?;
        }
    }
    out.flush().map_err(io)?;
    Ok(Some(st))
}
