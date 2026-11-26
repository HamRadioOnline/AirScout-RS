//! `export`: the tiles of a `.gmdb` as a `<zoom>/<x>/<y>.png` tree (`btn_Start_Click`,
//! `MapTileGenerator/MainDlg.cs:53-89`).
//!
//! A file that exists with the tile's time is skipped, as in C#. Deviations from the C#: a tile
//! without data is skipped with a message (C# `:69` reads `d != null && !File.Exists(f) ||
//! GetLastWriteTime(f) != CacheTime`, so with no data and a missing file or another time it
//! throws a null reference at `:73`); a db with several `Type`s needs `--type` (C# writes them
//! all into one tree, where the providers overwrite each other); a `CacheTime` that is no file
//! time (outside the years 1601 to 9999) is not set as the mtime.

use crate::{type_name, write_via_part, Error};
use as_maps::{TileInfo, TileStore};
use chrono::{DateTime, Datelike, Local, NaiveDateTime, TimeDelta, TimeZone, Utc};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

enum Outcome {
    Written,
    Unchanged,
    NoData,
}

/// The instant of a local `CacheTime` (the earlier one when the clock is set back). A time in
/// the gap when the clock is set forward is taken one hour later.
pub(crate) fn local_instant(t: NaiveDateTime) -> Option<DateTime<Local>> {
    Local.from_local_datetime(&t).earliest().or_else(|| {
        let later = t.checked_add_signed(TimeDelta::hours(1))?;
        Local.from_local_datetime(&later).earliest()
    })
}

/// The years a file time may have: Windows `SystemTime` starts in 1601 and ends near 30828,
/// chrono's local-time conversion has limits too.
fn file_year(t: &DateTime<Utc>) -> bool {
    (1601..=9999).contains(&t.year())
}

/// A `SystemTime` (such as a file's mtime) as UTC; `None` outside [`file_year`]. Never panics,
/// unlike chrono's `From<SystemTime>`.
pub(crate) fn utc_of(t: SystemTime) -> Option<DateTime<Utc>> {
    let (sec, nsec) = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => (i64::try_from(d.as_secs()).ok()?, d.subsec_nanos()),
        Err(e) => {
            let d = e.duration();
            let s = i64::try_from(d.as_secs()).ok()?;
            match d.subsec_nanos() {
                0 => (-s, 0),
                n => (s.checked_neg()?.checked_sub(1)?, 1_000_000_000 - n),
            }
        }
    };
    DateTime::from_timestamp(sec, nsec).filter(file_year)
}

/// A local `CacheTime` as a file time; `None` when it is no local time or outside
/// [`file_year`]. Checked arithmetic, so no `SystemTime` overflow on any OS.
fn file_time(t: NaiveDateTime) -> Option<SystemTime> {
    // The year is checked first, so the local-time conversion never nears chrono's limits.
    if !(1601..=9999).contains(&t.year()) {
        return None;
    }
    let u = local_instant(t)?.with_timezone(&Utc);
    if !file_year(&u) {
        return None;
    }
    let secs = u.timestamp();
    let whole = if secs >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(secs.unsigned_abs()))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(secs.unsigned_abs()))
    }?;
    whole.checked_add(Duration::from_nanos(u64::from(u.timestamp_subsec_nanos())))
}

