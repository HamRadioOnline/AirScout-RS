//! Signal level database (`SignalDatabase.cs`) and the Spectrum Lab parts of
//! `MapDlg.cs:7620-7710`.
use as_signals::{parse_speclab_fft, speclab_url, SignalDb, SignalError};
use chrono::{DateTime, Duration, Utc};

fn t(s: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(s, 0).unwrap()
}

#[test]
fn find_exact_second_only() {
    let db = SignalDb::open_in_memory().unwrap();
    db.insert_or_update_if_newer(-87.5, t(1_795_438_420))
        .unwrap();
    assert_eq!(db.find(t(1_795_438_420)).unwrap(), Some(-87.5));
    // stored as unix seconds: a time within the same second finds it
    assert_eq!(
        db.find(t(1_795_438_420) + Duration::milliseconds(900))
            .unwrap(),
        Some(-87.5)
    );
    assert_eq!(db.find(t(1_795_438_421)).unwrap(), None);
    assert_eq!(db.find(t(1_795_438_419)).unwrap(), None);
}

#[test]
fn insert_or_update_if_newer_by_second() {
    let db = SignalDb::open_in_memory().unwrap();
    assert_eq!(db.insert_or_update_if_newer(-80.0, t(100)).unwrap(), 1);
    // same whole second: the stored time is not older, nothing changes
    assert_eq!(db.insert_or_update_if_newer(-70.0, t(100)).unwrap(), 0);
    assert_eq!(db.find(t(100)).unwrap(), Some(-80.0));
    // a sub-second time is "newer" than the stored whole second
    let later = t(100) + Duration::milliseconds(500);
    assert_eq!(db.insert_or_update_if_newer(-60.0, later).unwrap(), 1);
    assert_eq!(db.find(t(100)).unwrap(), Some(-60.0));
}

#[test]
fn bulk_insert_and_count() {
    let db = SignalDb::open_in_memory().unwrap();
    let rows: Vec<(f64, DateTime<Utc>)> =
        (0..61).map(|i| (-90.0 + i as f64, t(1000 + i))).collect();
    assert_eq!(db.bulk_insert(&rows).unwrap(), 0);
    assert_eq!(db.count().unwrap(), 61);
    // duplicates fail one by one: C# returns -errors and keeps the rest
    let more = [(-1.0, t(1000)), (-2.0, t(2000)), (-3.0, t(1001))];
    assert_eq!(db.bulk_insert(&more).unwrap(), -2);
    assert_eq!(db.count().unwrap(), 62);
    assert_eq!(db.find(t(1000)).unwrap(), Some(-90.0));
    // the poller's bulk path
    assert_eq!(
        db.bulk_insert_or_update_if_newer(&[(5.0, t(1000)), (6.0, t(3000))])
            .unwrap(),
        2
    );
    assert_eq!(db.count().unwrap(), 63);
    assert_eq!(db.find(t(1000)).unwrap(), Some(-90.0));
    // housekeeping
    assert_eq!(db.delete_older_than(t(1060)).unwrap(), 60);
    assert_eq!(db.count().unwrap(), 3);
}

#[test]
fn signals_file_persists() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = SignalDb::open(dir.path()).unwrap();
        db.insert_or_update_if_newer(-50.0, t(42)).unwrap();
    }
    assert!(dir.path().join("signals.db3").exists());
    let db = SignalDb::open(dir.path()).unwrap();
    assert_eq!(db.find(t(42)).unwrap(), Some(-50.0));
}

#[test]
fn speclab_url_joins_with_ampersand() {
    // C# joins the second parameter with '?' (GUI bug, not reproduced; logic-findings #78)
    assert_eq!(
        speclab_url("http://localhost:10080", "_fft_expt.json", 400, 1600),
        "http://localhost:10080/_fft_expt.json?f1=400&f2=1600"
    );
    // clamping as in the poller: f1 in 0..=3000, f2 in 1..=3000, f1 < f2
    assert_eq!(
        speclab_url("http://h/", "x.json", -5, 5000),
        "http://h/x.json?f1=0&f2=3000"
    );
    assert_eq!(
        speclab_url("http://h/", "x.json", 2000, 1000),
        "http://h/x.json?f1=999&f2=1000"
    );
}

/// A Spectrum Lab `_fft_expt.json` as the C# parser reads it: the header is split at `:`
/// and `,`, item 1 is the unix time, item 13 the bin count; then the bin array.
fn fft(bins: &str) -> String {
    format!(
        "[{{\"t_unix\":1795438420.75,\"f0\":0.0,\"df\":5.3833,\"ch\":1,\"f1\":400,\"f2\":1600,\"nbins\":8,\"unit\":\"dB\"}},\r\n[{bins}]\r\n]"
    )
}

#[test]
fn parse_fft_max() {
    let (at, max) =
        parse_speclab_fft(&fft("-87.5,-90.1,-60.25,-75.0,-99.9,-88.8,-70.5,-95.1")).unwrap();
    // the fractional seconds are truncated
    assert_eq!(at, t(1_795_438_420));
    assert_eq!(max, -60.25);
}

#[test]
fn parse_fft_drops_last_char() {
    // `Remove(IndexOf("]") - 1)` cuts the character before ']': the last bin -5.5 is
    // read as -5
    let (_, max) = parse_speclab_fft(&fft("-87.5,-90.1,-5.5")).unwrap();
    assert_eq!(max, -5.0);
    // with a space before ']' only the space goes
    let (_, max) = parse_speclab_fft(&fft("-87.5,-90.1,-5.5 ")).unwrap();
    assert_eq!(max, -5.5);
}

#[test]
fn parse_fft_errors() {
    let bad = [
        String::new(),
        "{\"t\":1}".to_string(),
        "[{\"t_unix\":1795438420}]".to_string(),
        fft("-87.5,abc,-1.0"),
        fft("-87.5,,-1.0"),
        fft("1.0,2.0 ").replace("\"nbins\":8", "\"nbins\":\"x\""),
        // no character before ']'
        fft(""),
        "[{\"t_unix\":\"x\",\"f0\":0,\"df\":1,\"ch\":1,\"f1\":1,\"f2\":2,\"nbins\":2,\"u\":1},[1.0,2.0]]"
            .to_string(),
    ];
    for b in bad {
        assert!(
            matches!(parse_speclab_fft(&b), Err(SignalError::Parse(_))),
            "{b:?}"
        );
    }
}
