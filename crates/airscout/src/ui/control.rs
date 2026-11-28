//! Right column of MapDlg (`sc_Main.Panel2`).

use crate::app::{parse_position, AppState, ControlBox, ControlTab};
use crate::ui::forms::geometry::{designer_rect, dock_layout, Dock};
use crate::ui::forms::trace::{self, Kind};
use crate::ui::forms::widgets::{
    button, checkbox, group_box, label, scoped, tab_control_with_selection,
};
use as_aircraft::{enums::LifeMode, PlaneCategory};
use as_geo::band::Band;
use as_geo::maidenhead;
use eframe::egui::{self, pos2, vec2, Color32, ComboBox, DragValue, Rect, Sense, TextEdit};

const LIGHT_SALMON: Color32 = Color32::from_rgb(255, 160, 122);
const PALE_GREEN: Color32 = Color32::from_rgb(152, 251, 152);
const FLORAL_WHITE: Color32 = Color32::from_rgb(255, 250, 240);

// GroupBox.Height - DisplayRectangle.Height: 16 pt above the body, 3 below it.
const MINIMIZED_HEIGHT: f32 = 19.0;
// Cover the egui caption down to the start of the earliest group body.
const TITLE_CLICK_HEIGHT: f32 = 16.0;

const COMMON_HEIGHT: f32 = 100.0;
const BUTTONS_HEIGHT: f32 = 123.0;
const ZOOM_HEIGHT: f32 = 79.0;
const MIN_TABS_HEIGHT: f32 = 86.0;

const TABS: [(ControlTab, (&str, &str)); 3] = [
    (ControlTab::Single, ("tp_Control_Single", "Single")),
    (ControlTab::Multi, ("tp_Control_Multi", "Multi")),
    (ControlTab::Opt, ("tp_Control_Options", "Opt")),
];

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let rect = ui.max_rect();
    let locators = ["cb_MyLoc", "cb_DXLoc"].map(|name| ui.make_persistent_id(name));
    let zoom_height = group_height(state.settings.gui.show_zoom_box, ZOOM_HEIGHT);
    let min_size = vec2(
        152.0,
        COMMON_HEIGHT + MIN_TABS_HEIGHT + zoom_height + BUTTONS_HEIGHT,
    );
    if rect.width() < min_size.x || rect.height() < min_size.y {
        // Keep the entire column reachable when the viewport cannot even fit
        // the common panel, the bottom boxes and a usable tab viewport.
        egui::ScrollArea::both()
            .id_salt("map_control_column")
            .show(ui, |ui| {
                let size = rect.size().max(min_size);
                let layout = Rect::from_min_size(ui.cursor().min, size);
                ui.set_min_size(size);
                draw_column(ui, state, layout, zoom_height, locators);
            });
    } else {
        draw_column(ui, state, rect, zoom_height, locators);
    }
}

fn draw_column(
    ui: &mut egui::Ui,
    state: &mut AppState,
    rect: Rect,
    zoom_height: f32,
    locators: [egui::Id; 2],
) {
    let docks = [
        (Dock::Fill, egui::Vec2::ZERO),
        (Dock::Bottom, vec2(0.0, zoom_height)),
        (Dock::Top, vec2(0.0, COMMON_HEIGHT)),
        (Dock::Bottom, vec2(0.0, BUTTONS_HEIGHT)),
    ];
    let rects = dock_layout(rect, &docks);
    common_info(ui, state, rects[2]);
    tabs(ui, state, rects[0], locators);
    zoom(ui, state, rects[1]);
    buttons(ui, state, rects[3]);
    // Check focus after later controls have had a chance to claim it.
    if let Some((response, text)) =
        invalid_locators(ui, state, locators).find(|(response, _)| response.lost_focus())
    {
        response.ctx.data_mut(|data| {
            data.insert_temp(response.id.with("validation_error"), text.to_owned());
        });
        response.request_focus();
        response.ctx.request_repaint();
    }
}

