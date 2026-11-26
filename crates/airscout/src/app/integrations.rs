//! Lifecycle of the integrations (Win-Test receiver, Track worker) and the Track worker's events.
//!
//! The C# stops all background workers when the Options dialog opens (`MapDlg.cs:3893`) and
//! starts them again after OK or Cancel (`:3976`), in LIFE and in HISTORY. ON of HISTORY stops
//! them (`StopAllBackgroundWorkers`, `:6558`) and OFF starts them again (`:6585`). Each start
//! gets a new generation; events of older workers are dropped, so a stopped worker cannot change
//! the ROT or CAT field or the settings. The CAT worker is one of the integrations: its events
//! are handled in `app/cat.rs`.

use super::*;
use crate::track_outputs::OutputGroup;
use crate::track_service::TrackEvent;
use crate::tracking::{RotStatus, TrackStatus};
use crate::wintest_nearest::NearestSettings;
use as_cat::RigStatus;
use as_settings::{TrackDde, TrackFile, TrackSerial, TrackUdp};

/// The shortest time between two settings pushes to the web server.
const WEB_SYNC_PERIOD: Duration = Duration::from_secs(1);

impl AppState {
    /// `StartAllBackgroundWorkers`, integration part: (re)start the receiver and the Track
    /// worker the settings switch on, under a new generation.
    pub fn start_integrations(&mut self) {
        self.reset_integration_state();
        // the new Track worker steps at once from the input cell: give it the current input
        self.track_push();
        self.services
            .start_integrations(&self.settings, self.integration_gen, self.cache.clone());
        self.nearest_synced = Some(NearestSettings::from_settings(&self.settings));
        // the services hand the web server these settings at the start
        self.web_synced = Some((Instant::now(), self.settings.clone()));
    }

    /// `StopAllBackgroundWorkers`, integration part. ROT shows NONE at once, as the C# worker
    /// reports when it ends (`:8342-8343`); the new generation drops its late events.
    pub fn stop_integrations(&mut self) {
        self.services.stop_integrations();
        self.reset_integration_state();
    }

    /// ROT and CAT show NONE at once (the C# workers report it when they end, `:8342-8343`,
    /// `CATWorker.cs:322`) and a new generation drops the late events of the old workers.
    /// Shared by start, stop and exit.
    pub(super) fn reset_integration_state(&mut self) {
        self.rot_status = RotStatus::None;
        self.cat_status = RigStatus::None;
        self.integration_gen += 1;
    }

    /// The Options dialog was closed with OK or Cancel, in LIFE or HISTORY (`:3976`).
    pub fn options_closed(&mut self) {
        self.start_integrations();
    }

    /// Give the running receiver the nearest settings when they changed (band, filters), and
    /// the web server the settings at most once a second. Cheap; called every frame.
    pub fn sync_integrations(&mut self) {
        self.sync_integrations_at(Instant::now());
    }

    /// [`sync_integrations`](Self::sync_integrations) at the frame time `at`.
    pub fn sync_integrations_at(&mut self, at: Instant) {
        if !self.settings.server.activate {
            return;
        }
        let now = NearestSettings::from_settings(&self.settings);
        if self.nearest_synced.as_ref() != Some(&now) {
            self.services.set_nearest_settings(now.clone());
            self.nearest_synced = Some(now);
        }
        self.sync_web_settings(at);
    }

    /// Compares the settings with those the web server got last, once a second at most,
    /// and pushes them when they differ. The routes read them on their next request.
    fn sync_web_settings(&mut self, at: Instant) {
        if let Some((checked, last)) = &mut self.web_synced {
            if at.saturating_duration_since(*checked) < WEB_SYNC_PERIOD {
                return;
            }
            if *last == self.settings {
                *checked = at;
                return;
            }
        }
        self.services.set_web_settings(&self.settings);
        self.web_synced = Some((at, self.settings.clone()));
    }

    /// An event of the Track worker of `generation`; one of an older worker is dropped.
    pub(super) fn on_track_event(&mut self, generation: u64, event: TrackEvent) {
        if generation != self.integration_gen {
            return;
        }
        match event {
            TrackEvent::Rot(s) => self.rot_status = s,
            TrackEvent::Text(t) => self.status = t,
            // the C# switches the output off in the settings (`Track_UDP_None = true`,
            // :8216-8230), so it stays off after a restart
            TrackEvent::Disabled(g) => {
                match g {
                    OutputGroup::Dde => self.settings.track.dde = TrackDde::None,
                    OutputGroup::Udp => self.settings.track.udp = TrackUdp::None,
                    OutputGroup::Serial => self.settings.track.serial = TrackSerial::None,
                    OutputGroup::File => self.settings.track.file = TrackFile::None,
                }
                self.track_status = TrackStatus::Error;
            }
        }
    }
}
