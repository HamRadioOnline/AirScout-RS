//! History from URL: the pure helpers and the download job against a mockito server, a temp
//! data dir and a fake disk (no network, no 50 GB).
use airscout::analysis::AnalysisJob;
use airscout::history_download::{
    archive_url, keep, parse_aclist, read_capped, run, run_with_stall, step_entry_names, DiskSpace,
    DownloadRequest, Filter,
};
use airscout::jobs::{JobCtx, JobEvent};
use airscout::services::ServiceEvent;
use airscout::ui::dialogs::history_url::HistoryUrlDlg;
use as_aircraft::db::AircraftType;
use as_aircraft::positions::PositionDb;
use as_aircraft::{PlaneCategory, PlaneInfo};
use as_settings::Settings;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

const GB: u64 = 1024 * 1024 * 1024;

struct FakeDisk(u64);

impl DiskSpace for FakeDisk {
    fn available(&self, _dir: &Path) -> io::Result<u64> {
        Ok(self.0)
    }
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2017, 7, 1).unwrap()
}

fn plane_json(hex: &str, lat: f64, alt: f64, post_ms: i64) -> String {
    format!(
        r#"{{"Icao":"{hex}","Call":"DLH123","Lat":{lat},"Long":8.1,"Trak":90.4,"GAlt":{alt},"Spd":400.7,"Reg":"D-ABCD","Type":"A320","PosTime":{post_ms}}}"#
    )
}

fn step_json(planes: &[String]) -> String {
    format!(
        r#"{{"acList":[{}],"total":{}}}"#,
        planes.join(","),
        planes.len()
    )
}

