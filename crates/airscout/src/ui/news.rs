//! Latest News tab and its update prompt.

use crate::app::AppState;
use eframe::egui;

pub(super) fn show(ui: &mut egui::Ui, url: &str) {
    ui.hyperlink(url);
    if ui.button("Open in browser").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

pub(super) fn prompt(ctx: &egui::Context, state: &mut AppState) {
    let Some(date) = state.news_prompt else {
        return;
    };
    let mut answer = None;
    egui::Modal::new(egui::Id::unique("website_news")).show(ctx, |ui| {
        ui.heading("Website News");
        ui.label(format!(
            "There are news on the website. Latest update: {date}\n Do you want to read it now?"
        ));
        ui.horizontal(|ui| {
            if ui.button("Yes").clicked() {
                answer = Some(true);
            }
            if ui.button("No").clicked() {
                answer = Some(false);
            }
        });
    });
    if let Some(yes) = answer {
        if let Some(url) = state.answer_news(yes) {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
    }
}