fn common_info(ui: &mut egui::Ui, state: &mut AppState, rect: Rect) {
    trace::scope(ui, "pa_CommonInfo", rect, |ui| {
        let utc_label = label(ui, "label1", "UTC", designer_rect(rect, [4, 3], [29, 16]));
        let utc = state.current_time(chrono::Utc::now());
        let value = utc.format("%Y-%m-%d %H:%M:%S").to_string();
        let fill = if state.settings.time.online {
            LIGHT_SALMON
        } else {
            super::status::ALARM_PLUM
        };
        let utc = text_box(
            ui,
            "tb_UTC",
            &mut value.as_str(),
            designer_rect(rect, [4, 19], [144, 22]),
            Some(fill),
        )
        .labelled_by(utc_label.id);
        if utc.double_clicked() {
            state.open_set_time_dlg();
        }
        let band_label = label(ui, "label8", "Band", designer_rect(rect, [4, 44], [32, 16]));
        let band_rect = designer_rect(rect, [8, 60], [130, 24]);
        trace::record(
            ui,
            "cb_Band",
            Kind::Text,
            state.settings.band.name(),
            band_rect,
        );
        scoped(ui, band_rect, |ui| {
            ui.add_enabled_ui(!state.is_playing(), |ui| {
                ComboBox::from_id_salt("cb_Band")
                    .width(band_rect.width())
                    .selected_text(state.settings.band.name())
                    .show_ui(ui, |ui| {
                        for b in Band::all_except_none_and_all() {
                            ui.selectable_value(&mut state.settings.band, *b, b.name());
                        }
                    })
                    .response
                    .labelled_by(band_label.id);
            });
        });
    });
}

fn tabs(ui: &mut egui::Ui, state: &mut AppState, rect: Rect, locators: [egui::Id; 2]) {
    let current = state.control_tab as usize;
    let mut active = current;
    // A tab change would remove the locator before its lost focus can be handled.
    let allow_change = !state.is_playing() && !locator_blocks_navigation(ui, state, locators);
    tab_control_with_selection(
        ui,
        "tc_Control",
        rect,
        &TABS.map(|(_, page)| page),
        &mut active,
        allow_change,
        |ui, idx| {
            let tab = TABS[idx].0;
            if idx != current {
                state.set_control_tab(tab);
            }
            match tab {
                ControlTab::Single => {
                    let height = group_height(state.settings.gui.show_info_box, 275.0);
                    page_body(ui, "map_info_scroll", height, |ui| {
                        single(ui, state, locators);
                    });
                }
                ControlTab::Multi => super::multi::show(ui, state),
                ControlTab::Opt => {
                    let gui = &state.settings.gui;
                    let filter = group_height(gui.show_filter_box, 110.0);
                    let alarm = group_height(gui.show_alarm_box, 47.0);
                    page_body(ui, "map_opt_scroll", filter + alarm, |ui| {
                        opt(ui, state, filter, alarm)
                    });
                }
            }
        },
    );
}

/// Fixed-height group contents scroll inside the tab when required.
fn page_body(ui: &mut egui::Ui, id: &str, height: f32, body: impl FnOnce(&mut egui::Ui)) {
    let rect = ui.max_rect();
    if height > rect.height() {
        egui::ScrollArea::vertical().id_salt(id).show(ui, |ui| {
            let size = vec2(rect.width(), height);
            let layout = Rect::from_min_size(ui.cursor().min, size);
            ui.set_min_size(size);
            scoped(ui, layout, body);
        });
    } else {
        scoped(
            ui,
            Rect::from_min_size(rect.min, vec2(rect.width(), height)),
            body,
        );
    }
}

fn group_height(open: bool, full_height: f32) -> f32 {
    if open {
        full_height
    } else {
        MINIMIZED_HEIGHT
    }
}

fn title_clicked(ui: &egui::Ui, name: &str, rect: Rect, expanded: bool) -> bool {
    let title = if expanded {
        Rect::from_min_size(rect.min, vec2(rect.width(), TITLE_CLICK_HEIGHT))
    } else {
        rect
    };
    ui.interact(title, ui.make_persistent_id(name), Sense::click())
        .clicked_by(egui::PointerButton::Primary)
}

