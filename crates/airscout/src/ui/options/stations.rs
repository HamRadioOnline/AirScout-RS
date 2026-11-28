//! Stations tab: My and DX positions and their QRV entries for the selected band.
//!
//! Location and QRV edits stay in the dialog until Options is confirmed.

use crate::ui::forms::{
    geometry::designer_rect,
    trace::{self, Kind},
    widgets,
};
use as_geo::band::Band;
use as_geo::latlon::GPoint;
use as_geo::maidenhead::{self, PositionInRectangle};
use as_settings::{ElevationEnabled, GuiSettings, Settings};
use as_stations::{callsign, GeoSource, Location, Qrv};
use chrono::{DateTime, Utc};
use eframe::egui::{self, DragValue, Rect, Ui};
use std::collections::BTreeMap;

const QRZ_FAILURE: &str =
    "Position query at QRZ.com failed or does not match with current locator. \nNo update was performed.";
const QRZ_SUCCESS: &str = "Position update from QRZ.com was performed succesfully.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    My,
    Dx,
}

impl Who {
    pub const BOTH: [Who; 2] = [Who::My, Who::Dx];

    pub(crate) fn call(self, s: &Settings) -> &str {
        match self {
            Who::My => &s.my_call,
            Who::Dx => &s.dx_call,
        }
    }

    pub(crate) fn position(self, s: &Settings) -> GPoint {
        match self {
            Who::My => GPoint::new(s.my_lat, s.my_lon),
            Who::Dx => GPoint::new(s.dx_lat, s.dx_lon),
        }
    }

    fn set_position(self, s: &mut Settings, p: GPoint) {
        match self {
            Who::My => (s.my_lat, s.my_lon) = (p.lat, p.lon),
            Who::Dx => (s.dx_lat, s.dx_lon) = (p.lat, p.lon),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Who::My => "My",
            Who::Dx => "DX",
        }
    }
}

/// `(call, 6-character locator)` a QRV row belongs to.
pub type StationKey = (String, String);

/// Changes staged for outer Options OK.
#[derive(Clone, Debug, PartialEq)]
pub enum QrvEdit {
    Upsert(Qrv),
    Delete {
        key: StationKey,
        band: Band,
        expected_updated_at: DateTime<Utc>,
    },
}

const INVALID_QRV: &str =
    "Antenna Height, Antenna Gain and Power must all be finite numbers or all be empty.";

#[derive(Clone, Debug, Default, PartialEq)]
struct QrvRow {
    fields: [String; 3],
    loaded: Option<Qrv>,
    edited_by: Option<Who>,
}

fn qrv_values(qrv: &Qrv) -> [f64; 3] {
    [qrv.antenna_height, qrv.antenna_gain, qrv.power]
}

fn finite_number(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

impl QrvRow {
    fn from_stored(qrv: Qrv) -> Self {
        Self {
            fields: qrv_values(&qrv).map(|n| n.to_string()),
            loaded: Some(qrv),
            edited_by: None,
        }
    }

    fn values(&self) -> Result<Option<[f64; 3]>, &'static str> {
        if self.fields.iter().all(|text| text.trim().is_empty()) {
            return Ok(None);
        }
        match self.fields.each_ref().map(|text| finite_number(text)) {
            [Some(height), Some(gain), Some(power)] => Ok(Some([height, gain, power])),
            _ => Err(INVALID_QRV),
        }
    }

    fn edit(&self, key: &StationKey, band: Band, now: DateTime<Utc>) -> Option<QrvEdit> {
        let (call, loc) = key;
        match self.values().ok()? {
            None => self.loaded.as_ref().map(|row| QrvEdit::Delete {
                key: key.clone(),
                band,
                expected_updated_at: row.last_updated,
            }),
            Some(values) if self.loaded.as_ref().map(qrv_values) != Some(values) => {
                let [height, gain, power] = values;
                Qrv::new(call, loc, band, height, gain, power, now)
                    .ok()
                    .map(QrvEdit::Upsert)
            }
            Some(_) => None,
        }
    }
}

/// The key of a station in the station database, if call and position are valid.
pub fn station_key(call: &str, lat: f64, lon: f64) -> Option<StationKey> {
    let call = call.trim();
    if !callsign::check(call) {
        return None;
    }
    let loc = maidenhead::loc_from_lat_lon(lat, lon, false, 3, false)
        .ok()
        .flatten()?;
    Some((call.to_uppercase(), loc))
}

