//! `PlaneFeedDisclaimerDlg`: the text a feed asks the user to accept before it may be used.

use as_settings::Settings;
use eframe::egui;

/// Pending acceptance of one feed's disclaimer.
pub struct DisclaimerDlg {
    pub feed: String,
    pub text: String,
}

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisclaimerResult {
    Open,
    Accept,
    Decline,
}

impl DisclaimerDlg {
    pub fn new(feed: &str, text: &str) -> Self {
        Self {
            feed: feed.to_string(),
            text: text.to_string(),
        }
    }

    pub fn show(&self, ctx: &egui::Context) -> DisclaimerResult {
        let mut result = DisclaimerResult::Open;
        egui::Modal::new(egui::Id::unique("feed_disclaimer_dlg")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading(format!("Disclaimer: {}", self.feed));
            ui.label(&self.text);
            ui.horizontal(|ui| {
                if ui.button("Accept").clicked() {
                    result = DisclaimerResult::Accept;
                }
                if ui.button("Decline").clicked() {
                    result = DisclaimerResult::Decline;
                }
            });
        });
        result
    }
}

/// The user picked feed `name` for `draft`: a feed whose disclaimer is not yet accepted returns
/// the dialog to show and leaves `draft.feed` alone; any other feed is selected at once.
pub fn select_feed(draft: &mut Settings, name: &str) -> Option<DisclaimerDlg> {
    let text = as_feeds::registry::feed_by_name(name).and_then(|f| f.disclaimer());
    match text {
        Some(t) if !draft.feeds_disclaimer_accepted.contains(&name.to_string()) => {
            Some(DisclaimerDlg::new(name, t))
        }
        _ => {
            draft.feed = name.to_string();
            None
        }
    }
}

/// Apply the answer to `dlg`: Accept remembers the feed and selects it, Decline keeps the old
/// one. Returns the dialog while it is still open.
pub fn resolve(
    draft: &mut Settings,
    dlg: DisclaimerDlg,
    result: DisclaimerResult,
) -> Option<DisclaimerDlg> {
    match result {
        DisclaimerResult::Open => Some(dlg),
        DisclaimerResult::Accept => {
            if !draft.feeds_disclaimer_accepted.contains(&dlg.feed) {
                draft.feeds_disclaimer_accepted.push(dlg.feed.clone());
            }
            draft.feed = dlg.feed;
            None
        }
        DisclaimerResult::Decline => None,
    }
}