/// A zip with the given step files (stored under the names of `step_entry_names`).
fn zip_with(steps: &[(usize, String)]) -> Vec<u8> {
    let names = step_entry_names(date(), 5);
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for (i, json) in steps {
        w.start_file(names[*i].clone(), opts).unwrap();
        w.write_all(json.as_bytes()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn two_step_zip() -> Vec<u8> {
    // 2017-07-01 00:05:00Z and 00:10:00Z
    zip_with(&[
        (
            1,
            step_json(&[plane_json("3C6444", 50.1, 30000.0, 1_498_867_500_000)]),
        ),
        (
            2,
            step_json(&[plane_json("3C6444", 50.2, 30000.0, 1_498_867_800_000)]),
        ),
    ])
}

fn req(base: &str, dir: &Path) -> DownloadRequest {
    DownloadRequest {
        url_base: base.to_string(),
        date: date(),
        step_min: 5,
        dir: dir.to_path_buf(),
        filter: Filter::from_settings(&Settings::default()),
    }
}

fn ctx() -> (JobCtx, Receiver<ServiceEvent>, Arc<AtomicBool>) {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    (JobCtx::new(3, cancel.clone(), tx), rx, cancel)
}

fn events(rx: &Receiver<ServiceEvent>) -> Vec<JobEvent> {
    rx.try_iter()
        .map(|e| match e {
            ServiceEvent::Job { event, .. } => event,
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

fn done(ev: &[JobEvent]) -> Option<&str> {
    ev.iter().find_map(|e| match e {
        JobEvent::Done(s) => Some(s.as_str()),
        _ => None,
    })
}

fn failed(ev: &[JobEvent]) -> Option<&str> {
    ev.iter().find_map(|e| match e {
        JobEvent::Failed(s) => Some(s.as_str()),
        _ => None,
    })
}

fn run_job(data: &Path, r: &DownloadRequest, disk: u64) -> Vec<JobEvent> {
    let (c, rx, _) = ctx();
    run(&c, data, r, &FakeDisk(disk));
    events(&rx)
}

#[test]
fn archive_url_adds_slash() {
    assert_eq!(
        archive_url("http://h/x", date()),
        "http://h/x/2017-07-01.zip"
    );
    assert_eq!(
        archive_url("http://h/x/", date()),
        "http://h/x/2017-07-01.zip"
    );
}

#[test]
fn archive_url_puts_the_file_before_the_query() {
    // deviates from the C# plain concatenation (MapDlg.cs:8880-8890)
    assert_eq!(
        archive_url("https://h/x?token=S", date()),
        "https://h/x/2017-07-01.zip?token=S"
    );
    assert_eq!(
        archive_url("https://h/x/?token=S", date()),
        "https://h/x/2017-07-01.zip?token=S"
    );
}

#[test]
fn step_entry_names_every_5_min() {
    let n = step_entry_names(date(), 5);
    assert_eq!(n.len(), 288);
    assert_eq!(n[0], "2017-07-01-0000Z.json");
    assert_eq!(n[1], "2017-07-01-0005Z.json");
    assert_eq!(n[287], "2017-07-01-2355Z.json");
    // a step of 0 is treated as 1 and never loops forever
    assert_eq!(step_entry_names(date(), 0).len(), 1440);
}

#[test]
fn parse_aclist_fields() {
    let json = format!(
        r#"{{"acList":[{},{{"Icao":"ABCDEF"}}]}}"#,
        plane_json("3C6444", 50.1, 30000.7, 1_498_867_500_123)
    );
    let types = |t: &str| {
        (t == "A320").then(|| AircraftType {
            icao: "A320".into(),
            iata: String::new(),
            manufacturer: "Airbus".into(),
            model: "A320".into(),
            category: PlaneCategory::Medium,
            last_updated: Utc::now(),
        })
    };
    let v = parse_aclist(&json, &types);
    assert_eq!(v.len(), 2);
    let p = &v[0];
    assert_eq!(p.hex, "3C6444");
    assert_eq!(p.call, "DLH123");
    assert_eq!(p.reg, "D-ABCD");
    assert_eq!((p.lat, p.lon), (50.1, 8.1));
    // Trak, GAlt and Spd are truncated to int like ReadPropertyDoubleToInt
    assert_eq!((p.track, p.alt_ft, p.speed_kts), (90.0, 30000.0, 400.0));
    assert_eq!(p.type_code, "A320");
    assert_eq!(p.manufacturer, "Airbus");
    assert_eq!(p.category, PlaneCategory::Medium);
    assert_eq!(p.time, Utc.timestamp_millis_opt(1_498_867_500_123).unwrap());
    // a record with only an Icao: missing values are C# MinValue, the type is unknown
    let q = &v[1];
    assert_eq!(q.hex, "ABCDEF");
    assert_eq!(q.time, DateTime::<Utc>::MIN_UTC);
    assert_eq!(q.lat, f64::MIN);
    assert_eq!(q.manufacturer, "[unknown]");
    assert!(!keep(q, &Settings::default()));
    // garbage is not a panic
    assert!(parse_aclist("not json", &types).is_empty());
    assert!(parse_aclist(r#"{"acList":5}"#, &types).is_empty());
}

#[test]
fn keep_filters_alt_and_area() {
    let s = Settings::default(); // 5000..12200 m, lat 35..60, lon -15..30
    let json = step_json(&[
        plane_json("3C6444", 50.0, 30000.0, 1_498_867_500_000), // 9144 m: kept
        plane_json("3C6445", 50.0, 1000.0, 1_498_867_500_000),  // 305 m: too low
        plane_json("3C6446", 50.0, 45000.0, 1_498_867_500_000), // 13716 m: too high
        plane_json("3C6447", 70.0, 30000.0, 1_498_867_500_000), // outside the area
    ]);
    let v: Vec<PlaneInfo> = parse_aclist(&json, &|_| None);
    let kept: Vec<&str> = v
        .iter()
        .filter(|p| keep(p, &s))
        .map(|p| p.hex.as_str())
        .collect();
    assert_eq!(kept, ["3C6444"]);
    // every side of the area is a bound
    let ok = v.iter().find(|p| p.hex == "3C6444").unwrap().clone();
    for (lat, lon) in [(30.0, 8.1), (65.0, 8.1), (50.0, -20.0), (50.0, 40.0)] {
        let p = PlaneInfo {
            lat,
            lon,
            ..ok.clone()
        };
        assert!(!keep(&p, &s), "{lat}/{lon} is outside");
    }
}

#[test]
fn read_capped_refuses_oversize() {
    assert_eq!(
        read_capped(Cursor::new(vec![1u8; 10]), 10).unwrap(),
        Some(vec![1u8; 10])
    );
    assert_eq!(read_capped(Cursor::new(vec![1u8; 11]), 10).unwrap(), None);
}

#[test]
fn low_disk_space_refuses_before_request() {
    let mut server = mockito::Server::new();
    let m = server.mock("GET", mockito::Matcher::Any).expect(0).create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let ev = run_job(data.path(), &req(&server.url(), &dir), 50 * GB - 1);
    assert_eq!(
        failed(&ev),
        Some("Not enough disk space to run this operation.")
    );
    m.assert();
    // 50 GB exactly is enough
    let ev = run_job(data.path(), &req("http://127.0.0.1:1", &dir), 50 * GB);
    assert!(failed(&ev).is_some_and(|e| !e.contains("disk space")));
}

#[test]
fn http_404_reports_error_and_imports_nothing() {
    let mut server = mockito::Server::new();
    let m = server
        .mock("GET", "/2017-07-01.zip")
        .with_status(404)
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    m.assert();
    assert!(failed(&ev).unwrap().contains("404"));
    assert_eq!(PositionDb::open(data.path()).unwrap().count().unwrap(), 0);
    assert!(!dir.join("2017-07-01.zip").exists());
    assert!(!dir.join("2017-07-01.zip.part").exists());
}

#[test]
fn http_500_and_network_down_are_errors() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/2017-07-01.zip")
        .with_status(500)
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    assert!(failed(&ev).unwrap().contains("500"));
    let ev = run_job(data.path(), &req("http://127.0.0.1:1", &dir), 60 * GB);
    assert!(failed(&ev).is_some());
}

#[test]
fn error_texts_do_not_carry_the_query() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/2017-07-01.zip")
        .match_query(mockito::Matcher::Any)
        .with_status(403)
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let base = format!("{}?token=SECRET123", server.url());
    let ev = run_job(data.path(), &req(&base, &dir), 60 * GB);
    for e in &ev {
        let text = match e {
            JobEvent::Progress(s) | JobEvent::Done(s) | JobEvent::Failed(s) => s,
            _ => continue,
        };
        assert!(!text.contains("SECRET123"), "{text}");
    }
    assert!(
        !format!("{:?}", AnalysisJob::Download(Box::new(req(&base, &dir)))).contains("SECRET123")
    );
}

#[test]
fn corrupt_zip_is_error_not_panic() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/2017-07-01.zip")
        .with_body("this is not a zip archive at all")
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    assert!(failed(&ev).unwrap().contains("zip"));
    // a zip we just downloaded and cannot read is dropped, so the next try downloads again
    assert!(!dir.join("2017-07-01.zip").exists());
}

#[test]
fn truncated_zip_is_error() {
    let mut z = two_step_zip();
    z.truncate(z.len() / 2);
    let mut server = mockito::Server::new();
    server.mock("GET", "/2017-07-01.zip").with_body(z).create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    assert!(failed(&ev).is_some());
    assert_eq!(PositionDb::open(data.path()).unwrap().count().unwrap(), 0);
}

#[test]
fn missing_steps_are_counted() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/2017-07-01.zip")
        .with_body(two_step_zip())
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    assert_eq!(done(&ev), Some("2 steps imported, 286 missing"));
}

#[test]
fn archive_without_any_step_is_an_error() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/2017-07-01.zip")
        .with_body(zip_with(&[]))
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    assert!(failed(&ev).unwrap().contains("288"));
}

#[test]
fn download_imports_into_position_db() {
    let mut server = mockito::Server::new();
    let m = server
        .mock("GET", "/2017-07-01.zip")
        .match_header("user-agent", as_net::user_agent().as_str())
        .with_body(two_step_zip())
        .expect(1)
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    m.assert();
    assert!(done(&ev).is_some(), "{ev:?}");
    let rows = PositionDb::open(data.path())
        .unwrap()
        .get_all(DateTime::<Utc>::MIN_UTC, DateTime::<Utc>::MAX_UTC)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].hex, "3C6444");
    assert_eq!(
        rows[0].last_updated,
        Utc.with_ymd_and_hms(2017, 7, 1, 0, 5, 0).unwrap()
    );
    assert_eq!((rows[0].lat, rows[0].alt_ft), (50.1, 30000.0));
    // the zip is kept, the .part is gone, the csv has the C# header and row format
    assert!(dir.join("2017-07-01.zip").exists());
    assert!(!dir.join("2017-07-01.zip.part").exists());
    let csv = std::fs::read_to_string(dir.join("2017-07-01.csv")).unwrap();
    let mut lines = csv.lines();
    assert_eq!(
        lines.next(),
        Some("time;call;reg;hex;lat;lon;track;alt;speed;squawk;radar;type")
    );
    assert_eq!(
        lines.next(),
        Some(
            "2017-07-01 00:05:00Z;DLH123;D-ABCD;3C6444;50.10000000;8.10000000;90;30000;400;;;A320"
        )
    );
}

#[test]
fn existing_zip_is_not_downloaded_again() {
    let mut server = mockito::Server::new();
    let m = server.mock("GET", mockito::Matcher::Any).expect(0).create();
    let data = tempfile::tempdir().unwrap();
    let dir: PathBuf = data.path().join("h");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("2017-07-01.zip"), two_step_zip()).unwrap();
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    m.assert();
    assert_eq!(done(&ev), Some("2 steps imported, 286 missing"));
}