/// Format a station locator for display. Storage keys always use six characters.
pub(crate) fn locator_for_settings(pos: GPoint, gui: &GuiSettings) -> String {
    maidenhead::loc_from_lat_lon(
        pos.lat,
        pos.lon,
        gui.locator_small_letters_for_subsquares,
        gui.locator_max_length.clamp(6, 14) / 2,
        gui.locator_auto_length,
    )
    .ok()
    .flatten()
    .unwrap_or_default()
}

/// A locator of at least 6 characters that parses (`tb_Options_MyLoc_TextChanged`).
pub fn loc_is_valid(text: &str) -> bool {
    let t = text.trim();
    t.len() >= 6 && maidenhead::check(t)
}

/// The inputs to an elevation and local-obstruction read.
pub(crate) type DetailsKey = (GPoint, ElevationEnabled);

/// Edit state of one station.
#[derive(Clone, Debug, PartialEq)]
pub struct StationEdit {
    /// Text of the locator field.
    pub loc: String,
    seen: (u64, u64),
    format: (i32, bool, bool),
    pub qrv_key: Option<StationKey>,
    pub elevation_m: Option<i16>,
    pub(crate) obstruction: Option<bool>,
    pub(crate) details_key: Option<DetailsKey>,
    pub(crate) details_id: Option<u64>,
    qrz: Option<QrzRequest>,
    pub(crate) matched_call: String,
    original: (String, GPoint),
}

#[derive(Clone, Debug, PartialEq)]
struct QrzRequest {
    id: u64,
    call: String,
    position: GPoint,
    locator: String,
}

impl StationEdit {
    fn new(s: &Settings, who: Who) -> Self {
        let pos = who.position(s);
        let call = who.call(s);
        Self {
            loc: locator_for_settings(pos, &s.gui),
            seen: position_bits(pos),
            format: locator_format(&s.gui),
            qrv_key: None,
            elevation_m: None,
            obstruction: None,
            details_key: None,
            details_id: None,
            qrz: None,
            matched_call: call.to_owned(),
            original: (call.trim().to_ascii_uppercase(), pos),
        }
    }

    fn sync_from_position(&mut self, pos: GPoint, gui: &GuiSettings) {
        let format = locator_format(gui);
        if self.seen != position_bits(pos) || self.format != format {
            self.set_position(pos, gui);
        }
    }

    fn set_position(&mut self, pos: GPoint, gui: &GuiSettings) {
        self.loc = locator_for_settings(pos, gui);
        self.seen = position_bits(pos);
        self.format = locator_format(gui);
    }

    /// The locator text was edited. A complete locator moves the position to its centre.
    pub fn set_loc(&mut self, text: &str, lat: &mut f64, lon: &mut f64) {
        self.loc = text.to_owned();
        (*lat, *lon) = maidenhead::lat_lon_from_loc(text.trim(), PositionInRectangle::MiddleMiddle)
            .ok()
            .filter(|_| loc_is_valid(text))
            .unwrap_or((f64::NAN, f64::NAN));
        self.seen = position_bits(GPoint::new(*lat, *lon));
        self.qrz = None;
    }

    pub(crate) fn clear_details(&mut self) {
        self.details_key = None;
        self.details_id = None;
        self.elevation_m = None;
        self.obstruction = None;
    }
}

fn position_bits(p: GPoint) -> (u64, u64) {
    (p.lat.to_bits(), p.lon.to_bits())
}

fn locator_format(gui: &GuiSettings) -> (i32, bool, bool) {
    (
        gui.locator_max_length,
        gui.locator_auto_length,
        gui.locator_small_letters_for_subsquares,
    )
}