fn text_box(
    ui: &mut egui::Ui,
    name: &str,
    value: &mut dyn egui::TextBuffer,
    rect: Rect,
    fill: Option<Color32>,
) -> egui::Response {
    trace::record(ui, name, Kind::Text, value.as_str(), rect);
    edit_text(
        ui,
        ui.make_persistent_id(name),
        value,
        rect,
        fill.map(|fill| (fill, Color32::BLACK)),
    )
}

fn edit_text(
    ui: &mut egui::Ui,
    id: egui::Id,
    value: &mut dyn egui::TextBuffer,
    rect: Rect,
    colors: Option<(Color32, Color32)>,
) -> egui::Response {
    let mut edit = TextEdit::singleline(value)
        .id(id)
        .margin(egui::Margin::symmetric(4, 2))
        .desired_width(rect.width())
        .min_size(rect.size());
    if let Some((background, foreground)) = colors {
        edit = edit.background_color(background).text_color(foreground);
    }
    ui.put(rect, edit)
}

fn edit_locator(
    ui: &mut egui::Ui,
    id: egui::Id,
    text: &mut String,
    rect: Rect,
    fill: Color32,
) -> egui::Response {
    let error_id = id.with("validation_error");
    let error_text = ui.data(|data| data.get_temp::<String>(error_id));
    let has_error = error_text.as_ref() == Some(text);
    let colors = if has_error {
        (Color32::RED, Color32::WHITE)
    } else {
        (fill, Color32::BLACK)
    };
    let response = edit_text(ui, id, text, rect, Some(colors));
    if error_text.is_some() && (response.changed() || !has_error) {
        ui.data_mut(|data| data.remove::<String>(error_id));
        ui.ctx().request_repaint();
    }
    response
}

fn invalid_locators<'a>(
    ui: &'a egui::Ui,
    state: &'a AppState,
    ids: [egui::Id; 2],
) -> impl Iterator<Item = (egui::Response, &'a str)> {
    ids.into_iter()
        .zip([state.my_loc_text.as_str(), state.dx_loc_text.as_str()])
        .filter(|(_, text)| !text.is_empty() && parse_position(text).is_none())
        .filter_map(move |(id, text)| ui.ctx().read_response(id).map(|response| (response, text)))
        .filter(|(response, _)| response.enabled())
}

fn locator_blocks_navigation(ui: &egui::Ui, state: &AppState, ids: [egui::Id; 2]) -> bool {
    invalid_locators(ui, state, ids)
        .any(|(response, _)| response.has_focus() || response.lost_focus())
}

fn station_field<T: Clone + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    (label_name, caption, field_name): (&str, &str, &str),
    (label_rect, field_rect): (Rect, Rect),
    text: &mut String,
    enabled: bool,
    locator: Option<(egui::Id, Color32)>,
    items: impl FnOnce() -> Vec<(String, T)>,
) -> (bool, Option<T>) {
    let label = label(ui, label_name, caption, label_rect);
    trace::record(ui, field_name, Kind::Text, text, field_rect);
    ui.add_enabled_ui(enabled, |ui| {
        let split = field_rect.right() - 20.0;
        let edit_rect = Rect::from_min_max(field_rect.min, pos2(split, field_rect.bottom()));
        let button_rect = Rect::from_min_max(pos2(split, field_rect.top()), field_rect.max);
        let edit = match locator {
            Some((id, fill)) => edit_locator(ui, id, text, edit_rect, fill),
            None => edit_text(ui, ui.make_persistent_id(field_name), text, edit_rect, None),
        };
        let changed = edit.labelled_by(label.id).changed();
        let button = ui.put(
            button_rect,
            egui::Button::new("").min_size(button_rect.size()),
        );
        button.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, enabled, format!("{caption} choices"))
        });
        let center = button.rect.center();
        let arrow = vec![
            center + vec2(-4.0, -2.0),
            center + vec2(4.0, -2.0),
            center + vec2(0.0, 2.0),
        ];
        ui.painter().add(egui::Shape::convex_polygon(
            arrow,
            ui.style().interact(&button).text_color(),
            egui::Stroke::NONE,
        ));

        let id = ui.make_persistent_id((field_name, "choices"));
        if !enabled {
            egui::Popup::close_id(ui.ctx(), id);
            return (changed, None);
        }
        if button.clicked() && !egui::Popup::is_id_open(ui.ctx(), id) {
            let choices = items();
            ui.data_mut(|d| d.insert_temp(id, choices));
        }
        let chosen = egui::Popup::menu(&button)
            .id(id)
            .anchor(field_rect)
            .width(field_rect.width())
            .show(|ui| {
                let items = ui
                    .data(|d| d.get_temp::<Vec<(String, T)>>(id))
                    .unwrap_or_default();
                egui::ScrollArea::vertical()
                    .max_height(ui.spacing().combo_height)
                    .show(ui, |ui| {
                        let mut chosen = None;
                        for (label, value) in items {
                            if ui.selectable_label(label == *text, label).clicked() {
                                chosen = Some(value);
                                ui.close();
                            }
                        }
                        chosen
                    })
                    .inner
            })
            .and_then(|r| r.inner);
        (changed, chosen)
    })
    .inner
}

