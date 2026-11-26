//! The HISTORY player: step width buttons, time advance and the scrollbar markers
//! (`MapDlg.cs:6865-6906`, `5370-5395`, `6839-6855`).

use chrono::{DateTime, Duration, Utc};

/// The buttons of the player. Rewind and Fast-forward change the step width by 10 s, Back and
/// Forward by 1 s, Pause stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerButton {
    Rewind,
    Back,
    Pause,
    Forward,
    FastForward,
}

/// Largest step width in seconds, in both directions.
pub const MAX_INTERVAL_S: i64 = 3600;

/// New step width (`Time_Offline_Interval`) after a click. A negative width plays backwards.
///
/// Not reproduced (GUI): the original clamps Back and Rewind to at least 1 (MapDlg.cs:6869-6883),
/// so they never play backwards.
pub fn step(interval_s: i64, b: PlayerButton) -> i64 {
    let delta = match b {
        PlayerButton::Rewind => -10,
        PlayerButton::Back => -1,
        PlayerButton::Pause => return 0,
        PlayerButton::Forward => 1,
        PlayerButton::FastForward => 10,
    };
    (interval_s + delta).clamp(-MAX_INTERVAL_S, MAX_INTERVAL_S)
}

/// The next offline time: `t + interval`, clamped to `[min, max]` (MapDlg.cs:5372-5376).
pub fn advance(
    t: DateTime<Utc>,
    interval_s: i64,
    min: DateTime<Utc>,
    max: DateTime<Utc>,
) -> DateTime<Utc> {
    (t + Duration::seconds(interval_s)).max(min).min(max)
}

/// One flag per pixel of the scrollbar: true if a report lies in that pixel. Markers outside
/// `[min, max]` are dropped. With `min == max` all markers at that instant fall into bin 0; the
/// original divides by `Max - Min` there (MapDlg.cs:6839-6855).
pub fn marker_bins(
    markers: &[DateTime<Utc>],
    min: DateTime<Utc>,
    max: DateTime<Utc>,
    width_px: usize,
) -> Vec<bool> {
    let mut bins = vec![false; width_px];
    if width_px == 0 {
        return bins;
    }
    let span = (max - min).num_milliseconds();
    for m in markers.iter().filter(|m| **m >= min && **m <= max) {
        let i = if span <= 0 {
            0
        } else {
            let off = (*m - min).num_milliseconds() as f64 / span as f64;
            (off * (width_px - 1) as f64).round() as usize
        };
        bins[i.min(width_px - 1)] = true;
    }
    bins
}