pub(crate) fn details_key(s: &Settings, who: Who) -> Option<DetailsKey> {
    let p = who.position(s);
    maidenhead::check_lat_lon(p.lat, p.lon).then_some((p, s.elevation_enabled))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StationMessage {
    BandError(&'static str),
    Qrz(&'static str),
}

/// State of the tab.
#[derive(Clone, Debug, PartialEq)]
pub struct StationsState {
    pub my: StationEdit,
    pub dx: StationEdit,
    qrvs: BTreeMap<StationKey, BTreeMap<Band, QrvRow>>,
    message: Option<StationMessage>,
}

impl StationsState {
    pub fn new(s: &Settings) -> Self {
        Self {
            my: StationEdit::new(s, Who::My),
            dx: StationEdit::new(s, Who::Dx),
            qrvs: BTreeMap::new(),
            message: None,
        }
    }

    pub fn edit(&self, who: Who) -> &StationEdit {
        match who {
            Who::My => &self.my,
            Who::Dx => &self.dx,
        }
    }

    pub fn edit_mut(&mut self, who: Who) -> &mut StationEdit {
        match who {
            Who::My => &mut self.my,
            Who::Dx => &mut self.dx,
        }
    }

    pub(super) fn message(&self) -> Option<&'static str> {
        self.message.map(|message| match message {
            StationMessage::BandError(text) | StationMessage::Qrz(text) => text,
        })
    }

    pub(super) fn clear_band_error(&mut self) {
        if matches!(self.message, Some(StationMessage::BandError(_))) {
            self.message = None;
        }
    }

    /// Apply the local database result of a call edit, clearing the prior station on a miss.
    pub fn apply_call_match(
        &mut self,
        s: &mut Settings,
        who: Who,
        call: &str,
        pos: Option<GPoint>,
    ) {
        match who {
            Who::My => s.my_call = call.to_owned(),
            Who::Dx => s.dx_call = call.to_owned(),
        }
        self.set_position(s, who, pos.unwrap_or(GPoint::new(f64::NAN, f64::NAN)));
        let edit = self.edit_mut(who);
        edit.matched_call = call.to_owned();
        edit.qrz = None;
        self.message = None;
    }

    pub(super) fn set_position(&mut self, s: &mut Settings, who: Who, pos: GPoint) {
        let edit = self.edit_mut(who);
        if who.position(s) != pos {
            edit.qrz = None;
        }
        who.set_position(s, pos);
        edit.set_position(pos, &s.gui);
        if edit.details_key != details_key(s, who) {
            edit.clear_details();
        }
    }

    pub(crate) fn start_qrz(&mut self, s: &Settings, who: Who, id: u64) {
        let edit = self.edit_mut(who);
        edit.qrz = Some(QrzRequest {
            id,
            call: who.call(s).to_owned(),
            position: who.position(s),
            locator: edit.loc.clone(),
        });
        self.message = None;
    }

    pub(crate) fn finish_qrz(
        &mut self,
        s: &mut Settings,
        who: Who,
        id: u64,
        pos: Result<GPoint, String>,
    ) {
        let edit = self.edit_mut(who);
        if edit.qrz.as_ref().is_none_or(|request| request.id != id) {
            return;
        }
        let request = edit.qrz.take().unwrap();
        if request.call != who.call(s)
            || position_bits(request.position) != position_bits(who.position(s))
            || request.locator != edit.loc
        {
            return;
        }
        match pos {
            Ok(pos) => self.apply_qrz_position(s, who, pos),
            Err(_) => self.message = Some(StationMessage::Qrz(QRZ_FAILURE)),
        }
    }

    fn apply_qrz_position(&mut self, s: &mut Settings, who: Who, pos: GPoint) {
        let locator = maidenhead::convert(self.edit(who).loc.trim(), false).unwrap_or_default();
        let key = maidenhead::loc_from_lat_lon(pos.lat, pos.lon, false, 3, false)
            .ok()
            .flatten();
        if key.is_some() && key.as_deref() == locator.get(..6) {
            self.set_position(s, who, pos);
            self.message = Some(StationMessage::Qrz(QRZ_SUCCESS));
        } else {
            self.message = Some(StationMessage::Qrz(QRZ_FAILURE));
        }
    }

    pub(super) fn pending_locations(&self, s: &Settings, now: DateTime<Utc>) -> Vec<Location> {
        Who::BOTH
            .into_iter()
            .filter_map(|who| {
                let edit = self.edit(who);
                let pos = who.position(s);
                let (call, loc) = station_key(who.call(s), pos.lat, pos.lon)?;
                if !loc_is_valid(&edit.loc) || (edit.original.0 == call && edit.original.1 == pos) {
                    return None;
                }
                let source = if maidenhead::is_precise(pos.lat, pos.lon, 3).unwrap_or(false) {
                    GeoSource::FromUser
                } else {
                    GeoSource::FromLoc
                };
                Some(Location::new(&call, &loc, pos.lat, pos.lon, source, 0, now))
            })
            .collect()
    }

    /// Load each station once and report whether either station's displayed key changed.
    pub fn refresh_qrvs(
        &mut self,
        s: &Settings,
        load: &mut dyn FnMut(&str, &str) -> Vec<Qrv>,
    ) -> bool {
        let mut changed = false;
        for who in Who::BOTH {
            let pos = who.position(s);
            let key = station_key(who.call(s), pos.lat, pos.lon);
            if self.edit(who).qrv_key == key {
                continue;
            }
            if let Some(key) = &key {
                self.qrvs.entry(key.clone()).or_insert_with(|| {
                    load(&key.0, &key.1)
                        .into_iter()
                        .map(|q| (q.band, QrvRow::from_stored(q)))
                        .collect()
                });
            }
            self.edit_mut(who).qrv_key = key;
            changed = true;
        }
        changed
    }

    /// Both visible QRV trios may be empty or contain three finite numbers.
    pub fn validate_current_qrv(&self, band: Band) -> Result<(), &'static str> {
        self.invalid_current_qrv(band)
            .map_or(Ok(()), |_| Err(INVALID_QRV))
    }

    fn invalid_current_qrv(&self, band: Band) -> Option<(Who, &StationKey)> {
        Who::BOTH.into_iter().find_map(|who| {
            let key = self.edit(who).qrv_key.as_ref()?;
            let row = self.qrvs.get(key)?.get(&band)?;
            row.values().is_err().then_some((who, key))
        })
    }

    fn validate_station_positions(&self, s: &Settings) -> Result<(), &'static str> {
        for who in Who::BOTH {
            let pos = who.position(s);
            if station_key(who.call(s), pos.lat, pos.lon).is_none() {
                return Err(match who {
                    Who::My => "Enter a valid My station call and position.",
                    Who::Dx => "Enter a valid DX station call and position.",
                });
            }
        }
        Ok(())
    }

    /// Validate all pending QRV edits, including rows at old keys, and, when `visible` (the
    /// Stations page is current), both stations and their displayed QRV rows
    /// (`tab_Options_Stations_Validating`, `OptionsDlg.cs:868-873`).
    pub(super) fn validate_station_edits(&self, s: &Settings, visible: bool) -> Result<(), String> {
        if visible {
            self.validate_station_positions(s)?;
            if let Some((who, key)) = self.invalid_current_qrv(s.band) {
                return Err(qrv_error(who, key, s.band));
            }
        }
        for (key, rows) in &self.qrvs {
            for (&band, row) in rows {
                let Some(who) = row.edited_by else { continue };
                if row.values().is_err()
                    && row
                        .loaded
                        .as_ref()
                        .is_none_or(|q| row.fields != qrv_values(q).map(|n| n.to_string()))
                {
                    return Err(qrv_error(who, key, band));
                }
            }
        }
        Ok(())
    }

    /// Upserts and version-checked deletes across every station and band edited in this dialog.
    pub fn qrv_edits(&self, now: DateTime<Utc>) -> Vec<QrvEdit> {
        self.qrvs
            .iter()
            .flat_map(|(key, rows)| {
                rows.iter()
                    .filter_map(move |(&band, row)| row.edit(key, band, now))
            })
            .collect()
    }
}