fn call_choices(calls: Vec<String>) -> Vec<(String, String)> {
    calls.into_iter().map(|call| (call.clone(), call)).collect()
}

/// Pale green for a precise position, floral white for incomplete or invalid edits.
fn loc_fill(text: &str, lat: f64, lon: f64) -> Color32 {
    if text.len() >= 6
        && maidenhead::check(text)
        && maidenhead::is_precise(lat, lon, 3).unwrap_or(false)
    {
        PALE_GREEN
    } else {
        FLORAL_WHITE
    }
}

fn single(ui: &mut egui::Ui, state: &mut AppState, locators: [egui::Id; 2]) {
    let page = ui.max_rect();
    let rect = page.shrink2(vec2(7.0, 0.0));
    let expanded = state.settings.gui.show_info_box;
    group_box(ui, "gb_Map_Info", "Info", rect, |ui| {
        if !expanded {
            return;
        }
        let enabled = !state.is_playing();
        let rects = |label_y, field_y| {
            (
                designer_rect(rect, [3, label_y], [50, 13]),
                designer_rect(rect, [3, field_y], [133, 24]),
            )
        };
        let mut my_call = state.settings.my_call.clone();
        let (changed, selected) = station_field(
            ui,
            ("label20", "My Call", "cb_MyCall"),
            rects(18, 34),
            &mut my_call,
            enabled,
            None,
            || call_choices(state.my_call_items()),
        );
        if let Some(call) = selected {
            state.my_call_selected(&call);
        } else if changed {
            state.my_call_changed(&my_call);
        }

        let s = &state.settings;
        let fill = loc_fill(&state.my_loc_text, s.my_lat, s.my_lon);
        let mut my_loc = state.my_loc_text.clone();
        let (changed, selected) = station_field(
            ui,
            ("label19", "My Loc", "cb_MyLoc"),
            rects(58, 71),
            &mut my_loc,
            enabled,
            Some((locators[0], fill)),
            || state.my_loc_items(),
        );
        if let Some(p) = selected {
            state.my_loc_selected(p);
        } else if changed {
            state.set_my_loc_text(&my_loc);
        }

        let mut dx_call = state.settings.dx_call.clone();
        let (changed, selected) = station_field(
            ui,
            ("label18", "DX Call", "cb_DXCall"),
            rects(97, 113),
            &mut dx_call,
            enabled,
            None,
            || call_choices(state.dx_call_items()),
        );
        if let Some(call) = selected {
            state.dx_call_selected(&call);
        } else if changed {
            state.dx_call_changed(&dx_call);
        }

        let s = &state.settings;
        let fill = loc_fill(&state.dx_loc_text, s.dx_lat, s.dx_lon);
        let mut dx_loc = state.dx_loc_text.clone();
        let (changed, selected) = station_field(
            ui,
            ("label17", "DX Loc", "cb_DXLoc"),
            rects(139, 154),
            &mut dx_loc,
            enabled,
            Some((locators[1], fill)),
            || state.dx_loc_items(),
        );
        if let Some(p) = selected {
            state.dx_loc_selected(p);
        } else if changed {
            state.set_dx_loc_text(&dx_loc);
        }
        let (qtf, qrb) = state.qtf_qrb();
        for (name, caption, field, label_y, field_y, value) in [
            ("label16", "QRB", "tb_QRB", 181, 198, qrb),
            ("label6", "QTF", "tb_QTF", 226, 242, qtf),
        ] {
            let l = label(
                ui,
                name,
                caption,
                designer_rect(rect, [3, label_y], [32, 16]),
            );
            text_box(
                ui,
                field,
                &mut value.to_string().as_str(),
                designer_rect(rect, [3, field_y], [133, 22]),
                Some(FLORAL_WHITE),
            )
            .labelled_by(l.id);
        }
    });
    if title_clicked(ui, "gb_Map_Info_title", rect, expanded)
        && !locator_blocks_navigation(ui, state, locators)
    {
        state.toggle_box(ControlBox::Info);
    }
}