#[test]
fn corrupt_existing_zip_is_kept() {
    let data = tempfile::tempdir().unwrap();
    let dir: PathBuf = data.path().join("h");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("2017-07-01.zip"), b"junk").unwrap();
    let ev = run_job(data.path(), &req("http://127.0.0.1:1", &dir), 60 * GB);
    assert!(failed(&ev).unwrap().contains("zip"));
    assert!(dir.join("2017-07-01.zip").exists());
}

#[test]
fn cancel_stops_download() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/2017-07-01.zip")
        .with_chunked_body(|w| {
            w.write_all(&[0u8; 1024])?;
            w.flush()?;
            std::thread::sleep(Duration::from_secs(4));
            Ok(())
        })
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let (c, rx, cancel) = ctx();
    let r = req(&server.url(), &dir);
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::Relaxed);
    });
    let t = Instant::now();
    run(&c, data.path(), &r, &FakeDisk(60 * GB));
    assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
    let ev = events(&rx);
    assert!(done(&ev).is_none() && failed(&ev).is_none(), "{ev:?}");
    assert!(!dir.join("2017-07-01.zip").exists());
    assert!(!dir.join("2017-07-01.zip.part").exists());
}

#[test]
fn start_disabled_without_url_or_disclaimer() {
    let mut d = HistoryUrlDlg::new(&Settings::default().analysis);
    assert!(!d.can_start());
    d.url = "https://example.invalid/archive".into();
    assert!(!d.can_start(), "disclaimer not accepted");
    d.accepted = true;
    assert!(d.can_start());
    d.url = "   ".into();
    assert!(!d.can_start());
}