fn qrv_error(who: Who, (call, loc): &StationKey, band: Band) -> String {
    format!(
        "{} station {call} {loc} on {}. {INVALID_QRV}",
        who.name(),
        band.name()
    )
}

/// A button of the tab that opens a dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StationAction {
    /// `MapStationDlg` of a station.
    Map(Who),
    /// Explicit QRZ lookup for a station.
    Lookup(Who),
    /// `HorizonDlg` of a station.
    Horizon(Who),
    /// `LocalObstructionsDlg` (of My station).
    Obstructions,
}

/// Draw the tab; returns the button that was pressed.
pub fn show(ui: &mut Ui, s: &mut Settings, st: &mut StationsState) -> Option<StationAction> {
    let body = ui.max_rect();
    ui.set_min_size(egui::vec2(665.0, 454.0));
    let my = station(ui, body, s, &mut st.my, Who::My);
    widgets::group_box(
        ui,
        "groupBox16",
        "Stations Information",
        designer_rect(body, [247, 8], [177, 228]),
        |ui| {
            let rect = designer_rect(ui.max_rect(), [16, 18], [155, 204]);
            let text = "Information from callsign database and other sources are used to prefill fields. You can overwrite and complete entries here. Your local database is updated.";
            trace::record(ui, "label52", Kind::Text, text, rect);
            widgets::put_left(ui, rect, egui::Label::new(text).wrap());
        },
    );
    let dx = station(ui, body, s, &mut st.dx, Who::Dx);

    let band = s.band;
    for who in Who::BOTH {
        let key = match who {
            Who::My => &st.my.qrv_key,
            Who::Dx => &st.dx.qrv_key,
        };
        let pos = who.position(s);
        let current_key = station_key(who.call(s), pos.lat, pos.lon);
        let mut empty = QrvRow::default();
        let row = key
            .as_ref()
            .filter(|key| Some(*key) == current_key.as_ref())
            .and_then(|key| st.qrvs.get_mut(key))
            .map(|rows| rows.entry(band).or_default());
        let editable = row.is_some() && Band::all_except_none_and_all().contains(&band);
        if qrv_group(ui, body, who, row.unwrap_or(&mut empty), editable) {
            st.clear_band_error();
        }
    }
    if let Some(next) = band_selector(ui, body, band) {
        match st
            .validate_station_positions(s)
            .and_then(|()| st.validate_current_qrv(band))
        {
            Ok(()) => {
                s.band = next;
                st.message = None;
                ui.ctx().request_repaint();
            }
            Err(error) => st.message = Some(StationMessage::BandError(error)),
        }
    }

    let manage = widgets::group_box(
        ui,
        "groupBox18",
        "Local Obstructions for My Station",
        designer_rect(body, [6, 376], [235, 75]),
        |ui| {
            let r = ui.max_rect();
            let text = match st.my.obstruction {
                Some(true) => "Local Obstruction found for this location.",
                Some(false) => "Local Obstruction not found for this location.",
                None => "Status is unknown.",
            };
            let status = designer_rect(r, [6, 22], [223, 24]);
            trace::record(
                ui,
                "lbl_Options_LocalObstructions",
                Kind::Text,
                text,
                status,
            );
            widgets::put_left(
                ui,
                status,
                egui::Label::new(egui::RichText::new(text).size(10.0)).wrap(),
            );
            widgets::button(
                ui,
                "btn_Options_LocalObstructions",
                "Manage",
                designer_rect(r, [9, 46], [220, 23]),
            )
            .clicked()
            .then_some(StationAction::Obstructions)
        },
    );

    widgets::group_box(
        ui,
        "groupBox14",
        "Location Settings",
        designer_rect(body, [247, 376], [418, 78]),
        |ui| location_settings(ui, s),
    );
    my.or(dx).or(manage)
}

