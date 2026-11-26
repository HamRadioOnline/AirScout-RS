//! Tracking state (`bw_Track`, `MapDlg.cs:8054-8315`) and its mode transitions.
//!
//! The Track worker (`track_service`) owns the step clock: it steps the input the UI pushes
//! (`Services::track_input`) once a second, drives the outputs and the rig's Doppler target, and
//! keeps running while the window is minimised. The UI only pushes a changed input and shows what
//! the worker showed for it (`Services::track_shown`).

use super::*;
use crate::tracker::{TrackInput, TrackParams, TrackShown};
use crate::tracking::TrackStatus;
use crate::ui::map_geom::toggle_selection;
use as_aircraft::enums::TrackMode;

/// Tracking state that is not shown: the station heights and the input revisions.
#[derive(Default)]
pub(super) struct TrackRt {
    /// Id of the current Play's elevation request (counts up per Play).
    gen: u64,
    requested: bool,
    /// Heights above sea level [m] of My and DX, once the service answered.
    heights: Option<(f64, f64)>,
    /// Revision of the last pushed input; counts up per push and survives Play.
    rev: u64,
    /// The last pushed input.
    pushed: Option<TrackInput>,
    /// The worker's answer applied last, by [`shown_key`].
    applied: Option<ShownKey>,
}

/// Tells one worker step's answer from the next: a step with values carries its own timestamp
/// (`now + Track_Offset`). `TrackShown` itself is no key: values that are not computed are NaN,
/// and NaN never compares equal.
type ShownKey = (u64, TrackStatus, Option<DateTime<Utc>>);

fn shown_key(sh: &TrackShown) -> ShownKey {
    (sh.rev, sh.status, sh.values.as_ref().map(|v| v.timestamp))
}

impl AppState {
    /// A click on a plane starts TRACK only while playing in LIFE.
    fn track_running(&self) -> bool {
        self.is_playing() && self.life_mode == LifeMode::Life
    }

    /// `TrackMode = NONE` (Pause `:4099`, Play, no selected plane). The status falls back to
    /// STOPPED, which the C# loop reports for values that are not valid.
    pub(super) fn track_stop(&mut self) {
        self.track_mode = TrackMode::None;
        self.track_status = if self.settings.track.activate {
            TrackStatus::Stopped
        } else {
            TrackStatus::None
        };
    }

    /// Play: no tracking yet; the station heights are read again at the first step.
    pub(super) fn track_reset(&mut self) {
        self.track_stop();
        // the revisions go on: an answer for an input of the last Play must not match
        let rt = &mut self.track_rt;
        rt.gen += 1;
        rt.requested = false;
        rt.heights = None;
    }

    /// The values the plane tooltips and the readout show: only while `TrackMode` is TRACK
    /// (`MapDlg.cs:4913`, `:5604`).
    pub fn tooltip_track(&self) -> Option<&crate::tracking::TrackValues> {
        (self.track_mode == TrackMode::Track)
            .then_some(self.track.as_ref())
            .flatten()
    }

    /// Click on a plane in the map (`MapDlg.cs:5478-5510`). With tracking activated and playing
    /// the mode becomes TRACK, even if the click deselected the plane: the next tick
    /// (`track_unselected`) resets it. Without activated tracking a deselect resets it at once.
    pub fn select_plane(&mut self, hex: &str) {
        let was_selected = self.selected.as_deref() == Some(hex);
        toggle_selection(&mut self.selected, hex);
        if self.settings.track.activate {
            if self.track_running() {
                self.track_mode = TrackMode::Track;
            }
        } else if was_selected {
            self.track_mode = TrackMode::None;
        }
    }

    /// Right-click on the DX marker (`MapDlg.cs:5533-5546`): point the antenna at the DX station
    /// once (SINGLE with `Track_SetAz = QTF`, `Track_SetEl = 0`).
    pub fn right_click_dx(&mut self) {
        let s = &mut self.settings;
        let qtf = as_geo::latlon::bearing(s.my_lat, s.my_lon, s.dx_lat, s.dx_lon);
        self.track_mode = TrackMode::Single;
        s.track.set_az = qtf;
        s.track.set_el = 0.0;
    }

