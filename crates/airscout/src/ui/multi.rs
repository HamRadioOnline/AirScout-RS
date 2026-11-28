//! Multi tab of the control panel (`lv_Control_Watchlist`, MapDlg.cs:6059-6332, :5261-5287).

use crate::app::AppState;
use crate::ui::forms::trace;
use crate::ui::forms::widgets::{button, label, scoped};
use eframe::egui::{self, pos2, vec2, Color32, Rect, RichText, Sense};

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
    let page = ui.max_rect().shrink2(vec2(7.0, 0.0));
    let manage_rect = Rect::from_min_max(pos2(page.min.x, page.max.y - 23.0), page.max);
    let list = Rect::from_min_max(page.min, pos2(page.max.x, manage_rect.min.y));
    trace::scope(ui, "lv_Control_Watchlist", list, |ui| {
        scoped(ui, list, |ui| watchlist(ui, state));
    });

    let enabled = crate::wintest_watch::manage_enabled(&state.settings);
    if ui
        .add_enabled_ui(enabled, |ui| {
            button(
                ui,
                "btn_Control_Manage_Watchlist",
                "Manage Watchlist",
                manage_rect,
            )
        })
        .inner
        .clicked()
    {
        state.open_watchlist_dlg();
    }
}

const CALL_WIDTH: f32 = 86.0;
const HEADER_HEIGHT: f32 = 23.0;

fn watchlist(ui: &mut egui::Ui, state: &mut AppState) {
    let list = ui.max_rect();
    let call = Rect::from_min_size(list.min, vec2(CALL_WIDTH, HEADER_HEIGHT));
    let loc = Rect::from_min_max(pos2(call.max.x, call.min.y), pos2(list.max.x, call.max.y));
    if button(ui, "ch_Control_Watchlist_Call", "Call", call).clicked() {
        for idx in 0..state.settings.watchlist.0.len() {
            if !state.settings.watchlist.0[idx].out_of_range {
                state.set_watch_checked(idx, true);
            }
        }
    }
    label(ui, "ch_Control_Watchlist_Loc", "Loc", loc);

    let rows_rect = Rect::from_min_max(pos2(list.min.x, call.max.y), list.max);
    scoped(ui, rows_rect, |ui| watch_rows(ui, state));
}

fn watch_rows(ui: &mut egui::Ui, state: &mut AppState) {
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
                let (_, row_rect) = ui.allocate_space(vec2(ui.available_width(), 20.0));
                let check = Rect::from_min_size(row_rect.min, vec2(20.0, row_rect.height()));
                let call = Rect::from_min_max(
                    check.right_top(),
                    pos2(row_rect.min.x + CALL_WIDTH, row_rect.max.y),
                );
                let loc = Rect::from_min_max(call.right_top(), row_rect.max);
                let mut c = row.checked;
                let enabled = !row.out_of_range;
                let r = ui
                    .add_enabled_ui(enabled, |ui| {
                        ui.put(check, egui::Checkbox::without_text(&mut c))
                    })
                    .inner;
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
                for (text, rect) in [(&row.call, call), (&row.loc, loc)] {
                    let mut t = RichText::new(text);
                    if row.out_of_range {
                        t = t.color(Color32::GRAY);
                    } else if fill.is_some() {
                        t = t.color(Color32::BLACK);
                    }
                    let r = ui.put(rect, egui::Label::new(t).truncate().sense(Sense::click()));
                    clicked |= r.clicked();
                    if r.hovered() {
                        hover.get_or_insert(r);
                    }
                }
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