fn location_settings(ui: &mut Ui, s: &mut Settings) {
    let r = ui.max_rect();
    let label_rect = designer_rect(r, [14, 17], [161, 13]);
    let caption = "Number of Locator digits (6 ..14):";
    trace::record(ui, "label48", Kind::Text, caption, label_rect);
    let label = widgets::put_left(
        ui,
        label_rect,
        egui::Label::new(egui::RichText::new(caption).size(10.0)).extend(),
    );
    let rect = designer_rect(r, [180, 15], [50, 20]);
    trace::record(ui, "ud_Options_Locator_MaxLength", Kind::Text, "", rect);
    let mut pairs = s.gui.locator_max_length.clamp(6, 14) / 2;
    let response = ui.put(
        rect,
        DragValue::new(&mut pairs)
            .range(3..=7)
            .custom_formatter(|pairs, _| format!("{}", pairs as i32 * 2))
            .custom_parser(|value| {
                value
                    .trim()
                    .parse::<i32>()
                    .ok()
                    .map(|digits| f64::from(digits / 2))
            }),
    );
    if response.labelled_by(label.id).changed() {
        s.gui.locator_max_length = pairs * 2;
    }
    let small_letters_rect = designer_rect(r, [239, 16], [179, 17]);
    let small_letters = "Small letters for subsquares:";
    trace::record(
        ui,
        "cb_Options_SmallLettersForSubSquares",
        Kind::Text,
        small_letters,
        small_letters_rect,
    );
    widgets::put_left(
        ui,
        small_letters_rect,
        egui::Checkbox::new(
            &mut s.gui.locator_small_letters_for_subsquares,
            egui::RichText::new(small_letters).size(10.0),
        ),
    );
    widgets::checkbox(
        ui,
        "cb_Options_Locator_AutoLength",
        "Cut locator to significant digits automatically (Minimum 6 digits):",
        &mut s.gui.locator_auto_length,
        designer_rect(r, [12, 38], [401, 17]),
    );
    widgets::checkbox(
        ui,
        "cb_Options_Locations_RestrictToAreaOfInterest",
        "Restrict locations to current Area of Interest",
        &mut s.location_restrict_to_area,
        designer_rect(r, [12, 58], [290, 17]),
    );
}

