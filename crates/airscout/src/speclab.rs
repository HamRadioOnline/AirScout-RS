//! Spectrum Lab poller (`bw_SpecLab_Receive`, MapDlg.cs:7630-7728) and the spectrum the bottom
//! tab draws (`ProgressChanged`, MapDlg.cs:7732-7826).
//!
//! The poller asks the user's own Spectrum Lab server (the URL in the settings) for the FFT of
//! the configured band every `SpecLab_Update` seconds and reports the maximum level. While the
//! `record` flag is set (the app sets it while a plane is nearest), it also stores the levels in
//! the signal database, in batches of [`BULK_COUNT`]. Nothing but the request itself is sent: no
//! call sign, no position.
//!
//! Deviations from the original, none of which changes a stored level:
//! - the first poll runs at once, the original sleeps first;
//! - a failed poll is reported (`SpectrumError`); the original swallows web and socket errors;
//! - the batch is written at 60 samples, the original at 61 (`Count > maxcount`);
//! - the buffer is also written when the poller is stopped (Pause). The original's worker lives as
//!   long as the app, so its buffer survives Pause/Play; the port starts a new poller on every Play
//!   and must not lose the recorded levels (logic-findings #81).

use crate::background::sleep_cancellable;
use crate::services::ServiceEvent;
use as_settings::SpecLab;
use as_signals::{parse_speclab_fft, speclab_url, SignalDb};
use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

/// Levels kept for the plot (`SpectrumMaxPoints`).
pub const SPECTRUM_MAX_POINTS: usize = 600;
/// Samples collected before one batch is written (`maxcount`).
pub const BULK_COUNT: usize = 60;
/// Y range of an empty plot [dB].
const EMPTY_Y: (f64, f64) = (-120.0, 0.0);
/// Smallest Y span [dB]: one sample or constant levels must not give a flat plot.
const MIN_SPAN: f64 = 10.0;

/// Handle of a running poller. [`stop`](Self::stop) cancels it; a thread inside a request ends
/// when the request returns and reports nothing from the moment of the cancel.
pub struct SpecLabService {
    cancel: Arc<AtomicBool>,
}

impl SpecLabService {
    /// Start polling every `max(update_s, 1)` s. The app starts it with Play and stops it with
    /// Pause.
    pub fn start(
        settings: &SpecLab,
        signals: Arc<SignalDb>,
        record: Arc<AtomicBool>,
        tx: Sender<ServiceEvent>,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        let interval = Duration::from_secs(settings.update_s.max(1));
        Self::start_with_interval(settings, signals, record, tx, cancel, interval)
    }

    /// As [`start`](Self::start) with an explicit poll interval (tests).
    pub fn start_with_interval(
        settings: &SpecLab,
        signals: Arc<SignalDb>,
        record: Arc<AtomicBool>,
        tx: Sender<ServiceEvent>,
        cancel: Arc<AtomicBool>,
        interval: Duration,
    ) -> Self {
        let url = speclab_url(
            &settings.url,
            &settings.file_name,
            i32::try_from(settings.f1).unwrap_or(i32::MAX),
            i32::try_from(settings.f2).unwrap_or(i32::MAX),
        );
        let c = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("speclab".into())
            .spawn(move || poll_loop(&url, &signals, &record, &tx, &c, interval));
        if spawned.is_err() {
            cancel.store(true, Ordering::Relaxed);
        }
        Self { cancel }
    }