    /// End of `DrawPlanes` (`MapDlg.cs:5086-5089`): no selected plane among the shown ones stops
    /// tracking. `DrawPlanes` returns early without planes (`:4964`), which leaves the mode.
    pub(super) fn track_unselected(&mut self) {
        if self.tick.active.is_empty() {
            return;
        }
        let any = self
            .selected
            .as_deref()
            .is_some_and(|h| self.tick.shown.iter().any(|p| p.hex == h));
        if !any {
            self.track_mode = TrackMode::None;
        }
    }

    /// Every frame: push the input of the `bw_Track` inner loop (`MapDlg.cs:8054-8315`) to the
    /// Track worker when it changed, under a new revision, and show the worker's answer for the
    /// current revision. The C# runs `bw_Track` whenever `Track_Activate` is on, so the worker
    /// steps while playing or paused, in LIFE only (the C# stops it in HISTORY). SINGLE set while
    /// paused is processed every second until Play or Pause resets it (1:1).
    pub fn track_sync(&mut self) {
        self.track_push();
        // an answer for an older input, or one already shown, changes nothing (so the ERROR of
        // a switched-off output holds until the input or the worker's answer changes)
        let Some(sh) = self.services.track_shown() else {
            return;
        };
        let key = shown_key(&sh);
        if sh.rev != self.track_rt.rev || self.track_rt.applied == Some(key) {
            return;
        }
        self.track_status = sh.status;
        if sh.values.is_some() {
            self.track = sh.values;
        }
        self.track_rt.applied = Some(key);
    }

    /// Push the input to the Track worker when it changed, under a new revision. Also called
    /// before the integrations start, so a new worker's first (immediate) step reads the current
    /// input, not the one from before the Options dialog.
    pub(super) fn track_push(&mut self) {
        let mut input = self.track_input();
        input.rev = self.track_rt.rev;
        if self.track_rt.pushed.as_ref() != Some(&input) {
            self.track_rt.rev += 1;
            input.rev = self.track_rt.rev;
            self.services.track_input(input.clone());
            self.track_rt.pushed = Some(input);
        }
    }

    /// The snapshot a step reads. Asks the service for the station heights once per Play, only
    /// while LIFE, `Track_Activate` and TRACK hold (only TRACK reads them, `:8080-8083`).
    pub(super) fn track_input(&mut self) -> TrackInput {
        let life = self.life_mode == LifeMode::Life;
        let active = self.settings.track.activate;
        let mode = self.track_mode;
        if life && active && mode == TrackMode::Track {
            self.request_station_heights();
        }
        TrackInput {
            rev: self.track_rt.rev,
            life,
            mode,
            hex: self.selected.clone(),
            heights: self.track_rt.heights,
            params: active.then(|| TrackParams::from_settings(&self.settings)),
        }
    }

    /// Heights above sea level [m] of My and DX are ground elevation plus antenna height
    /// (`:8082-8083`); the service looks the elevations up. The C# reads the elevation every
    /// second; the port asks a worker once per Play.
    fn request_station_heights(&mut self) {
        if !self.track_rt.requested {
            self.track_rt.requested = true;
            let s = &self.settings;
            let (my, dx) = (
                GPoint::new(s.my_lat, s.my_lon),
                GPoint::new(s.dx_lat, s.dx_lon),
            );
            self.services
                .request_ground_elevations(self.track_rt.gen, my, dx);
        }
    }

    /// The service's answer; one of an older Play is dropped.
    pub(super) fn on_track_heights(&mut self, id: u64, my_m: f64, dx_m: f64) {
        if id == self.track_rt.gen {
            let s = &self.settings;
            self.track_rt.heights = Some((my_m + s.my_height_m, dx_m + s.dx_height_m));
        }
    }
}