fn zoom(ui: &mut egui::Ui, state: &mut AppState, rect: Rect) {
    let expanded = state.settings.gui.show_zoom_box;
    group_box(ui, "gb_Map_Zoom", "Map Zoom", rect, |ui| {
        if !expanded {
            return;
        }
        let panel = designer_rect(rect, [3, 16], [146, 60]);
        trace::scope(ui, "pa_Map_Zoom", panel, |ui| {
            if button(
                ui,
                "btn_Zoom_In",
                "+",
                designer_rect(panel, [17, 8], [30, 23]),
            )
            .clicked()
            {
                state.pending_zoom += 1;
            }
            let value = format!("{:.0}", state.map_zoom);
            text_box(
                ui,
                "tb_Zoom",
                &mut value.as_str(),
                designer_rect(panel, [53, 9], [41, 20]),
                Some(FLORAL_WHITE),
            );
            if button(
                ui,
                "btn_Zoom_Out",
                "-",
                designer_rect(panel, [100, 8], [30, 23]),
            )
            .clicked()
            {
                state.pending_zoom -= 1;
            }
            checkbox(
                ui,
                "cb_AutoCenter",
                "Auto Center",
                &mut state.settings.gui.map_auto_center,
                designer_rect(panel, [20, 37], [100, 19]),
            );
        });
    });
    if title_clicked(ui, "gb_Map_Zoom_title", rect, expanded) {
        state.toggle_box(ControlBox::Zoom);
    }
}

fn buttons(ui: &mut egui::Ui, state: &mut AppState, rect: Rect) {
    group_box(ui, "gb_Map_Buttons", "Control", rect, |ui| {
        let options = ui.add_enabled_ui(!state.is_playing(), |ui| {
            button(
                ui,
                "btn_Options",
                "Options",
                designer_rect(rect, [19, 18], [114, 29]),
            )
        });
        if options.inner.clicked() {
            state.open_options_dlg();
        }
        if button(
            ui,
            "btn_Map_Save",
            "Save",
            designer_rect(rect, [19, 53], [114, 29]),
        )
        .clicked()
        {
            state.request_map_save();
        }

        // HISTORY has its own player (Analysis tab); the feed cannot run there.
        let caption = if state.is_playing() { "Pause" } else { "Play" };
        let play = ui.add_enabled_ui(state.life_mode != LifeMode::History, |ui| {
            button(
                ui,
                "btn_Map_PlayPause",
                caption,
                designer_rect(rect, [20, 88], [114, 29]),
            )
        });
        if play.inner.clicked() {
            if state.is_playing() {
                state.pause();
            } else {
                let _ = state.play();
            }
        }
    });
}