fn station(
    ui: &mut Ui,
    body: Rect,
    s: &mut Settings,
    edit: &mut StationEdit,
    who: Who,
) -> Option<StationAction> {
    let name = who.name();
    let my = who == Who::My;
    let (group, x, label_x, field_x) = if my {
        ("groupBox4", 6, 16, 89)
    } else {
        ("groupBox5", 430, 26, 90)
    };
    let [call_label, loc_label, lat_label, lon_label, elevation_label, unit_label] = if my {
        [
            "label10", "label11", "label39", "label40", "label12", "label18",
        ]
    } else {
        [
            "label42", "label44", "label15", "label14", "label43", "label21",
        ]
    };
    edit.sync_from_position(who.position(s), &s.gui);
    let (call, lat, lon, height) = match who {
        Who::My => (
            &mut s.my_call,
            &mut s.my_lat,
            &mut s.my_lon,
            &mut s.my_height_m,
        ),
        Who::Dx => (
            &mut s.dx_call,
            &mut s.dx_lat,
            &mut s.dx_lon,
            &mut s.dx_height_m,
        ),
    };
    station_group(
        ui,
        group,
        if my { "My Station" } else { "DX Station" },
        designer_rect(body, [x, 8], [235, 228]),
        |ui| {
            let r = ui.max_rect();
            let field = |y| designer_rect(r, [field_x, y], [100, 21]);
            let label = |x, y, w| designer_rect(r, [x, y], [w, 13]);
            let valid_call = callsign::check(call.trim());
            let call_caption = widgets::label(ui, call_label, "Call:", label(label_x, 16, 27));
            station_text(
                ui,
                &format!("tb_Options_{name}Call"),
                call,
                valid_call,
                field(12),
                call_caption,
            );
            if !valid_call {
                ui.put(
                    designer_rect(r, [84, 197], [145, 18]),
                    egui::Label::new(
                        egui::RichText::new(format!("{name} call is not valid"))
                            .color(ui.visuals().error_fg_color),
                    ),
                );
            }

            let loc_caption = widgets::label(ui, loc_label, "Loc:", label(label_x, 40, 28));
            let mut loc = edit.loc.clone();
            if station_text(
                ui,
                &format!("tb_Options_{name}Loc"),
                &mut loc,
                loc_is_valid(&edit.loc),
                designer_rect(r, [field_x, 36], [136, 21]),
                loc_caption,
            )
            .changed()
            {
                edit.set_loc(&loc, lat, lon);
            }
            let lat_caption = widgets::label(
                ui,
                lat_label,
                "Lat:",
                label(label_x, if my { 63 } else { 64 }, 25),
            );
            let lon_caption = widgets::label(
                ui,
                lon_label,
                "Lon:",
                label(label_x, if my { 87 } else { 90 }, 28),
            );
            let lat_changed = station_coordinate(
                ui,
                &format!("tb_Options_{name}Lat"),
                lat,
                90.0,
                field(59),
                lat_caption,
            );
            let lon_changed = station_coordinate(
                ui,
                &format!("tb_Options_{name}Lon"),
                lon,
                180.0,
                field(83),
                lon_caption,
            );
            if lat_changed || lon_changed {
                edit.sync_from_position(GPoint::new(*lat, *lon), &s.gui);
                edit.clear_details();
                edit.qrz = None;
            }

            if edit.details_key
                != maidenhead::check_lat_lon(*lat, *lon)
                    .then_some((GPoint::new(*lat, *lon), s.elevation_enabled))
            {
                edit.clear_details();
            }
            let elevation_y = if my { 111 } else { 115 };
            let elevation_caption = widgets::label(
                ui,
                elevation_label,
                "Elevation:",
                label(if my { 14 } else { 26 }, elevation_y, 54),
            );
            widgets::label(
                ui,
                unit_label,
                "m asl",
                label(if my { 198 } else { 194 }, elevation_y, 31),
            );
            let elevation_rect = if my {
                field(107)
            } else {
                designer_rect(r, [field_x, 108], [100, 20])
            };
            let mut elevation = edit
                .elevation_m
                .map(|value| value.to_string())
                .unwrap_or_default();
            trace::record(
                ui,
                &format!("tb_Options_{name}Elevation"),
                Kind::Text,
                "",
                elevation_rect,
            );
            ui.put(
                elevation_rect,
                egui::TextEdit::singleline(&mut elevation).interactive(false),
            )
            .labelled_by(elevation_caption.id);

            let ready =
                valid_call && loc_is_valid(&edit.loc) && maidenhead::check_lat_lon(*lat, *lon);
            let mut action = None;
            let buttons = egui::UiBuilder::new();
            ui.scope_builder(if ready { buttons } else { buttons.disabled() }, |ui| {
                let map_rect = designer_rect(r, if my { [7, 134] } else { [8, 135] }, [75, 81]);
                if widgets::button(ui, &format!("btn_Options_{name}Map"), "\nMap", map_rect)
                    .clicked()
                {
                    action = Some(StationAction::Map(who));
                }
                let horizon_rect = designer_rect(r, [field_x, 157], [100, 20]);
                if widgets::button(
                    ui,
                    &format!("btn_Options_{name}Horizon"),
                    "Radio Horizon",
                    horizon_rect,
                )
                .on_hover_text("Calculates a radio horizon around the station's location")
                .clicked()
                {
                    action = Some(StationAction::Horizon(who));
                }
            });
            let qrz = egui::UiBuilder::new();
            ui.scope_builder(if valid_call { qrz } else { qrz.disabled() }, |ui| {
                let rect = designer_rect(r, [field_x, 134], [100, 20]);
                if widgets::button(ui, &format!("btn_{name}Call_QRZ"), "QRZ lookup", rect)
                    .on_hover_text("Tries to get an exact location from www.qrz.com. The information will only be used if the 6digit Maidenhead locator matches with the value in the Loc input box.")
                    .clicked()
                {
                    action = Some(StationAction::Lookup(who));
                }
            });

            let caption = widgets::label(
                ui,
                &format!("lbl_{name}AntennaHeight"),
                &format!("{name} height [m]"),
                designer_rect(r, [85, 181], [88, 17]),
            );
            let rect = designer_rect(r, [173, 181], [56, 18]);
            trace::record(
                ui,
                &format!("opt_{name}AntennaHeight"),
                Kind::Text,
                "",
                rect,
            );
            ui.put(
                rect,
                DragValue::new(height)
                    .range(0.0..=10_000.0)
                    .clamp_existing_to_range(false),
            )
            .labelled_by(caption.id);
            action
        },
    )
}

