//! `AppState` methods of the watchlist, options, station, obstruction, horizon, set-time and wizard
//! dialogs.

use super::*;
use crate::feed_files::{export_feed, import_feed};
use crate::ui::dialogs::feed_file::{FeedFileKind, FeedFileRequest};
use crate::ui::dialogs::feed_settings::desktop_default_feed;
use crate::ui::options::stations::details_key;

impl AppState {
    /// Check or uncheck a watchlist row. While playing in MULTI the next tick re-sends the paths.
    pub fn set_watch_checked(&mut self, idx: usize, checked: bool) {
        let Some(item) = self.settings.watchlist.0.get_mut(idx) else {
            return;
        };
        if item.checked == checked {
            return;
        }
        item.checked = checked;
        if self.playing_forward() && self.path_mode() == PathMode::Multi {
            self.watchlist_dirty = true;
        }
    }

    /// Open the watchlist dialog and start loading the station list (once per open;
    /// `Location_RestrictToAreaOfInterest` limits it to the area).
    pub fn open_watchlist_dlg(&mut self) {
        let a = &self.settings.area;
        let area = self
            .settings
            .location_restrict_to_area
            .then(|| GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon));
        self.watchlist_dlg = Some(WatchlistDlg::new(
            self.settings
                .watchlist
                .0
                .iter()
                .map(|i| (i.call.clone(), i.loc.clone()))
                .collect(),
        ));
        self.services.load_stations(area);
    }

    /// Open the Options dialog on a copy of the settings; refused while playing (the button is
    /// disabled then, as in the original). The integrations and the background workers stop while
    /// it is open (`MapDlg.cs:3893`, #155); [`Self::options_closed`] starts them again.
    pub fn open_options_dlg(&mut self) -> bool {
        if self.is_playing() || self.options_dlg.is_some() {
            return false;
        }
        let mut dlg = OptionsDlg::new(&self.settings)
            .with_tmp_dir(self.data_dir.as_ref().map(|dir| dir.join("Tmp")))
            .with_tile_dir(self.tile_dir.clone())
            .with_map_store(self.map_store.clone());
        dlg.refresh_qrvs(&mut |call, loc| self.services.qrv_get_all(call, loc));
        self.options_dlg = Some(dlg);
        self.stop_integrations();
        // #155: the calculators, updaters and preloader must not race a delete or clear that is
        // issued from the Options dialog while it is open.
        self.stop_background();
        self.request_sizes();
        true
    }

    /// Execute the file action for the feed selected in the requested slot.
    pub(crate) fn apply_options_feed_file(&self, dlg: &mut OptionsDlg, request: FeedFileRequest) {
        let name = dlg.draft.feed_slot(request.slot).to_owned();
        let status = match request.kind {
            FeedFileKind::Import => {
                const INVALID_SETTINGS: &str =
                    "Cannot import plane feed settings: invalid settings for the selected feed.";
                import_feed(&request.path).and_then(|(imported_name, mut table)| {
                    let mut feed = desktop_default_feed(&name)
                        .ok_or("Cannot import plane feed settings: no feed selected.")?;
                    if imported_name != name {
                        return Err(INVALID_SETTINGS.into());
                    }
                    let supported = feed.settings();
                    if table.keys().any(|key| !supported.contains_key(key)) {
                        return Err(INVALID_SETTINGS.into());
                    }
                    feed.load_settings(&table).map_err(|_| INVALID_SETTINGS)?;
                    as_settings::canonicalize_feed_table(&name, &mut table);
                    dlg.draft.feed_settings.insert(name, table.into());
                    Ok("Plane feed settings imported.")
                })
            }
            FeedFileKind::Export => {
                let stored = dlg
                    .draft
                    .feed_settings
                    .get(&name)
                    .and_then(|value| value.as_table())
                    .filter(|table| !table.is_empty());
                let default = if stored.is_none() {
                    desktop_default_feed(&name).map(|feed| feed.settings())
                } else {
                    None
                };
                stored
                    .or(default.as_ref())
                    .ok_or_else(|| "Cannot export plane feed settings: no feed selected.".into())
                    .and_then(|table| export_feed(&name, table, &request.path))
                    .map(|()| "Plane feed settings exported.")
            }
        };
        dlg.set_feed_file_status(status.map(str::to_owned).unwrap_or_else(|error| error));
    }

    /// OK of the Options dialog: take the new settings, save them. The background workers are
    /// restarted by [`Self::options_closed`] (which both OK and Cancel call), the map rebuilds its
    /// tile layer and the tick reads its interval from the settings, so both follow on the next
    /// frame. The window layout fields and the time (the dialog does not edit it; in HISTORY it is
    /// the replay time) keep their live values.
    ///
    /// In HISTORY the workers stay stopped (finding #115): OFF restarts them.
    pub fn apply_options(&mut self, mut new: Settings) {
        new.time = self.settings.time.clone();
        let g = &self.settings.gui;
        new.gui.window_size = g.window_size;
        new.gui.window_pos = g.window_pos;
        new.gui.side_panel_width = g.side_panel_width;
        new.gui.bottom_panel_height = g.bottom_panel_height;
        new.gui.news_last_update = new.gui.news_last_update.max(g.news_last_update);
        self.settings = new;
        self.sync_loc_texts();
        self.last_tick = None;
        self.sync_services();
        // Options OK may have written station locations (`UpdateWatchlistInMap`, MapDlg.cs:3989).
        self.refresh_watch_marks();
        if let Some(path) = self.settings_path.clone() {
            if let Err(e) = self.save_settings(&path) {
                self.status = format!("cannot save settings: {e}");
            }
        }
    }

    /// Stored QRV entries of a station, for the Options dialog.
    pub fn qrvs_of(&self, call: &str, loc: &str) -> Vec<Qrv> {
        self.services.qrv_get_all(call, loc)
    }

    /// Resolve call edits and refresh the QRV keys. Report changes that need a redraw.
    pub(crate) fn resolve_options_station_calls(&self, dlg: &mut OptionsDlg) -> bool {
        let mut changed = false;
        for who in Who::BOTH {
            let call = who.call(&dlg.draft);
            if call != dlg.stations.edit(who).matched_call {
                let call = call.to_owned();
                let normalized = call.trim().to_ascii_uppercase();
                let position = as_stations::callsign::check(&normalized)
                    .then(|| self.services.location_find(&normalized))
                    .flatten();
                dlg.stations
                    .apply_call_match(&mut dlg.draft, who, &call, position);
                changed = true;
            }
        }
        let keys_changed = dlg.refresh_qrvs(&mut |call, loc| self.qrvs_of(call, loc));
        changed || keys_changed
    }

    /// Read station details once per position and draft elevation configuration.
    pub(crate) fn request_options_station_details(&mut self, dlg: &mut OptionsDlg) {
        if dlg.tab != crate::ui::options::OptionsTab::Stations {
            return;
        }
        for who in Who::BOTH {
            let key = details_key(&dlg.draft, who);
            let edit = dlg.stations.edit_mut(who);
            if key == edit.details_key {
                continue;
            }
            edit.clear_details();
            if let Some((pos, enabled)) = key {
                let id = self.next_options_station_request_id();
                edit.details_key = key;
                edit.details_id = Some(id);
                self.services
                    .request_options_station_details(id, who, enabled, pos);
            }
        }
    }

    fn next_options_station_request_id(&mut self) -> u64 {
        self.options_station_request_id += 1;
        self.options_station_request_id
    }

    fn request_options_qrz_lookup(&mut self, who: Who, s: &Settings) {
        let id = self.next_options_station_request_id();
        let Some(dlg) = &mut self.options_dlg else {
            return;
        };
        let call = who.call(s).trim().to_ascii_uppercase();
        if !as_stations::callsign::check(&call) {
            return;
        }
        dlg.stations.start_qrz(s, who, id);
        self.services
            .request_qrz_lookup(id, who, format!("{}{call}", s.gui.qrz_url_database));
    }

    /// Persist confirmed station edits before replacing live settings or restarting workers.
    pub(crate) fn write_options_station_edits(&mut self, dlg: &OptionsDlg) -> Result<(), String> {
        dlg.validate_station_edits()?;
        let now = Utc::now();
        let mut locations = dlg.pending_locations(now);
        locations.retain(|row| {
            self.services.station_position(&row.call, &row.loc)
                != Some(GPoint::new(row.lat, row.lon))
        });
        let edits = dlg.stations.qrv_edits(now);
        if locations.is_empty() && edits.is_empty() {
            return Ok(());
        }
        self.services
            .write_options_station_edits(&locations, &edits)
            .map_err(|_| "Cannot save station edits.".to_owned())
    }

    /// Open the dialog a button of the Stations tab asked for, on the Options draft `s`.
    pub fn open_station_dialog(&mut self, action: StationAction, s: &Settings) {
        match action {
            StationAction::Obstructions => self.open_obstructions_dlg(s),
            StationAction::Horizon(who) => self.open_horizon_dlg(who, s),
            StationAction::Map(_) => {}
            StationAction::Lookup(who) => self.request_options_qrz_lookup(who, s),
        }
    }

    /// `btn_Options_LocalObstructions_Click`: the obstruction at My position for the effective
    /// elevation model.
    fn open_obstructions_dlg(&mut self, s: &Settings) {
        let (lat, lon) = (s.my_lat, s.my_lon);
        let model = s.elevation_enabled.effective_model();
        self.obstructions_dlg = Some(if model == ElevationModel::None {
            ObstructionsDlg::failed(
                lat,
                lon,
                "No elevation model enabled: enable one in Options".into(),
            )
        } else {
            match self.services.obstruction_load(model, lat, lon) {
                Ok(o) => ObstructionsDlg::new(model, &o),
                Err(e) => ObstructionsDlg::failed(lat, lon, format!("local obstruction: {e}")),
            }
        });
    }

    /// OK of the obstruction dialog: store the edited obstruction.
    pub fn store_obstruction(&mut self, model: ElevationModel, o: &LocalObstruction) {
        if let Err(e) = self.services.obstruction_store(model, o) {
            self.status = format!("cannot save local obstruction: {e}");
        } else if let Some(dlg) = &mut self.options_dlg {
            dlg.stations.my.clear_details();
        }
    }

    /// `btn_Options_MyHorizon_Click` / `btn_Options_DXHorizon_Click`. Only My station has a local
    /// obstruction. The antenna height is the stored QRV value of the current band, or the band
    /// default if that is 0 (`HorizonDlg` constructor).
    fn open_horizon_dlg(&mut self, who: Who, s: &Settings) {
        let (call, lat, lon) = match who {
            Who::My => (&s.my_call, s.my_lat, s.my_lon),
            Who::Dx => (&s.dx_call, s.dx_lat, s.dx_lon),
        };
        let stored = station_key(call, lat, lon)
            .and_then(|(c, l)| {
                self.services
                    .qrv_get_all(&c, &l)
                    .into_iter()
                    .find(|q| q.band == s.band)
            })
            .map_or(0.0, |q| q.antenna_height);
        let height = if stored != 0.0 {
            stored
        } else {
            as_stations::default_antenna_height(s.band)
        };
        // `LocationFindOrCreate(call, loc)`: the stored position of the station, else the centre
        // of the locator cell. The new row is not inserted (`logic-findings.md` #90).
        let pos = station_key(call, lat, lon)
            .and_then(|(c, l)| {
                self.services.station_position(&c, &l).or_else(|| {
                    maidenhead::lat_lon_from_loc(&l, PositionInRectangle::MiddleMiddle)
                        .ok()
                        .map(|(la, lo)| GPoint::new(la, lo))
                })
            })
            .unwrap_or_else(|| GPoint::new(lat, lon));
        self.horizon_dlg = Some(HorizonDlg::new(call.trim(), pos, height, who == Who::My, s));
    }

    /// Calculate button of the horizon dialog.
    pub fn horizon_start(&mut self) {
        let Some(d) = &mut self.horizon_dlg else {
            return;
        };
        self.horizon_seq += 1;
        let req = d.begin(self.horizon_seq);
        if let Err(e) = self.services.start_horizon(req) {
            d.on_failed(self.horizon_seq, e);
        }
    }

    /// Export button of the horizon dialog: `<data dir>/Tmp/horizon_<call>.csv`.
    pub fn horizon_export(&mut self) {
        let Some(d) = &mut self.horizon_dlg else {
            return;
        };
        match &self.data_dir {
            Some(dir) => d.export(&dir.join("Tmp")),
            None => d.status = "Export failed: no data directory".into(),
        }
    }

    /// Close the horizon dialog; a running calculation is cancelled (`HorizonDlg_FormClosing`).
    pub fn horizon_close(&mut self) {
        if self
            .horizon_dlg
            .take()
            .is_some_and(|d| d.state == HorizonState::Running)
        {
            self.services.cancel_horizon();
        }
    }

    /// OK of the watchlist dialog (`btn_Control_Manage_Watchlist_Click`): rebuild the watchlist
    /// from the dialog's right list. Out of range is decided by the stored position (unknown
    /// stations count as out of range), and calls that were checked stay checked.
    pub fn apply_watchlist(&mut self, entries: &[(String, String)]) {
        let checked: Vec<String> = self
            .settings
            .watchlist
            .0
            .iter()
            .filter(|i| i.checked)
            .map(|i| i.call.clone())
            .collect();
        let s = &self.settings;
        let items: Vec<WatchlistItem> = entries
            .iter()
            .map(|(call, loc)| {
                let out_of_range = self.services.station_position(call, loc).is_none_or(|p| {
                    as_geo::latlon::distance(s.my_lat, s.my_lon, p.lat, p.lon)
                        > s.path_max_length_km
                });
                WatchlistItem {
                    call: call.clone(),
                    loc: loc.clone(),
                    checked: checked.contains(call),
                    out_of_range,
                    ..WatchlistItem::default()
                }
            })
            .collect();
        self.settings.watchlist.0 = items;
        self.settings.watchlist.sort();
        if self.playing_forward() && self.path_mode() == PathMode::Multi {
            self.watchlist_dirty = true;
        }
    }

    /// Selecting a watchlist row sets the DX station (call, locator and the stored position).
    /// Refused while playing.
    pub fn set_dx_from_watch(&mut self, idx: usize) {
        if self.is_playing() {
            return;
        }
        let Some(item) = self.settings.watchlist.0.get(idx) else {
            return;
        };
        let (call, loc) = (item.call.clone(), item.loc.clone());
        self.settings.dx_call = call.clone();
        if let Some(p) = self
            .services
            .station_position(&call, &loc)
            .or_else(|| parse_position(&loc))
        {
            self.settings.dx_lat = p.lat;
            self.settings.dx_lon = p.lon;
        }
        self.dx_loc_text = loc;
    }

    /// `(QRB km, QTF deg)` from My to a watchlist station with a stored position (the row tooltip).
    pub fn watch_qrb_qtf(&self, call: &str, loc: &str) -> Option<(f64, f64)> {
        let p = self.services.station_position(call, loc)?;
        let s = &self.settings;
        Some((
            as_geo::latlon::distance(s.my_lat, s.my_lon, p.lat, p.lon),
            as_geo::latlon::bearing(s.my_lat, s.my_lon, p.lat, p.lon),
        ))
    }

    /// Double click on the UTC field (`tb_UTC`): only while paused.
    pub fn open_set_time_dlg(&mut self) -> bool {
        if self.is_playing() {
            return false;
        }
        let t = &self.settings.time;
        self.set_time_dlg = Some(SetTimeDlg::new(t.online, t.offline));
        true
    }

    /// OK of `SetTimeDlg`.
    pub fn apply_time(&mut self, online: bool, offline: DateTime<Utc>) {
        self.settings.time.online = online;
        self.settings.time.offline = offline;
    }

    /// Startup after the settings load: the first-run wizard on a fresh install (a settings file
    /// that failed to load never gets it, so it is not overwritten), else the services at once.
    pub fn startup(&mut self) {
        // `Time_Mode_Online = true` on every start (MapDlg.cs:1958-1959)
        self.settings.time.online = true;
        if self.settings.first_run && self.settings_error.is_none() {
            self.wizard = Some(WizardState::new(&self.settings));
        } else {
            self.start_background();
            self.start_integrations();
        }
    }

    /// The My and DX locator fields from the settings, after the settings were replaced (a
    /// stale field would move the station back on its next edit).
    pub(super) fn sync_loc_texts(&mut self) {
        self.my_loc_text = self.locator(Who::My.position(&self.settings));
        self.dx_loc_text = self.locator(Who::Dx.position(&self.settings));
    }

    /// Finish of the wizard: take its settings (first run done), save them, start the services.
    pub fn finish_wizard(&mut self) {
        let Some(w) = self.wizard.take() else {
            return;
        };
        self.settings = w.finished_settings();
        self.sync_loc_texts();
        if let Some(path) = self.settings_path.clone() {
            if let Err(e) = self.save_settings(&path) {
                self.status = format!("cannot save settings: {e}");
            }
        }
        self.start_background();
        self.start_integrations();
    }
}
