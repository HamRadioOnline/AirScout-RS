//! Stations tab (`tab_Options_Stations` of the original): My and DX station (call, position,
//! locator, height) and the QRV data of the selected station, one row per band.
//!
//! The original saves the QRV data when the tab is left; here the edits stay in the dialog and
//! are written once, when Options is confirmed (`StationsState::pending_qrvs`).

use as_geo::band::Band;
use as_geo::maidenhead::{self, PositionInRectangle};
use as_settings::Settings;
use as_stations::{callsign, default_antenna_gain, default_antenna_height, default_power, Qrv};
use chrono::Utc;
use eframe::egui::{self, Color32, DragValue, Grid, Role, Ui, WidgetInfo};

/// `Locator_MaxLength` (10 characters) divided by two.
const LOCATOR_PRECISION: i32 = as_settings::LOCATOR_MAX_LENGTH / 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    My,
    Dx,
}

impl Who {
    pub const BOTH: [Who; 2] = [Who::My, Who::Dx];

    fn name(self) -> &'static str {
        match self {
            Who::My => "My",
            Who::Dx => "DX",
        }
    }
}

/// QRV values of one band as shown in the grid.
#[derive(Clone, Debug, PartialEq)]
pub struct QrvRow {
    pub band: Band,
    /// Antenna height above ground [m].
    pub height: f64,
    /// Antenna gain [dBd].
    pub gain: f64,
    /// Power [W].
    pub power: f64,
    /// Values as loaded (stored, or the band defaults); a row is written only if it differs.
    loaded: (f64, f64, f64),
}

impl QrvRow {
    fn new(band: Band, v: (f64, f64, f64)) -> Self {
        Self {
            band,
            height: v.0,
            gain: v.1,
            power: v.2,
            loaded: v,
        }
    }

    fn changed(&self) -> bool {
        (self.height, self.gain, self.power) != self.loaded
    }
}

/// `(call, 6-character locator)` a QRV set belongs to.
pub type StationKey = (String, String);

/// The QRV rows of one station.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QrvSet {
    /// `None` while call or position are not valid: there is nothing to edit.
    pub key: Option<StationKey>,
    pub rows: Vec<QrvRow>,
}

impl QrvSet {
    /// Rows for all bands: the stored values, or the per-band defaults.
    pub fn load(key: Option<StationKey>, stored: &[Qrv]) -> Self {
        let rows = if key.is_some() {
            Band::all_except_none_and_all()
                .iter()
                .map(|&b| {
                    let v = stored.iter().find(|q| q.band == b).map_or_else(
                        || {
                            (
                                default_antenna_height(b),
                                default_antenna_gain(b),
                                default_power(b),
                            )
                        },
                        |q| (q.antenna_height, q.antenna_gain, q.power),
                    );
                    QrvRow::new(b, v)
                })
                .collect()
        } else {
            Vec::new()
        };
        Self { key, rows }
    }