fn opt(ui: &mut egui::Ui, state: &mut AppState, filter_height: f32, alarm_height: f32) {
    let page = ui.max_rect().shrink2(vec2(4.0, 0.0));
    let filter_rect = Rect::from_min_size(page.min, vec2(page.width(), filter_height));
    let alarm_rect = Rect::from_min_size(
        pos2(page.min.x, filter_rect.max.y),
        vec2(page.width(), alarm_height),
    );
    let filter_expanded = state.settings.gui.show_filter_box;
    group_box(ui, "gb_Map_Filter", "Planes Filter", filter_rect, |ui| {
        if filter_expanded {
            let panel = Rect::from_min_size(filter_rect.min + vec2(3.0, 16.0), vec2(140.0, 91.0));
            trace::scope(ui, "pa_Planes_Filter", panel, |ui| {
                plane_filter(ui, state, panel)
            });
        }
    });
    if title_clicked(ui, "gb_Map_Filter_title", filter_rect, filter_expanded) {
        state.toggle_box(ControlBox::Filter);
    }
    if state.alarm_text().is_some() {
        ui.painter()
            .rect_filled(alarm_rect, 0.0, super::status::ALARM_PLUM);
    }
    let alarm_expanded = state.settings.gui.show_alarm_box;
    group_box(ui, "gb_Map_Alarms", "Alarms", alarm_rect, |ui| {
        if alarm_expanded {
            checkbox(
                ui,
                "cb_Alarms_Activate",
                "Activate Alarms",
                &mut state.settings.gui.alarm_activate,
                designer_rect(alarm_rect, [14, 19], [118, 19]),
            );
        }
    });
    if title_clicked(ui, "gb_Map_Alarms_title", alarm_rect, alarm_expanded) {
        state.toggle_box(ControlBox::Alarm);
    }
}

fn plane_filter(ui: &mut egui::Ui, state: &mut AppState, panel: Rect) {
    let enabled = !state.is_playing();
    let gui = &mut state.settings.gui;
    let circ = label(
        ui,
        "label13",
        "Max. Circ.:",
        designer_rect(panel, [0, 11], [61, 16]),
    );
    let alt = label(
        ui,
        "label9",
        "Min. Alt.:",
        designer_rect(panel, [0, 37], [61, 16]),
    );
    let cat = label(
        ui,
        "label11",
        "Min. Cat:",
        designer_rect(panel, [0, 64], [61, 16]),
    );
    label(
        ui,
        "label12",
        "km",
        designer_rect(panel, [119, 11], [21, 16]),
    );
    label(
        ui,
        "label10",
        "m",
        designer_rect(panel, [119, 37], [18, 16]),
    );

    ui.add_enabled_ui(enabled, |ui| {
        let rect = designer_rect(panel, [61, 8], [57, 22]);
        trace::record(
            ui,
            "tb_Planes_Filter_Max_Circumcircle",
            Kind::Text,
            "",
            rect,
        );
        ui.put(
            rect,
            DragValue::new(&mut gui.planes_filter_max_circumcircle_km).range(-1.0..=2000.0),
        )
        .labelled_by(circ.id);
        let rect = designer_rect(panel, [61, 34], [57, 22]);
        trace::record(ui, "tb_Planes_Filter_Min_Alt", Kind::Text, "", rect);
        ui.put(
            rect,
            DragValue::new(&mut gui.planes_filter_min_alt_m).range(0..=20000),
        )
        .labelled_by(alt.id);

        let rect = designer_rect(panel, [61, 60], [77, 24]);
        trace::record(ui, "cb_Planes_Filter_Min_Cat", Kind::Text, "", rect);
        scoped(ui, rect, |ui| {
            ComboBox::from_id_salt("cb_Planes_Filter_Min_Cat")
                .width(rect.width())
                .truncate()
                .selected_text(format!("{:?}", gui.planes_filter_min_category))
                .show_ui(ui, |ui| {
                    for c in PlaneCategory::ALL {
                        ui.selectable_value(
                            &mut gui.planes_filter_min_category,
                            c,
                            format!("{c:?}"),
                        );
                    }
                })
                .response
                .labelled_by(cat.id);
        });
    });
}
