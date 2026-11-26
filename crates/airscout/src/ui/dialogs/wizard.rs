//! `FirstRunWizard`: terms, coverage, elevation models, user, plane feed, finish. Shown when the
//! settings file did not exist; nothing starts and nothing is written before Finish.

use super::disclaimer::{self, DisclaimerDlg, DisclaimerResult};
use crate::app::parse_position;
use crate::ui::options::{num, planes, validate_area};
use as_geo::maidenhead;
use as_settings::Settings;
use as_stations::callsign;
use eframe::egui::{self, CentralPanel, Grid, ScrollArea, TextEdit, Ui};

/// The upstream terms file, byte for byte (it begins with a UTF-8 BOM).
const TERMS: &str = include_str!("../../../assets/TERMSANDCONDITIONS");

/// The terms for display: no BOM, no carriage returns.
pub fn terms_text() -> String {
    TERMS.trim_start_matches('\u{feff}').replace('\r', "")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WizardPage {
    Terms,
    Coverage,
    Elevation,
    User,
    Feed,
    Finish,
}

impl WizardPage {
    const ALL: [WizardPage; 6] = [
        WizardPage::Terms,
        WizardPage::Coverage,
        WizardPage::Elevation,
        WizardPage::User,
        WizardPage::Feed,
        WizardPage::Finish,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    fn title(self) -> &'static str {
        match self {
            WizardPage::Terms => "Terms and conditions",
            WizardPage::Coverage => "Coverage",
            WizardPage::Elevation => "Elevation models",
            WizardPage::User => "User",
            WizardPage::Feed => "Plane feed",
            WizardPage::Finish => "Finish",
        }
    }
}

/// What the user asked for this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WizardAction {
    None,
    Finish,
    /// Cancel: the app exits without writing settings.
    Cancel,
}

/// The wizard without any drawing: the page and the settings draft it edits.
pub struct WizardState {
    pub page: WizardPage,
    pub draft: Settings,
    pub terms_accepted: bool,
    /// Text of the locator field; follows the position and moves it when valid.
    pub loc_text: String,
    /// Why the last Next was refused.
    pub error: Option<String>,
    /// The disclaimer of the feed just picked (the draft keeps the old feed until accepted).
    pub disclaimer_dlg: Option<DisclaimerDlg>,
}

impl WizardState {
    pub fn new(settings: &Settings) -> Self {
        Self {
            page: WizardPage::Terms,
            loc_text: settings.my_loc().unwrap_or_default(),
            draft: settings.clone(),
            terms_accepted: false,
            error: None,
            disclaimer_dlg: None,
        }
    }

    #[allow(clippy::should_implement_trait)] // the brief names it `next`
    /// Leave the current page for the next one if it is valid; `Err` says why not. On the
    /// last page nothing happens (the Finish button ends the wizard).
    pub fn next(&mut self) -> Result<(), String> {
        let check = self.validate_page();
        self.error = check.clone().err();
        check?;
        if let Some(p) = WizardPage::ALL.get(self.page.index() + 1) {
            self.page = *p;
        }
        Ok(())
    }

    pub fn back(&mut self) {
        self.error = None;
        if let Some(i) = self.page.index().checked_sub(1) {
            self.page = WizardPage::ALL[i];
        }
    }

    fn validate_page(&mut self) -> Result<(), String> {
        match self.page {
            WizardPage::Terms => {
                if !self.terms_accepted {
                    return Err("Accept the terms and conditions to continue".into());
                }
            }
            WizardPage::Coverage => {
                let e = validate_area(&self.draft);
                if !e.is_empty() {
                    return Err(e.join("; "));
                }
            }
            WizardPage::Elevation => {
                let e = &self.draft.elevation_enabled;
                if !(e.globe || e.srtm3 || e.srtm1 || e.aster3 || e.aster1) {
                    return Err("Enable at least one elevation model".into());
                }
            }
            WizardPage::User => {
                let call = self.draft.my_call.trim().to_uppercase();
                if !callsign::check(&call) {
                    return Err("The call sign is not valid".into());
                }
                // a typo would silently keep the old position: refuse it
                if parse_position(&self.loc_text).is_none() {
                    return Err(
                        "The locator is not valid (e.g. JO50AA, or lat,lon in degrees)".into(),
                    );
                }
                if !maidenhead::check_lat_lon(self.draft.my_lat, self.draft.my_lon) {
                    return Err("The position is not valid".into());
                }
                self.draft.my_call = call;
            }
            WizardPage::Feed | WizardPage::Finish => {}
        }
        Ok(())
    }

    /// Edit of the locator field: a valid text (locator or `lat,lon`) moves the station.
    pub fn set_loc_text(&mut self, text: &str) {
        self.loc_text = text.to_string();
        if let Some(p) = parse_position(text) {
            self.draft.my_lat = p.lat;
            self.draft.my_lon = p.lon;
        }
    }

