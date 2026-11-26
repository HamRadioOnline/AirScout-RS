//! Multi tab of the control panel (`lv_Control_Watchlist`, MapDlg.cs:6059-6332, :5261-5287).

use crate::app::AppState;
use eframe::egui::{self, Color32, RichText, Sense};

/// `Color.Orange`.
pub const ORANGE: Color32 = Color32::from_rgb(255, 165, 0);
/// `Color.Red`.
pub const RED: Color32 = Color32::from_rgb(255, 0, 0);
/// `Color.Magenta`.
pub const MAGENTA: Color32 = Color32::from_rgb(255, 0, 255);

/// Row background of the best plane potential (`DrawItem`): above 75 magenta, above 50 red,
/// above 0 orange.
pub fn watch_row_color(potential: i32) -> Option<Color32> {
    match potential {
        76.. => Some(MAGENTA),
        51.. => Some(RED),
        1.. => Some(ORANGE),
        _ => None,
    }
}

struct Row {
    idx: usize,
    call: String,
    loc: String,
    checked: bool,
    out_of_range: bool,
}

/// Rows in display order (`RefreshWatchlistView`): checked first, then by call.
fn rows(state: &AppState) -> Vec<Row> {
    let mut v: Vec<Row> = state
        .settings
        .watchlist
        .0
        .iter()
        .enumerate()
        .map(|(idx, i)| Row {
            idx,
            call: i.call.clone(),
            loc: i.loc.clone(),
            checked: i.checked,
            out_of_range: i.out_of_range,
        })
        .collect();
    v.sort_by(|a, b| {
        b.checked
            .cmp(&a.checked)
            .then_with(|| a.call.cmp(&b.call))
            .then_with(|| a.loc.cmp(&b.loc))
    });
    v
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let manage = ui.add_enabled(
        crate::wintest_watch::manage_enabled(&state.settings),
        egui::Button::new("Manage"),
    );
    if manage.clicked() {
        state.open_watchlist_dlg();
    }
    ui.horizontal(|ui| {
        // a click on the column header checks all rows in range
        if ui.button("Call").clicked() {
            let n = state.settings.watchlist.0.len();
            for idx in 0..n {
                if !state.settings.watchlist.0[idx].out_of_range {
                    state.set_watch_checked(idx, true);
                }
            }
        }
        ui.label("Loc");
    });
    let rows = rows(state);
    egui::ScrollArea::vertical().show(ui, |ui| {
        for row in rows {
            let fill = state
                .watch_potential
                .get(&row.call)
                .and_then(|p| watch_row_color(*p));
            let frame = egui::Frame::new().fill(fill.unwrap_or(Color32::TRANSPARENT));
            let mut clicked = false;
            let mut hover = None;
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut c = row.checked;
                    let enabled = !row.out_of_range;
                    let r = ui.add_enabled(enabled, egui::Checkbox::without_text(&mut c));
                    r.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::CheckBox,
                            enabled,
                            c,
                            format!("check {}", row.call),
                        )
                    });
                    if r.changed() {
                        state.set_watch_checked(row.idx, c);
                    }
                    for text in [&row.call, &row.loc] {
                        let mut t = RichText::new(text);
                        if row.out_of_range {
                            t = t.color(Color32::GRAY);
                        } else if fill.is_some() {
                            // readable on the saturated backgrounds
                            t = t.color(Color32::BLACK);
                        }
                        let r = ui.add(egui::Label::new(t).sense(Sense::click()));
                        clicked |= r.clicked();
                        // the tooltip needs a station query: only for the row under the pointer
                        if r.hovered() {
                            hover.get_or_insert(r);
                        }
                    }
                });
            });
            if clicked {
                state.set_dx_from_watch(row.idx);
            }
            // the query runs only for the hovered row, not for every row of every frame
            if let Some(r) = hover {
                if let Some((qrb, qtf)) = state.watch_qrb_qtf(&row.call, &row.loc) {
                    r.on_hover_text(format!("{}\n{}\n{qrb:.0} km\n{qtf:.0}°", row.call, row.loc));
                }
            }
        }
    });
}