/// Position the title to the left of the call field, which starts at y=12 in these groups.
fn station_group<R>(
    ui: &mut Ui,
    name: &str,
    title: &str,
    rect: Rect,
    show: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.painter().rect_stroke(
        rect,
        0.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    widgets::put_left(
        ui,
        designer_rect(rect, [8, 0], [70, 15]),
        egui::Label::new(title).extend(),
    );
    trace::scope_with_text(ui, name, title, rect, |ui| widgets::scoped(ui, rect, show))
}

fn station_text(
    ui: &mut Ui,
    name: &str,
    text: &mut String,
    valid: bool,
    rect: Rect,
    label: egui::Response,
) -> egui::Response {
    trace::record(ui, name, Kind::Text, "", rect);
    let mut widget = egui::TextEdit::singleline(text).id_salt(name);
    if !valid {
        widget = widget.text_color(ui.visuals().error_fg_color);
    }
    ui.put(rect, widget).labelled_by(label.id)
}

fn station_coordinate(
    ui: &mut Ui,
    name: &str,
    value: &mut f64,
    limit: f64,
    rect: Rect,
    label: egui::Response,
) -> bool {
    trace::record(ui, name, Kind::Text, "", rect);
    let before = value.to_bits();
    let response = ui.put(
        rect,
        DragValue::new(value)
            .range(-limit..=limit)
            .clamp_existing_to_range(false)
            .speed(0.0005)
            .custom_formatter(|value, _| {
                if value.is_finite() {
                    format!("{value:.8}")
                } else {
                    String::new()
                }
            }),
    );
    response.labelled_by(label.id);
    before != value.to_bits()
}

fn band_selector(ui: &mut Ui, body: Rect, band: Band) -> Option<Band> {
    station_group(
        ui,
        "groupBox45",
        "Band",
        designer_rect(body, [247, 242], [177, 128]),
        |ui| {
            let r = ui.max_rect();
            let up = band_button(ui, true, designer_rect(r, [52, 16], [70, 24]));
            let rect = designer_rect(r, [52, 46], [70, 26]);
            trace::record(ui, "tb_Options_Band", Kind::Text, band.name(), rect);
            let mut display = band.name();
            ui.painter().rect_filled(
                rect,
                ui.visuals().widgets.inactive.corner_radius,
                ui.visuals().text_edit_bg_color(),
            );
            ui.put(
                rect,
                egui::TextEdit::singleline(&mut display)
                    .align(egui::Align2::CENTER_CENTER)
                    .interactive(false),
            );
            let down = band_button(ui, false, designer_rect(r, [52, 79], [70, 24]));
            if up.clicked() {
                Some(band.next())
            } else if down.clicked() {
                Some(band.previous())
            } else {
                None
            }
        },
    )
}

fn band_button(ui: &mut Ui, up: bool, rect: Rect) -> egui::Response {
    let (name, caption, direction) = if up {
        ("btn_Options_BandUp", "▲", -1.0)
    } else {
        ("btn_Options_BandDown", "▼", 1.0)
    };
    trace::record(ui, name, Kind::Text, caption, rect);
    let response = ui.put(rect, egui::Button::new("").min_size(rect.size()));
    // Draw the Designer triangle directly because the default font lacks both glyphs.
    let center = response.rect.center();
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            center + egui::vec2(0.0, 4.0 * direction),
            center + egui::vec2(-5.0, -4.0 * direction),
            center + egui::vec2(5.0, -4.0 * direction),
        ],
        ui.style().interact(&response).fg_stroke.color,
        egui::Stroke::NONE,
    ));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, caption));
    response
}

