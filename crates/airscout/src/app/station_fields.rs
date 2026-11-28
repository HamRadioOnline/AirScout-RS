//! MapDlg station call and locator combo box handlers.

use super::{parse_position, AppState};
use crate::ui::options::stations::{loc_is_valid, locator_for_settings};
use as_geo::{latlon::GPoint, maidenhead};
use as_stations::callsign;

const DEFAULT_HEIGHT_M: f64 = 10.0;
const INVALID_POSITION: GPoint = GPoint::new(f64::NAN, f64::NAN);

impl AppState {
    /// `cb_MyCall_TextChanged` (MapDlg.cs:5785): look up the latest position.
    pub fn my_call_changed(&mut self, text: &str) {
        let p = self.call_position(text);
        self.settings.my_call = text.into();
        self.my_loc_selected(p);
    }

    /// `cb_DXCall_TextChanged` (MapDlg.cs:5909): look up the latest position.
    pub fn dx_call_changed(&mut self, text: &str) {
        let p = self.call_position(text);
        self.settings.dx_call = text.into();
        self.dx_loc_selected(p);
    }

    pub fn my_call_selected(&mut self, call: &str) {
        self.my_call_changed(call);
        self.settings.my_height_m = station_height(self.settings.my_lat, self.settings.my_lon);
    }

    pub fn dx_call_selected(&mut self, call: &str) {
        self.dx_call_changed(call);
        self.settings.dx_height_m = station_height(self.settings.dx_lat, self.settings.dx_lon);
    }

    pub fn my_call_items(&self) -> Vec<String> {
        self.settings.gui.my_calls.clone()
    }

    pub fn dx_call_items(&self) -> Vec<String> {
        let mut calls: Vec<_> = self
            .settings
            .watchlist
            .0
            .iter()
            .map(|i| i.call.clone())
            .collect();
        calls.sort();
        calls
    }

    pub fn my_loc_items(&self) -> Vec<(String, GPoint)> {
        self.loc_items(&self.settings.my_call)
    }

    pub fn dx_loc_items(&self) -> Vec<(String, GPoint)> {
        self.loc_items(&self.settings.dx_call)
    }

    pub fn my_loc_selected(&mut self, p: GPoint) {
        // UpdateStatus formats the canonical position without triggering a locator edit.
        self.my_loc_text = self.locator(p);
        self.settings.my_lat = p.lat;
        self.settings.my_lon = p.lon;
        self.sync_my();
    }

    pub fn dx_loc_selected(&mut self, p: GPoint) {
        self.dx_loc_text = self.locator(p);
        self.settings.dx_lat = p.lat;
        self.settings.dx_lon = p.lon;
    }

    pub fn set_my_loc_text(&mut self, text: &str) {
        self.my_loc_text = text.into();
        if let Some(p) = edited_position(text) {
            self.settings.my_lat = p.lat;
            self.settings.my_lon = p.lon;
            if loc_is_valid(text) {
                self.settings.my_height_m = DEFAULT_HEIGHT_M;
            }
        }
        self.sync_my();
    }

    pub fn set_dx_loc_text(&mut self, text: &str) {
        self.dx_loc_text = text.into();
        if let Some(p) = edited_position(text) {
            self.settings.dx_lat = p.lat;
            self.settings.dx_lon = p.lon;
            if loc_is_valid(text) {
                self.settings.dx_height_m = DEFAULT_HEIGHT_M;
            }
        }
    }

    fn call_position(&self, call: &str) -> GPoint {
        if callsign::check(call) {
            self.services
                .location_find(call)
                .unwrap_or(INVALID_POSITION)
        } else {
            INVALID_POSITION
        }
    }

    fn loc_items(&self, call: &str) -> Vec<(String, GPoint)> {
        self.services
            .location_find_all(call)
            .into_iter()
            .map(|p| (self.locator(p), p))
            .collect()
    }

    pub(super) fn locator(&self, p: GPoint) -> String {
        locator_for_settings(p, &self.settings.gui)
    }
}

fn station_height(lat: f64, lon: f64) -> f64 {
    if maidenhead::check_lat_lon(lat, lon) {
        DEFAULT_HEIGHT_M
    } else {
        0.0
    }
}

pub(crate) fn station_locator_valid(text: &str, lat: f64, lon: f64) -> bool {
    edited_position(text).is_some() && maidenhead::check_lat_lon(lat, lon)
}

fn edited_position(text: &str) -> Option<GPoint> {
    // Keep support for explicit coordinates. A partial locator only changes the edit text.
    if text.contains(',') || loc_is_valid(text) {
        parse_position(text)
    } else {
        None
    }
}