/// `n tile` / `n tiles`.
pub(crate) fn plural(n: u64, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// `true` when `file` is a file whose mtime is `when` to the whole second (file systems keep
/// from 100 ns to 2 s).
fn same_time(file: &Path, when: SystemTime) -> bool {
    let Some(when) = utc_of(when) else {
        return false;
    };
    fs::metadata(file)
        .and_then(|m| {
            if m.is_file() {
                m.modified()
            } else {
                Err(std::io::ErrorKind::Other.into())
            }
        })
        .is_ok_and(|m| utc_of(m).is_some_and(|m| m.timestamp() == when.timestamp()))
}

fn export_tile(store: &TileStore, tile: &TileInfo, file: &Path) -> Result<Outcome, Error> {
    // A missing, unreadable or out-of-range CacheTime: the file is written without setting
    // its mtime.
    let when = match tile.cache_time {
        Some(t) => {
            let when = file_time(t);
            if when.is_none() {
                println!(
                    "cache time {t} of {} is out of range; its mtime is not set",
                    file.display()
                );
            }
            when
        }
        None => None,
    };
    if when.is_some_and(|w| same_time(file, w)) {
        return Ok(Outcome::Unchanged);
    }
    let Some(data) = store.get_by_id(tile.id)? else {
        return Ok(Outcome::NoData);
    };
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    write_via_part(file, &data, when)?;
    Ok(Outcome::Written)
}

pub(crate) fn run(db: &Path, out: &Path, db_id: Option<i32>) -> Result<(), Error> {
    let store = TileStore::open_read_only(db)?;
    let types = store.types()?;
    if db_id.is_none() && types.len() > 1 {
        let list: Vec<String> = types
            .iter()
            .map(|&(t, n)| format!("{}: {}", type_name(t), plural(n, "tile")))
            .collect();
        return Err(format!(
            "{} holds tiles of more than one type ({}); choose one with --type",
            db.display(),
            list.join(", ")
        )
        .into());
    }
    let rows = store.count()?;
    let all = store.list()?;
    // Rows `list` cannot turn into a tile key (negative, too large, not a number).
    let no_key = rows.saturating_sub(all.len() as u64);
    let tiles: Vec<TileInfo> = all
        .into_iter()
        .filter(|t| db_id.is_none_or(|d| t.key.db_id == d))
        .collect();
    let n = tiles.len();
    let (mut written, mut unchanged, mut skipped) = (0u64, 0u64, no_key);
    for (i, tile) in tiles.iter().enumerate() {
        let i = i + 1;
        let k = tile.key;
        let file = out
            .join(k.zoom.to_string())
            .join(k.x.to_string())
            .join(format!("{}.png", k.y));
        let shown = file.display();
        match export_tile(&store, tile, &file) {
            Ok(Outcome::Written) => {
                written += 1;
                println!("Saving [{i} of {n}] {shown}");
            }
            Ok(Outcome::Unchanged) => {
                unchanged += 1;
                println!("Saving [{i} of {n}] {shown}");
            }
            Ok(Outcome::NoData) => {
                skipped += 1;
                println!("skipped [{i} of {n}] {shown}: no data (id {})", tile.id);
            }
            Err(e) => {
                skipped += 1;
                println!("skipped [{i} of {n}] {shown}: {e}");
            }
        }
    }
    if no_key > 0 {
        let verb = if no_key == 1 { "is" } else { "are" };
        println!(
            "skipped: {} {verb} no tile (X, Y, Zoom or Type out of range; Tiles has {})",
            plural(no_key, "row"),
            plural(rows, "row")
        );
    }
    println!("written {written}, unchanged {unchanged}, skipped {skipped}");
    println!("Ready.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(y: i32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, 7, 14)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
    }

    #[test]
    fn file_time_is_none_outside_1601_to_9999() {
        assert!(file_time(at(30_900)).is_none());
        assert!(file_time(at(1500)).is_none());
        assert!(file_time(NaiveDateTime::MAX).is_none());
        assert!(file_time(NaiveDateTime::MIN).is_none());
        let t = file_time(at(2019)).unwrap();
        assert_eq!(
            utc_of(t).unwrap().timestamp(),
            local_instant(at(2019)).unwrap().timestamp()
        );
        // Before 1970 works too.
        assert!(file_time(at(1700)).is_some_and(|t| t < UNIX_EPOCH));
    }

    #[test]
    fn utc_of_never_panics() {
        assert_eq!(utc_of(UNIX_EPOCH).unwrap().timestamp(), 0);
        let half = UNIX_EPOCH - Duration::from_millis(500);
        assert_eq!(utc_of(half).unwrap().timestamp(), -1);
        // Far outside chrono's and Windows' range, where the platform allows it.
        for far in [
            UNIX_EPOCH.checked_add(Duration::from_secs(1 << 50)),
            UNIX_EPOCH.checked_sub(Duration::from_secs(1 << 50)),
        ]
        .into_iter()
        .flatten()
        {
            assert!(utc_of(far).is_none());
        }
    }
}
