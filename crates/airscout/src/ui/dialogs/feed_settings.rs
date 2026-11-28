//! `PlaneFeedSettingsDlg`: a generic editor over a feed's settings table.

use as_feeds::registry::feed_by_name;
use as_settings::{FeedSettings, SettingsHost};
use eframe::egui::{self, DragValue, TextEdit};
use toml::{Table, Value};

pub struct FeedSettingsDlg {
    pub feed: String,
    /// The table being edited.
    pub table: Table,
    secret_keys: &'static [&'static str],
    show_secrets: bool,
    /// Why the last OK was refused. Credential-free by the as-feeds contract.
    pub error: Option<String>,
}

/// What the user did with the dialog this frame.
#[derive(Clone, PartialEq)]
pub enum FeedSettingsResult {
    Open,
    Ok(Table),
    Cancel,
}

// The table holds credentials: Debug never prints it.
impl std::fmt::Debug for FeedSettingsResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => f.write_str("Open"),
            Self::Ok(_) => f.write_str("Ok(<table>)"),
            Self::Cancel => f.write_str("Cancel"),
        }
    }
}

pub(crate) fn desktop_default_feed(name: &str) -> Option<Box<dyn as_feeds::PlaneFeed>> {
    let mut feed = feed_by_name(name)?;
    if name == "AirScoutServer" {
        let defaults = FeedSettings::for_host(SettingsHost::Desktop);
        feed.load_settings(defaults.get(name)?.as_table()?).ok()?;
    }
    Some(feed)
}

impl FeedSettingsDlg {
    /// Editor for `feed`, starting from `stored` (missing keys show the feed's defaults).
    pub fn new(feed: &str, stored: Option<&Table>) -> Option<Self> {
        let mut f = desktop_default_feed(feed)?;
        let mut error = None;
        if let Some(t) = stored {
            // an unusable stored table falls back to the defaults; say so, because OK would
            // overwrite it (the message is credential-free by the as-feeds contract)
            if let Err(e) = f.load_settings(t) {
                f = desktop_default_feed(feed)?;
                error = Some(format!(
                    "Stored settings are invalid and replaced by defaults: {e}"
                ));
            }
        }
        Some(Self {
            feed: feed.to_string(),
            table: f.settings(),
            secret_keys: f.secret_keys(),
            show_secrets: false,
            error,
        })
    }

    /// "Default": the feed's own defaults.
    pub fn restore_defaults(&mut self) {
        if let Some(f) = desktop_default_feed(&self.feed) {
            self.table = f.settings();
        }
        self.error = None;
    }

    /// OK: the table is stored only if a fresh feed accepts it.
    pub fn try_ok(&mut self) -> Option<Table> {
        let Some(mut f) = feed_by_name(&self.feed) else {
            self.error = Some(format!("unknown feed {}", self.feed));
            return None;
        };
        match f.load_settings(&self.table) {
            Ok(()) => {
                self.error = None;
                let mut table = self.table.clone();
                as_settings::canonicalize_feed_table(&self.feed, &mut table);
                Some(table)
            }
            Err(e) => {
                self.error = Some(e.to_string());
                None
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> FeedSettingsResult {
        let mut result = FeedSettingsResult::Open;
        egui::Modal::new(egui::Id::unique("feed_settings_dlg")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.heading(format!("Settings: {}", self.feed));
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .auto_shrink(false)
                .show(ui, |ui| {
                    egui::Grid::new("feed_settings")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (key, value) in self.table.iter_mut() {
                                let l = ui.label(key.as_str());
                                let secret = self.secret_keys.contains(&key.as_str());
                                let r = match value {
                                    Value::String(s) => ui.add(
                                        TextEdit::singleline(s)
                                            .password(secret && !self.show_secrets)
                                            .desired_width(300.0),
                                    ),
                                    Value::Integer(i) => ui.add(DragValue::new(i)),
                                    Value::Float(x) => ui.add(DragValue::new(x)),
                                    Value::Boolean(b) => ui.checkbox(b, ""),
                                    _ => ui.label("(not editable)"),
                                };
                                r.labelled_by(l.id);
                                ui.end_row();
                            }
                        });
                });
            ui.label(
                "The web server serves this feed's URL with its username, password and query \
                 string removed. A credential placed in the URL path is served as configured and \
                 still reaches the network: do not put a token in the path.",
            );
            if !self.secret_keys.is_empty() {
                ui.checkbox(&mut self.show_secrets, "Show credentials");
            }
            if let Some(e) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    if let Some(t) = self.try_ok() {
                        result = FeedSettingsResult::Ok(t);
                    }
                }
                if ui.button("Default").clicked() {
                    self.restore_defaults();
                }
                if ui.button("Cancel").clicked() {
                    result = FeedSettingsResult::Cancel;
                }
            });
        });
        result
    }
}