    /// The entries to write: rows the user changed.
    pub fn pending(&self) -> Vec<Qrv> {
        let Some((call, loc)) = &self.key else {
            return Vec::new();
        };
        self.rows
            .iter()
            .filter(|r| r.changed())
            .filter_map(|r| Qrv::new(call, loc, r.band, r.height, r.gain, r.power, Utc::now()).ok())
            .collect()
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

/// Locator text for a position (`MyLocator_Set`): empty for an invalid position, up to
/// `Locator_MaxLength` characters for a precise one, else 6.
pub fn loc_text_for(lat: f64, lon: f64, small_letters: bool) -> String {
    if !maidenhead::check_lat_lon(lat, lon) {
        return String::new();
    }
    maidenhead::loc_from_lat_lon(
        lat,
        lon,
        small_letters,
        LOCATOR_PRECISION,
        as_settings::LOCATOR_AUTO_LENGTH,
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

/// Edit state of one station.
#[derive(Clone, Debug, PartialEq)]
pub struct StationEdit {
    /// Text of the locator field.
    pub loc: String,
    /// Position `loc` was derived from.
    seen: (f64, f64),
    pub qrv: QrvSet,
}

impl StationEdit {
    fn new(lat: f64, lon: f64, small: bool) -> Self {
        Self {
            loc: loc_text_for(lat, lon, small),
            seen: (lat, lon),
            qrv: QrvSet::default(),
        }
    }

    /// Latitude or longitude changed: the locator follows. Does nothing while the position is
    /// the one the locator text came from.
    pub fn sync_from_position(&mut self, lat: f64, lon: f64, small: bool) {
        if (lat, lon) != self.seen {
            self.loc = loc_text_for(lat, lon, small);
            self.seen = (lat, lon);
        }
    }

    /// The locator text was edited: a valid one moves the position to its centre; an invalid
    /// one leaves the position alone (the field is marked).
    pub fn set_loc(&mut self, text: &str, lat: &mut f64, lon: &mut f64) {
        self.loc = text.to_string();
        if !loc_is_valid(text) {
            return;
        }
        if let Ok((la, lo)) =
            maidenhead::lat_lon_from_loc(text.trim(), PositionInRectangle::MiddleMiddle)
        {
            *lat = la;
            *lon = lo;
            self.seen = (la, lo);
        }
    }
}

/// State of the tab.
#[derive(Clone, Debug, PartialEq)]
pub struct StationsState {
    /// Station whose QRV grid is shown.
    pub selected: Who,
    pub my: StationEdit,
    pub dx: StationEdit,
}

impl StationsState {
    pub fn new(s: &Settings) -> Self {
        let small = s.gui.locator_small_letters_for_subsquares;
        Self {
            selected: Who::My,
            my: StationEdit::new(s.my_lat, s.my_lon, small),
            dx: StationEdit::new(s.dx_lat, s.dx_lon, small),
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

    /// Reload the QRV set of every station whose key changed. `load(call, loc)` reads the
    /// stored entries; it runs only on a change, not per frame.
    pub fn refresh_qrvs(&mut self, s: &Settings, load: &mut dyn FnMut(&str, &str) -> Vec<Qrv>) {
        for who in Who::BOTH {
            let (call, lat, lon) = match who {
                Who::My => (&s.my_call, s.my_lat, s.my_lon),
                Who::Dx => (&s.dx_call, s.dx_lat, s.dx_lon),
            };
            let key = station_key(call, lat, lon);
            let e = self.edit_mut(who);
            if e.qrv.key != key {
                let stored = key.as_ref().map(|(c, l)| load(c, l)).unwrap_or_default();
                e.qrv = QrvSet::load(key, &stored);
            }
        }
    }

    /// All QRV entries the user changed, for writing on OK.
    pub fn pending_qrvs(&self) -> Vec<Qrv> {
        let mut v = self.my.qrv.pending();
        v.extend(self.dx.qrv.pending());
        v
    }
}

/// A button of the tab that opens a dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StationAction {
    /// `MapStationDlg` of a station.
    Map(Who),
    /// `HorizonDlg` of a station.
    Horizon(Who),
    /// `LocalObstructionsDlg` (of My station).
    Obstructions,
}

/// Draw the tab; returns the button that was pressed.
pub fn show(ui: &mut Ui, s: &mut Settings, st: &mut StationsState) -> Option<StationAction> {
    let small = s.gui.locator_small_letters_for_subsquares;
    let mut action = None;
    for who in Who::BOTH {
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
        if let Some(a) = station(ui, who, call, lat, lon, height, st.edit_mut(who), small) {
            action = Some(a);
        }
        ui.separator();
    }
    qrv_grid(ui, st);
    action
}

#[allow(clippy::too_many_arguments)]
fn station(
    ui: &mut Ui,
    who: Who,
    call: &mut String,
    lat: &mut f64,
    lon: &mut f64,
    height: &mut f64,
    edit: &mut StationEdit,
    small: bool,
) -> Option<StationAction> {
    let n = who.name();
    let mut action = None;
    ui.strong(format!("{n} station"));
    Grid::new(format!("opt_station_{n}")).show(ui, |ui| {
        let l = ui.label(format!("{n} call"));
        let ok = callsign::check(call.trim());
        let mut t = egui::TextEdit::singleline(call).desired_width(120.0);
        if !ok {
            t = t.text_color(ui.visuals().error_fg_color);
        }
        ui.add(t).labelled_by(l.id);
        if !ok {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("{n} call is not valid"),
            );
        }
        ui.end_row();

        let l = ui.label(format!("{n} latitude"));
        ui.add(
            DragValue::new(lat)
                .range(-90.0..=90.0)
                .speed(0.0005)
                .fixed_decimals(6),
        )
        .labelled_by(l.id);
        ui.end_row();
        let l = ui.label(format!("{n} longitude"));
        ui.add(
            DragValue::new(lon)
                .range(-180.0..=180.0)
                .speed(0.0005)
                .fixed_decimals(6),
        )
        .labelled_by(l.id);
        ui.end_row();

        edit.sync_from_position(*lat, *lon, small);
        let l = ui.label(format!("{n} locator"));
        let valid = loc_is_valid(&edit.loc);
        let mut text = edit.loc.clone();
        let mut t = egui::TextEdit::singleline(&mut text).desired_width(120.0);
        if !valid {
            t = t.text_color(Color32::from_rgb(200, 60, 60));
        }
        if ui.add(t).labelled_by(l.id).changed() {
            edit.set_loc(&text, lat, lon);
        }
        ui.end_row();

        let l = ui.label(format!("{n} height [m]"));
        ui.add(DragValue::new(height).range(0.0..=10_000.0))
            .labelled_by(l.id);
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.button(format!("{n} map...")).clicked() {
            action = Some(StationAction::Map(who));
        }
        if ui.button(format!("{n} horizon...")).clicked() {
            action = Some(StationAction::Horizon(who));
        }
        if who == Who::My && ui.button("Local obstructions...").clicked() {
            action = Some(StationAction::Obstructions);
        }
    });
    action
}

fn qrv_grid(ui: &mut Ui, st: &mut StationsState) {
    ui.horizontal(|ui| {
        ui.strong("QRV data of");
        for who in Who::BOTH {
            ui.selectable_value(&mut st.selected, who, who.name());
        }
    });
    let set = &mut st.edit_mut(st.selected).qrv;
    let Some((call, loc)) = &set.key else {
        ui.label("Enter a valid call and position to edit QRV data.");
        return;
    };
    ui.label(format!("{call} {loc}"));
    Grid::new("opt_qrv").striped(true).show(ui, |ui| {
        for h in [
            "Band",
            "Antenna height [m]",
            "Antenna gain [dBd]",
            "Power [W]",
        ] {
            ui.strong(h);
        }
        ui.end_row();
        for r in &mut set.rows {
            ui.label(r.band.name());
            let band = r.band.name();
            let cells = [
                (&mut r.height, 0.0..=1000.0, "antenna height"),
                (&mut r.gain, -30.0..=100.0, "antenna gain"),
                (&mut r.power, 0.0..=100_000.0, "power"),
            ];
            for (v, range, what) in cells {
                let resp = ui.add(DragValue::new(v).range(range));
                let name = format!("{band} {what}");
                resp.widget_info(|| WidgetInfo::labeled(Role::SpinButton, true, name.clone()));
            }
            ui.end_row();
        }
    });
}