const URL_SECRETS: &str = "u:secret@";

fn texts(ev: &[JobEvent]) -> Vec<&str> {
    ev.iter()
        .filter_map(|e| match e {
            JobEvent::Progress(s) | JobEvent::Done(s) | JobEvent::Failed(s) => Some(s.as_str()),
            _ => None,
        })
        .collect()
}

fn assert_clean(text: &str) {
    for bad in ["secret", "token", "u:", URL_SECRETS] {
        assert!(!text.contains(bad), "{bad} in {text}");
    }
}

#[test]
fn userinfo_and_query_are_redacted_everywhere() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/p/2017-07-01.zip")
        .match_query(mockito::Matcher::Any)
        .with_status(403)
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let base = server
        .url()
        .replace("http://", "http://u:secret@")
        .to_string()
        + "/p?token=x";
    let r = req(&base, &dir);
    assert_clean(&format!("{r:?}"));
    let ev = run_job(data.path(), &r, 60 * GB);
    assert!(failed(&ev).unwrap().contains("403"));
    for t in texts(&ev) {
        assert_clean(t);
    }
    // a user name without a password too
    let one = req("https://u@h/p", &dir);
    assert!(!format!("{one:?}").contains("u@"));
}

#[test]
fn local_failure_keeps_a_fresh_zip() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/2017-07-01.zip")
        .with_body(two_step_zip())
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    // a directory where the csv goes: creating it fails after the download
    std::fs::create_dir_all(dir.join("2017-07-01.csv")).unwrap();
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    assert!(failed(&ev).unwrap().contains("cannot write"), "{ev:?}");
    assert!(dir.join("2017-07-01.zip").exists());
}

#[test]
fn unreadable_fresh_zip_is_deleted() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/2017-07-01.zip")
        .with_body("not a zip")
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let ev = run_job(data.path(), &req(&server.url(), &dir), 60 * GB);
    assert!(failed(&ev).unwrap().contains("zip"));
    assert!(!dir.join("2017-07-01.zip").exists());
}

#[test]
fn stalled_server_ends_the_download_with_an_error() {
    let mut server = mockito::Server::new();
    server
        .mock("GET", "/2017-07-01.zip")
        .with_chunked_body(|w| {
            w.write_all(&[0u8; 1024])?;
            w.flush()?;
            std::thread::sleep(Duration::from_secs(4));
            Ok(())
        })
        .create();
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join("h");
    let (c, rx, _) = ctx();
    let t = Instant::now();
    run_with_stall(
        &c,
        data.path(),
        &req(&server.url(), &dir),
        &FakeDisk(60 * GB),
        Duration::from_millis(400),
    );
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    let ev = events(&rx);
    assert!(failed(&ev).unwrap().contains("stalled"), "{ev:?}");
    assert!(!dir.join("2017-07-01.zip.part").exists());
}