fn qrv_group(ui: &mut Ui, body: Rect, who: Who, row: &mut QrvRow, editable: bool) -> bool {
    let my = who == Who::My;
    let (group, title, x, width) = if my {
        ("groupBox44", "My QRV", 6, 235)
    } else {
        ("groupBox46", "DX QRV", 429, 236)
    };
    let captions = if my {
        ["label41", "label121", "label123"]
    } else {
        ["label126", "label124", "label58"]
    };
    let units = if my {
        ["label28", "label106", "label122"]
    } else {
        ["label125", "label107", "label13"]
    };
    let label_y = if my { [20, 47, 74] } else { [19, 45, 73] };
    let field_y = if my { [16, 44, 70] } else { [16, 43, 70] };
    let offset = if my { 0 } else { 2 };
    let name = who.name();

    let changed = station_group(
        ui,
        group,
        title,
        designer_rect(body, [x, 242], [width, 128]),
        |ui| {
            let mut changed = false;
            let r = ui.max_rect();
            let caption_label = |ui: &mut Ui, name: &str, text: &str, rect| {
                trace::record(ui, name, Kind::Text, text, rect);
                widgets::put_left(
                    ui,
                    rect,
                    egui::Label::new(egui::RichText::new(text).size(10.0)).extend(),
                )
            };
            for (i, (caption, field, unit, caption_width, unit_x, unit_width)) in [
                ("Antenna Height:", "AntennaHeight", "m", 84, 214, 15),
                ("Antenna Gain:", "AntennaGain", "dbD", 75, 202, 27),
                ("Power:", "Power", "W", 40, 214, 18),
            ]
            .into_iter()
            .enumerate()
            {
                let label = caption_label(
                    ui,
                    captions[i],
                    caption,
                    designer_rect(r, [6 + offset, label_y[i]], [caption_width, 13]),
                );
                widgets::label(
                    ui,
                    units[i],
                    unit,
                    designer_rect(r, [unit_x + offset, label_y[i]], [unit_width, 13]),
                );
                let field = format!("tb_Options_{name}{field}");
                let rect = designer_rect(r, [89 + offset, field_y[i]], [100, 21]);
                trace::record(ui, &field, Kind::Text, "", rect);
                let text = &mut row.fields[i];
                let valid = text.trim().is_empty() || finite_number(text).is_some();
                let mut input = egui::TextEdit::singleline(text)
                    .id_salt(field)
                    .interactive(editable);
                if !valid {
                    input = input.text_color(ui.visuals().error_fg_color);
                }
                changed |= ui.put(rect, input).labelled_by(label.id).changed();
            }
            caption_label(
                ui,
                if my { "label127" } else { "label128" },
                "Last Updated:",
                designer_rect(r, [6, 101], [74, 13]),
            );
            let updated = row
                .loaded
                .as_ref()
                .map(|q| q.last_updated.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default();
            widgets::label(
                ui,
                &format!("lbl_Options_{name}LastUpdated"),
                &updated,
                designer_rect(r, [86 + offset, 101], [68, 13]),
            );
            changed
        },
    );
    if changed {
        row.edited_by = Some(who);
    }
    changed
}