    pub fn stop(self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for SpecLabService {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn fetch(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.text().map_err(|e| e.to_string())
}

fn poll_loop(
    url: &str,
    signals: &SignalDb,
    record: &AtomicBool,
    tx: &Sender<ServiceEvent>,
    cancel: &AtomicBool,
    interval: Duration,
) {
    let client = match reqwest::blocking::Client::builder()
        .user_agent(as_net::user_agent())
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(ServiceEvent::SpectrumError(format!("HTTP client: {e}")));
            return;
        }
    };
    let mut buffer: Vec<(f64, DateTime<Utc>)> = Vec::new();
    poll(
        &client,
        url,
        signals,
        record,
        tx,
        cancel,
        interval,
        &mut buffer,
    );
    // stopped: keep what was recorded
    if !buffer.is_empty() {
        if let Err(e) = signals.bulk_insert_or_update_if_newer(&buffer) {
            let _ = tx.send(ServiceEvent::SpectrumError(format!("signal database: {e}")));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn poll(
    client: &reqwest::blocking::Client,
    url: &str,
    signals: &SignalDb,
    record: &AtomicBool,
    tx: &Sender<ServiceEvent>,
    cancel: &AtomicBool,
    interval: Duration,
    buffer: &mut Vec<(f64, DateTime<Utc>)>,
) {
    loop {
        let event = match fetch(client, url)
            .and_then(|json| parse_speclab_fft(&json).map_err(|e| e.to_string()))
        {
            Ok((at, level)) => {
                if record.load(Ordering::Relaxed) {
                    buffer.push((level, at));
                    if buffer.len() >= BULK_COUNT {
                        let r = signals.bulk_insert_or_update_if_newer(buffer);
                        buffer.clear();
                        if let Err(e) = r {
                            let _ = tx
                                .send(ServiceEvent::SpectrumError(format!("signal database: {e}")));
                        }
                    }
                }
                ServiceEvent::Spectrum { at, level }
            }
            Err(e) => ServiceEvent::SpectrumError(format!("Spectrum Lab: {e}")),
        };
        // a request cannot be interrupted: after the cancel the result is dropped
        if cancel.load(Ordering::Relaxed) || tx.send(event).is_err() {
            return;
        }
        if sleep_cancellable(interval, cancel) {
            return;
        }
    }
}

/// What the Spectrum tab shows: the last [`SPECTRUM_MAX_POINTS`] levels, which of them were taken
/// while recording, and the status line.
#[derive(Debug, Default)]
pub struct SpectrumState {
    samples: VecDeque<(f64, bool)>,
    /// Text of the status line.
    pub status: String,
    /// The status is an error (shown white on red in the original).
    pub is_error: bool,
    /// Set by the app to `tick.nearest.is_some()`; read by the poller.
    pub record: Arc<AtomicBool>,
}

impl SpectrumState {
    /// Add a level taken while `recorded`; the oldest one drops out beyond 600.
    pub fn push(&mut self, level: f64, recorded: bool) {
        if self.samples.len() >= SPECTRUM_MAX_POINTS {
            self.samples.pop_front();
        }
        self.samples.push_back((level, recorded));
    }

    /// `ReportProgress(0)`: a new poller starts, the plot starts empty.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.status.clear();
        self.is_error = false;
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn levels(&self) -> impl Iterator<Item = f64> + '_ {
        self.samples.iter().map(|s| s.0)
    }

    pub fn recorded(&self) -> impl Iterator<Item = bool> + '_ {
        self.samples.iter().map(|s| s.1)
    }

    /// Y axis: -120..0 while empty, else the data range enlarged by 10 % on both sides.
    pub fn y_range(&self) -> (f64, f64) {
        let (lo, hi) = self
            .levels()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), l| {
                (lo.min(l), hi.max(l))
            });
        if lo > hi {
            return EMPTY_Y;
        }
        let d = (hi - lo) * 0.1;
        let (lo, hi) = (lo - d, hi + d);
        if hi - lo >= MIN_SPAN {
            return (lo, hi);
        }
        // widen around the centre, then shift back into -120..0
        let c = (lo + hi) / 2.0;
        let (mut lo, mut hi) = (c - MIN_SPAN / 2.0, c + MIN_SPAN / 2.0);
        if lo < EMPTY_Y.0 {
            (lo, hi) = (EMPTY_Y.0, EMPTY_Y.0 + MIN_SPAN);
        } else if hi > EMPTY_Y.1 {
            (lo, hi) = (EMPTY_Y.1 - MIN_SPAN, EMPTY_Y.1);
        }
        (lo, hi)
    }

    pub fn on_level(&mut self, at: DateTime<Utc>, level: f64) {
        let recorded = self.record.load(Ordering::Relaxed);
        self.push(level, recorded);
        self.status = format!("Spectrum Lab: {level:.1} dB at {}", at.format("%H:%M:%S"));
        self.is_error = false;
    }

    pub fn on_error(&mut self, text: String) {
        self.status = text;
        self.is_error = true;
    }
}
