//! The app side of the Win-Test commands (`MapDlg.cs:7078-7536`): show a path, sync, add and
//! remove watchlist entries. The worker already validated and resolved the datagrams.

use super::*;
use crate::wintest_service::{ShowPath, WinTestCommand};
use crate::wintest_watch::{add_watch, remove_watch, sync_watchlist};
use as_geo::latlon::bearing;

/// Antenna height the C# sets for a path of ASSHOWPATH (m).
const SHOW_PATH_HEIGHT_M: f64 = 10.0;

impl AppState {
    /// A command of the running receiver (the caller checked the generation).
    pub(super) fn on_wintest(&mut self, cmd: WinTestCommand) {
        let my = GPoint::new(self.settings.my_lat, self.settings.my_lon);
        let max_km = self.settings.path_max_length_km;
        let max = self.settings.watchlist_max_count;
        match cmd {
            WinTestCommand::ShowPath(p) => self.wintest_show_path(p),
            // upstream does not refresh the paths here
            WinTestCommand::Watchlist(e) => {
                sync_watchlist(&mut self.settings.watchlist, &e, my, max_km, max);
            }
            WinTestCommand::AddWatch(e) => {
                add_watch(&mut self.settings.watchlist, &e, my, max_km, max);
                self.watchlist_changed();
                if self.life_mode == LifeMode::Life
                    && !self.is_playing()
                    && self.settings.watchlist.0.iter().any(|i| i.checked)
                {
                    let _ = self.play();
                }
            }
            WinTestCommand::RemoveWatch(r) => {
                remove_watch(&mut self.settings.watchlist, &r);
                self.watchlist_changed();
                if self.life_mode == LifeMode::Life
                    && self.is_playing()
                    && !self.settings.watchlist.0.iter().any(|i| i.checked)
                {
                    self.pause();
                }
            }
        }
    }

    /// `UpdatePaths` of the port: MULTI re-sends the paths on the next tick.
    fn watchlist_changed(&mut self) {
        if self.is_playing() && self.path_mode() == PathMode::Multi {
            self.watchlist_dirty = true;
        }
    }

    /// ASSHOWPATH: pause, show the path in SINGLE and play (not in HISTORY).
    fn wintest_show_path(&mut self, p: ShowPath) {
        if self.is_playing() {
            self.pause();
        }
        self.control_tab = ControlTab::Single;
        let s = &mut self.settings;
        s.my_call = p.my_call;
        s.my_lat = p.my.lat;
        s.my_lon = p.my.lon;
        s.my_height_m = SHOW_PATH_HEIGHT_M;
        s.dx_call = p.dx_call;
        s.dx_lat = p.dx.lat;
        s.dx_lon = p.dx.lon;
        s.dx_height_m = SHOW_PATH_HEIGHT_M;
        if let Some(b) = p.band {
            s.band = b;
        }
        s.track.set_az = bearing(p.my.lat, p.my.lon, p.dx.lat, p.dx.lon);
        s.track.set_el = 0.0;
        self.sync_loc_texts();
        self.sync_my();
        self.alarm_focus = true;
        // HISTORY plays from its own controls (finding)
        if self.life_mode == LifeMode::Life {
            let _ = self.play();
        }
    }
}