    /// Edit of latitude or longitude: the locator field follows.
    pub fn set_position(&mut self, lat: f64, lon: f64) {
        self.draft.my_lat = lat;
        self.draft.my_lon = lon;
        self.loc_text = self.draft.my_loc().unwrap_or_default();
    }

    /// The settings to run with: the draft, first run done.
    pub fn finished_settings(&self) -> Settings {
        let mut s = self.draft.clone();
        s.first_run = false;
        s
    }

    /// Draw the wizard over the whole window. Nothing else is drawn meanwhile, so the map
    /// fetches no tiles.
    pub fn show(&mut self, root: &mut Ui) -> WizardAction {
        let ctx = root.ctx().clone();
        let mut action = WizardAction::None;
        CentralPanel::default().show(root, |ui| {
            ui.heading(format!(
                "AirScout first-run wizard: {} ({}/{})",
                self.page.title(),
                self.page.index() + 1,
                WizardPage::ALL.len()
            ));
            ui.separator();
            ScrollArea::vertical()
                .max_height((ui.available_height() - 70.0).max(100.0))
                .auto_shrink(false)
                .show(ui, |ui| self.page_body(ui));
            ui.separator();
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    action = WizardAction::Cancel;
                }
                if ui
                    .add_enabled(self.page != WizardPage::Terms, egui::Button::new("Back"))
                    .clicked()
                {
                    self.back();
                }
                if self.page == WizardPage::Finish {
                    if ui.button("Finish").clicked() {
                        action = WizardAction::Finish;
                    }
                } else if ui.button("Next").clicked() {
                    let _ = self.next();
                }
            });
        });
        self.show_disclaimer(&ctx);
        action
    }

    fn show_disclaimer(&mut self, ctx: &egui::Context) {
        if let Some(dlg) = self.disclaimer_dlg.take() {
            let result: DisclaimerResult = dlg.show(ctx);
            self.disclaimer_dlg = disclaimer::resolve(&mut self.draft, 1, dlg, result);
        }
    }

    fn page_body(&mut self, ui: &mut Ui) {
        match self.page {
            WizardPage::Terms => {
                ui.label(terms_text());
                ui.checkbox(
                    &mut self.terms_accepted,
                    "I accept the terms and conditions",
                );
            }
            WizardPage::Coverage => {
                ui.label("The area in which AirScout keeps stations and plane data.");
                let a = &mut self.draft.area;
                Grid::new("wiz_area").num_columns(2).show(ui, |ui| {
                    num(ui, "Min Lat", &mut a.min_lat, -90.0..=90.0);
                    num(ui, "Max Lat", &mut a.max_lat, -90.0..=90.0);
                    num(ui, "Min Lon", &mut a.min_lon, -180.0..=180.0);
                    num(ui, "Max Lon", &mut a.max_lon, -180.0..=180.0);
                });
            }
            WizardPage::Elevation => {
                ui.label(
                    "GLOBE is a small download. SRTM and ASTER are large downloads, but more \
                     detailed. At least one model is required.",
                );
                let e = &mut self.draft.elevation_enabled;
                ui.checkbox(&mut e.globe, "GLOBE (small)");
                ui.checkbox(&mut e.srtm3, "SRTM3 (large)");
                ui.checkbox(&mut e.srtm1, "SRTM1 (large)");
                ui.checkbox(&mut e.aster3, "ASTER3 (large)");
                ui.checkbox(&mut e.aster1, "ASTER1 (large)");
            }
            WizardPage::User => self.user_page(ui),
            WizardPage::Feed => {
                ui.label("Where AirScout gets its plane positions.");
                if let planes::FeedAction::Select(slot, name) =
                    planes::feed_combo(ui, &self.draft, 1)
                {
                    self.disclaimer_dlg = disclaimer::select_feed(&mut self.draft, slot, &name);
                }
                planes::feed_info(ui, &self.draft, 1);
            }
            WizardPage::Finish => {
                ui.label("Finish saves the settings and starts AirScout.");
                ui.label("The settings can be changed later in Options.");
            }
        }
    }

    fn user_page(&mut self, ui: &mut Ui) {
        ui.label("Your station. Nothing is sent anywhere.");
        let (mut lat, mut lon) = (self.draft.my_lat, self.draft.my_lon);
        let mut loc = self.loc_text.clone();
        Grid::new("wiz_user").num_columns(2).show(ui, |ui| {
            let l = ui.label("Call");
            ui.add(TextEdit::singleline(&mut self.draft.my_call).desired_width(160.0))
                .labelled_by(l.id);
            ui.end_row();
            let l = ui.label("Locator");
            let r = ui.add(TextEdit::singleline(&mut loc).desired_width(160.0));
            r.labelled_by(l.id);
            ui.end_row();
            num(ui, "Lat", &mut lat, -90.0..=90.0);
            num(ui, "Lon", &mut lon, -180.0..=180.0);
        });
        if loc != self.loc_text {
            self.set_loc_text(&loc);
        } else if (lat, lon) != (self.draft.my_lat, self.draft.my_lon) {
            self.set_position(lat, lon);
        }
    }
}
