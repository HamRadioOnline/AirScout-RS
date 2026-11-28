//! Map tab at its Designer rects: the map provider and refresh (`groupBox2`), the overlay
//! switches (`groupBox23`), the user-supplied overlay (`groupBox58`), the tracking gauges
//! (`groupBox22`), the multi-path marks (`groupBox49`), the plane icon sizes (`groupBox30`) and
//! the plane info window rows (`groupBox7`). The Rust-only preloader estimate and Thunderforest
//! key follow below them (D-026).

use super::fields::{small_label, small_wrapped_label, whole_number};
use crate::map_preloader::PRELOAD_MIN_ZOOM;
use crate::tiles;
use crate::ui::forms::geometry::designer_rect;
use crate::ui::forms::trace::{self, Kind};
use crate::ui::forms::widgets;
use crate::ui::map_track::{known_color, KNOWN_COLORS};
use as_geo::latlon::GRect;
use as_maps::preload_tile_count;
use as_settings::{Secret, Settings, MAP_PRELOADER_MAX_ZOOM};
use eframe::egui::{
    self, vec2, Button, Color32, ComboBox, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind,
    TextEdit, TextStyle, Ui,
};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

/// `ud_Options_Gauges_Width` (`OptionsDlg.Designer.cs:6604-6617`).
const GAUGE_WIDTH_RANGE: std::ops::RangeInclusive<u32> = 175..=1000;

/// What `tb_Options_Map_ToolTipFont` shows: egui draws one font (Decision 9, D-004, D-024).
const TOOLTIP_FONT_TEXT: &str = "egui default";

/// Edit buffer of the API key, with the "show" toggle. Masked unless shown.
pub struct KeyEdit {
    buf: String,
    pub show: bool,
}

impl KeyEdit {
    pub fn new(key: &Secret) -> Self {
        Self {
            buf: key.expose().to_string(),
            show: false,
        }
    }
}

/// What the Map tab asks the dialog to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapAction {
    None,
    /// `btn_Options_UserSupplied_Filename`: open the overlay file picker.
    SelectOverlay,
}

pub fn show(ui: &mut Ui, s: &mut Settings, key: &mut KeyEdit) -> MapAction {
    let body = ui.max_rect();
    map_group(ui, s, body);
    general_group(ui, s, body);
    let action = user_supplied_group(ui, s, body);
    gauges_group(ui, s, body);
    multi_path_group(ui, s, body);
    icon_sizes_group(ui, s, body);
    info_window_group(ui, s, body);
    rust_only_controls(ui, s, key, body);
    action
}

/// `groupBox2` "Map": the provider list, the map opacity, the preloader switch and its zoom,
/// and the screen update interval.
fn map_group(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [13, 10], [395, 167]);
    let list = tiles::providers(&s.gui.thunderforest_api_key);
    // A provider that needs a key falls back to OSM without it (provider_by_name).
    if !list.iter().any(|p| p.name == s.gui.map_provider) {
        s.gui.map_provider = list[0].name.to_string();
    }
    let g = &mut s.gui;
    widgets::group_box(ui, "groupBox2", "Map", group, |ui| {
        // `label61` is the list's only caption, so it names the combo.
        let caption = small_wrapped_label(
            ui,
            "label61",
            "You can select different map provider from the list below. Please note that the selection of maps is taken from Great Maps regardless of their legal status. Some of them might be copyrighted or otherwise restricted in use.  \nYou were asked to agree with the terms of use first.\nOpen Street Map is recommended as a default.",
            designer_rect(group, [32, 14], [342, 64]),
        );
        let rect = designer_rect(group, [11, 81], [363, 21]);
        trace::record(ui, "cb_Options_Map_Provider", Kind::Text, "", rect);
        widgets::scoped(ui, rect, |ui| {
            ComboBox::from_id_salt("cb_Options_Map_Provider")
                .width(rect.width())
                .truncate()
                .selected_text(g.map_provider.clone())
                .show_ui(ui, |ui| {
                    for p in &list {
                        ui.selectable_value(&mut g.map_provider, p.name.to_string(), p.name);
                    }
                })
                .response
                .labelled_by(caption.id);
        });

        let field = designer_rect(group, [124, 109], [54, 20]);
        let label = fitted_label(
            ui,
            "label148",
            "Map Opacity:",
            designer_rect(group, [12, 112], [70, 13]),
            field.left(),
        );
        percentage(ui, "ud_Options_Map_Opacity", &mut g.map_opacity, field).labelled_by(label.id);

        // `cb_Options_Map_Preloader_Enabled` / `ud_Options_Map_Preloader_MaxZoom`
        // (OptionsDlg.Designer.cs:7069-7117, label :2321-2329)
        checkbox(
            ui,
            "cb_Options_Map_Preloader_Enabled",
            "Enable Map Preloading",
            &mut g.map_preloader_enabled,
            designer_rect(group, [230, 111], [136, 17]),
            group,
        );
        let label = small_label(
            ui,
            "label144",
            "down to \nZoom Level:",
            designer_rect(group, [233, 131], [66, 26]),
        );
        let rect = designer_rect(group, [315, 134], [54, 22]);
        let enabled = g.map_preloader_enabled;
        widgets::scoped(ui, rect, |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                whole_number(
                    ui,
                    "ud_Options_Map_Preloader_MaxZoom",
                    &mut g.map_preloader_max_zoom,
                    0..=MAP_PRELOADER_MAX_ZOOM,
                    rect,
                )
                .labelled_by(label.id);
            });
        });

        let field = designer_rect(group, [124, 133], [37, 22]);
        let label = fitted_label(
            ui,
            "label29",
            "Update screen every ",
            designer_rect(group, [9, 137], [109, 13]),
            field.left(),
        );
        whole_number(
            ui,
            "tb_Options_Map_Update_Interval",
            &mut g.map_update_s,
            0..=3600,
            field,
        )
        .labelled_by(label.id);
        fitted_label(
            ui,
            "label97",
            "second(s).",
            designer_rect(group, [167, 137], [56, 13]),
            // label144 starts at x 233.
            group.left() + 233.0,
        );
    });
    ui.advance_cursor_after_rect(group);
}

/// `groupBox30` "Plane Icon Sizes per Category": the icon size per plane category
/// (`GuiSettings::planes_icon_size` is `[Light, Medium, Heavy, Superheavy]`).
fn icon_sizes_group(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [415, 387], [242, 87]);
    let sizes = &mut s.gui.planes_icon_size;
    widgets::group_box(
        ui,
        "groupBox30",
        "Plane Icon Sizes per Category",
        group,
        |ui| {
            for (index, label_name, text, at, size, name, field_at) in [
                (
                    0,
                    "label80",
                    "Light:",
                    [15, 21],
                    [33, 13],
                    "tb_Options_Planes_IconSize_L",
                    [63, 17],
                ),
                (
                    1,
                    "label83",
                    "Medium:",
                    [15, 47],
                    [47, 13],
                    "tb_Options_Planes_IconSize_M",
                    [63, 43],
                ),
                (
                    2,
                    "label82",
                    "Heavy:",
                    [131, 21],
                    [41, 13],
                    "tb_Options_Planes_IconSize_H",
                    [183, 15],
                ),
                (
                    3,
                    "label81",
                    "Super:",
                    [131, 47],
                    [38, 13],
                    "tb_Options_Planes_IconSize_S",
                    [183, 43],
                ),
            ] {
                let field = designer_rect(group, field_at, [37, 22]);
                let label = fitted_label(
                    ui,
                    label_name,
                    text,
                    designer_rect(group, at, size),
                    field.left(),
                );
                whole_number(ui, name, &mut sizes[index], 0..=128, field).labelled_by(label.id);
            }
            fitted_label(
                ui,
                "label84",
                "Restart of AirScout required!",
                designer_rect(group, [41, 66], [159, 15]),
                group.right(),
            );
        },
    );
    ui.advance_cursor_after_rect(group);
}

/// `groupBox7` "Info Window Elements": the units, the tooltip font and the three column boxes
/// of plane info rows, each with its visibility caption at the bottom.
fn info_window_group(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [14, 183], [394, 291]);
    let w = &mut s.gui.info_win;
    widgets::group_box(ui, "groupBox7", "Info Window Elements", group, |ui| {
        fitted_label(
            ui,
            "label72",
            "Units:",
            designer_rect(group, [8, 23], [34, 13]),
            group.left() + 66.0,
        );
        // `RadioButton`s: picking one clears the other; stored flags are shown as they are.
        if widgets::radio(
            ui,
            "rb_Options_InfoWin_Metric",
            "Metric",
            w.metric,
            designer_rect(group, [66, 19], [54, 17]),
        )
        .clicked()
        {
            (w.metric, w.imperial) = (true, false);
        }
        if widgets::radio(
            ui,
            "rb_Options_InfoWin_Imperial",
            "Imperial",
            w.imperial,
            designer_rect(group, [139, 19], [61, 17]),
        )
        .clicked()
        {
            (w.metric, w.imperial) = (false, true);
        }

        // Decision 9: the caption and the read-only field; no font picker (D-024).
        let field = designer_rect(group, [55, 40], [179, 20]);
        let label = fitted_label(
            ui,
            "label62",
            "Font:",
            designer_rect(group, [8, 43], [31, 13]),
            field.left(),
        );
        trace::record(ui, "tb_Options_Map_ToolTipFont", Kind::Text, "", field);
        let mut text = TOOLTIP_FONT_TEXT;
        ui.put(
            field,
            TextEdit::singleline(&mut text)
                .desired_width(field.width())
                .min_size(field.size()),
        )
        .labelled_by(label.id);

        let column = designer_rect(group, [14, 68], [127, 209]);
        widgets::group_box(ui, "groupBox29", "", column, |ui| {
            for (name, text, value, at, size) in [
                (
                    "cb_Options_InfoWin_Position",
                    "Plane Position",
                    &mut w.position,
                    [6, 18],
                    [107, 17],
                ),
                (
                    "cb_Options_InfoWin_Alt",
                    "Plane Altitude",
                    &mut w.alt,
                    [6, 37],
                    [105, 17],
                ),
                (
                    "cb_Options_InfoWin_Track",
                    "Plane Track",
                    &mut w.track,
                    [6, 56],
                    [95, 17],
                ),
                (
                    "cb_Options_InfoWin_Type",
                    "Plane Type",
                    &mut w.type_,
                    [6, 75],
                    [90, 17],
                ),
            ] {
                checkbox(
                    ui,
                    name,
                    text,
                    value,
                    designer_rect(column, at, size),
                    column,
                );
            }
            small_wrapped_label(
                ui,
                "label74",
                "Always visible",
                designer_rect(column, [7, 136], [114, 64]),
            );
        });

        let column = designer_rect(group, [147, 68], [122, 209]);
        widgets::group_box(ui, "groupBox33", "", column, |ui| {
            for (name, text, value, at, size) in [
                (
                    "cb_Options_InfoWin_Dist",
                    "Crossing Distance",
                    &mut w.dist,
                    [11, 18],
                    [111, 17],
                ),
                (
                    "cb_Options_InfoWin_Time",
                    "Crossing Time",
                    &mut w.time,
                    [11, 38],
                    [92, 17],
                ),
                (
                    "cb_Options_InfoWin_Angle",
                    "Crossing Angle",
                    &mut w.angle,
                    [11, 57],
                    [96, 17],
                ),
                (
                    "cb_Options_InfoWin_Epsilon",
                    "Elevation Angles",
                    &mut w.epsilon,
                    [11, 76],
                    [105, 17],
                ),
                (
                    "cb_Options_InfoWin_Squint",
                    "Squint Angle",
                    &mut w.squint,
                    [11, 96],
                    [86, 17],
                ),
                // The Designer puts it at [275, 310] in groupBox7, outside the group's
                // 394 x 291 rect; it is a detailed-mode row (`style.rs`), so it moves into this
                // column below Squint (D-025).
                (
                    "cb_Options_InfoWin_Speed",
                    "Plane Speed",
                    &mut w.speed,
                    [11, 115],
                    [87, 17],
                ),
            ] {
                checkbox(
                    ui,
                    name,
                    text,
                    value,
                    designer_rect(column, at, size),
                    column,
                );
            }
            small_wrapped_label(
                ui,
                "label75",
                "Visible only in detailed window when crossing",
                designer_rect(column, [11, 136], [100, 64]),
            );
        });

        let column = designer_rect(group, [275, 68], [113, 209]);
        widgets::group_box(ui, "groupBox34", "", column, |ui| {
            for (name, text, value, at, size) in [
                (
                    "cb_Options_InfoWin_MyAzimuth",
                    "My Azimuth",
                    &mut w.my_azimuth,
                    [18, 18],
                    [80, 17],
                ),
                (
                    "cb_Options_InfoWin_MyElevation",
                    "My Elevation",
                    &mut w.my_elevation,
                    [18, 38],
                    [87, 17],
                ),
                (
                    "cb_Options_InfoWin_MyDoppler",
                    "My Doppler",
                    &mut w.my_doppler,
                    [18, 57],
                    [80, 17],
                ),
                (
                    "cb_Options_InfoWin_DXAzimuth",
                    "DX Azimuth",
                    &mut w.dx_azimuth,
                    [18, 77],
                    [81, 17],
                ),
                (
                    "cb_Options_InfoWin_DXElevation",
                    "DX Elevation",
                    &mut w.dx_elevation,
                    [18, 96],
                    [88, 17],
                ),
                (
                    "cb_Options_InfoWin_DXDoppler",
                    "DX Doppler",
                    &mut w.dx_doppler,
                    [18, 115],
                    [81, 17],
                ),
            ] {
                checkbox(
                    ui,
                    name,
                    text,
                    value,
                    designer_rect(column, at, size),
                    column,
                );
            }
            small_wrapped_label(
                ui,
                "label76",
                "Visible only in detailed window and when tracking",
                designer_rect(column, [6, 136], [100, 64]),
            );
        });
    });
    ui.advance_cursor_after_rect(group);
}

/// The Rust-only Map controls (D-026), after the Designer groups: the preloader tile estimate
/// and the Thunderforest API key with its "show" toggle.
fn rust_only_controls(ui: &mut Ui, s: &mut Settings, key: &mut KeyEdit, body: Rect) {
    let g = &mut s.gui;
    // O(zoom levels), no list: cheap enough for every frame
    let a = &s.area;
    let area = GRect::new(a.min_lat, a.min_lon, a.max_lat, a.max_lon);
    let n = preload_tile_count(&area, PRELOAD_MIN_ZOOM, g.map_preloader_max_zoom);
    let text = format!(
        "Preloads about {n} tiles from www.airscout.eu (OpenStreetMap only); off by default."
    );
    let rect = designer_rect(body, [14, 480], [643, 17]);
    trace::record(
        ui,
        "lbl_Options_Map_PreloadEstimate",
        Kind::Text,
        &text,
        rect,
    );
    widgets::put_left(ui, rect, egui::Label::new(text).extend());

    let row = designer_rect(body, [14, 502], [394, 22]);
    let label_rect = Rect::from_min_size(row.min, vec2(140.0, row.height()));
    let field = Rect::from_min_size(label_rect.right_top(), vec2(180.0, row.height()));
    let label = fitted_label(
        ui,
        "lbl_Options_Map_ThunderforestKey",
        "Thunderforest API key",
        label_rect,
        field.left(),
    );
    trace::record(ui, "tb_Options_Map_ThunderforestKey", Kind::Text, "", field);
    let edit = TextEdit::singleline(&mut key.buf)
        .password(!key.show)
        .desired_width(field.width())
        .min_size(field.size());
    if ui.put(field, edit).labelled_by(label.id).changed() {
        g.thunderforest_api_key = Secret::from(key.buf.trim());
    }
    let toggle = Rect::from_min_max(field.right_top() + vec2(6.0, 0.0), row.max);
    trace::record(
        ui,
        "cb_Options_Map_ThunderforestKey_Show",
        Kind::Text,
        "show",
        toggle,
    );
    widgets::put_left(ui, toggle, egui::Checkbox::new(&mut key.show, "show"));
    ui.advance_cursor_after_rect(row);
}

/// `groupBox23` "General": the overlay switches. `groupBox39` (an empty placeholder group,
/// Decision 10) is not drawn.
fn general_group(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [415, 10], [241, 78]);
    let g = &mut s.gui;
    widgets::group_box(ui, "groupBox23", "General ", group, |ui| {
        for (name, text, value, at, size) in [
            (
                "cb_Options_Airports_Activate",
                "Airports",
                &mut g.map_show_airports,
                [16, 13],
                [61, 17],
            ),
            (
                "cb_Options_Locators_Activate",
                "Locator Grid",
                &mut g.map_show_locators,
                [118, 13],
                [84, 17],
            ),
            (
                "cb_Options_Distances_Activated",
                "Distance Circles",
                &mut g.map_show_distances,
                [16, 34],
                [102, 17],
            ),
            (
                "cb_Options_Watchlist_Activate",
                "Watchlist Locations",
                &mut g.map_show_watchlist,
                [118, 34],
                [119, 17],
            ),
            (
                "cb_Options_Beamwidths_Activate",
                "Beamwidths",
                &mut g.map_show_beam_width,
                [16, 55],
                [83, 17],
            ),
            (
                "cb_Options_ShowUserSuppliedOverlay",
                "User Supplied",
                &mut g.overlay_enabled,
                [118, 57],
                [92, 17],
            ),
        ] {
            checkbox(ui, name, text, value, designer_rect(group, at, size), group);
        }
    });
    ui.advance_cursor_after_rect(group);
}

/// `groupBox58` "User supplied Overlay": the image file, its opacity and its lat/lon box.
fn user_supplied_group(ui: &mut Ui, s: &mut Settings, body: Rect) -> MapAction {
    let group = designer_rect(body, [415, 91], [242, 124]);
    let g = &mut s.gui;
    let mut action = MapAction::None;
    widgets::group_box(ui, "groupBox58", "User supplied Overlay", group, |ui| {
        let field = designer_rect(group, [68, 24], [114, 20]);
        let label = fitted_label(
            ui,
            "label209",
            "Filename:",
            designer_rect(group, [15, 28], [52, 13]),
            field.left(),
        );
        overlay_filename(ui, &mut g.overlay_filename, field).labelled_by(label.id);
        if widgets::button(
            ui,
            "btn_Options_UserSupplied_Filename",
            "Select",
            designer_rect(group, [188, 23], [47, 23]),
        )
        .clicked()
        {
            action = MapAction::SelectOverlay;
        }

        let field = designer_rect(group, [68, 49], [54, 20]);
        let label = fitted_label(
            ui,
            "label210",
            "Opacity:",
            designer_rect(group, [15, 51], [46, 13]),
            field.left(),
        );
        percentage(
            ui,
            "pc_Options_UserSupplied_Opacity",
            &mut g.overlay_opacity,
            field,
        )
        .labelled_by(label.id);

        for (label_name, text, at, size, name, value, range, field_at) in [
            (
                "label200",
                "Min. Lat:",
                [15, 77],
                [48, 13],
                "tb_Options_UserSupplied_MinLat",
                &mut g.overlay_min_lat,
                -90.0..=90.0,
                [69, 72],
            ),
            (
                "label208",
                "Min. Lon:",
                [131, 76],
                [51, 13],
                "cb_Options_UserSupplied_MinLon",
                &mut g.overlay_min_lon,
                -180.0..=180.0,
                [185, 72],
            ),
            (
                "label34",
                "Max. Lat:",
                [15, 101],
                [51, 13],
                "cb_Options_UserSupplied_MaxLat",
                &mut g.overlay_max_lat,
                -90.0..=90.0,
                [69, 96],
            ),
            (
                "label204",
                "Max. Lon:",
                [131, 101],
                [54, 13],
                "cb_Options_UserSupplied_MaxLon",
                &mut g.overlay_max_lon,
                -180.0..=180.0,
                [185, 97],
            ),
        ] {
            let field = designer_rect(group, field_at, [50, 22]);
            let label = fitted_label(
                ui,
                label_name,
                text,
                designer_rect(group, at, size),
                field.left(),
            );
            degrees(ui, name, value, range, field).labelled_by(label.id);
        }
    });
    ui.advance_cursor_after_rect(group);
    action
}

/// `tb_Options_UserSupplied_Filename`: the path as text. An empty field stores `None`; the
/// stored path is written only when the user edits the field.
fn overlay_filename(ui: &mut Ui, value: &mut Option<PathBuf>, rect: Rect) -> egui::Response {
    trace::record(ui, "tb_Options_UserSupplied_Filename", Kind::Text, "", rect);
    let mut text = value
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let response = ui.put(
        rect,
        TextEdit::singleline(&mut text)
            .desired_width(rect.width())
            .min_size(rect.size()),
    );
    if response.changed() {
        *value = (!text.is_empty()).then(|| PathBuf::from(text));
    }
    response
}

/// A `PercentageControl` (`pc_Options_UserSupplied_Opacity`, `ud_Options_Map_Opacity`): 0..=1 in steps of 0.1, shown as a whole percentage
/// (`PercentageControl.UpdateEditText`). Typed `30%` or `0.3` store 0.3; a stored value outside
/// the range is shown as is.
fn percentage(ui: &mut Ui, name: &str, value: &mut f32, rect: Rect) -> egui::Response {
    trace::record(ui, name, Kind::Text, "", rect);
    ui.put(
        rect,
        egui::DragValue::from_get_set(|edit| {
            if let Some(edit) = edit.filter(|edit| edit.is_finite()) {
                *value = edit.clamp(0.0, 1.0) as f32;
            }
            f64::from(*value)
        })
        .range(0.0..=1.0)
        .speed(0.01)
        .custom_formatter(|v, _| format!("{}%", (v * 100.0).round()))
        .custom_parser(|text| {
            let text = text.trim();
            match text.strip_suffix('%') {
                Some(pct) => pct.trim().parse::<f64>().ok().map(|v| v / 100.0),
                None => text.parse::<f64>().ok(),
            }
        })
        .clamp_existing_to_range(false)
        .min_size(rect.size()),
    )
}

/// A `DoubleTextBox` with `FormatSpecifier = "F2"` and its Designer min/max. An edit is clamped
/// to `range`; a stored value outside it is shown as is.
fn degrees(
    ui: &mut Ui,
    name: &str,
    value: &mut f64,
    range: RangeInclusive<f64>,
    rect: Rect,
) -> egui::Response {
    trace::record(ui, name, Kind::Text, "", rect);
    ui.put(
        rect,
        egui::DragValue::new(value)
            .range(range)
            .speed(0.1)
            .fixed_decimals(2)
            .clamp_existing_to_range(false)
            .min_size(rect.size()),
    )
}

/// The overlay file picker behind `btn_Options_UserSupplied_Filename` (Decision 7): a path field
/// and the image files of the data directory's Tmp folder, with the C# filter
/// `*.jpg;*.jpeg;*.png;*.bmp;*.tiff`.
pub struct OverlayFileDlg {
    tmp: Option<PathBuf>,
    files: Vec<String>,
    path: String,
}

pub enum OverlayFileResult {
    Open,
    Cancel,
    Use(PathBuf),
}

/// `Dlg.Filter = "Image Files|*.jpg;*.jpeg;*.png;*.bmp;*.tiff"` (`OptionsDlg.cs:3363`).
const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "bmp", "tiff"];

impl OverlayFileDlg {
    /// Start at the stored file (`Dlg.FileName`).
    pub fn new(tmp: Option<PathBuf>, current: Option<&Path>) -> Self {
        let files = tmp.as_deref().map(list_images).unwrap_or_default();
        let path = current
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self { tmp, files, path }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> OverlayFileResult {
        let mut result = OverlayFileResult::Open;
        let modal = egui::Modal::new(egui::Id::unique("options_overlay_file")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 32.0).clamp(0.0, 520.0));
            ui.heading("User supplied Overlay");
            let label = ui.label("Image file");
            ui.add(TextEdit::singleline(&mut self.path).desired_width(ui.available_width()))
                .labelled_by(label.id);
            if let Some(tmp) = &self.tmp {
                ui.label("Relative paths are resolved in the data directory's Tmp folder.");
                if ui.button("Refresh list").clicked() {
                    self.files = list_images(tmp);
                }
                ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    for name in &self.files {
                        if ui.selectable_label(self.path == *name, name).clicked() {
                            self.path.clone_from(name);
                        }
                    }
                });
            }
            let entered = Path::new(self.path.trim());
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!entered.as_os_str().is_empty(), Button::new("Use"))
                    .clicked()
                {
                    result = OverlayFileResult::Use(match &self.tmp {
                        Some(tmp) => tmp.join(entered),
                        None => entered.to_path_buf(),
                    });
                }
                if ui.button("Cancel").clicked() {
                    result = OverlayFileResult::Cancel;
                }
            });
        });
        if modal.should_close() {
            return OverlayFileResult::Cancel;
        }
        result
    }
}

/// The image files in `dir`, by name.
fn list_images(dir: &Path) -> Vec<String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let ext = Path::new(&name).extension()?.to_str()?;
            IMAGE_EXTENSIONS
                .iter()
                .any(|e| ext.eq_ignore_ascii_case(e))
                .then_some(name)
        })
        .collect();
    files.sort();
    files
}

/// `groupBox22` "Tracking Gauges": the large tracking display, its gauge width and colour.
fn gauges_group(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [415, 221], [242, 96]);
    let g = &mut s.gui;
    widgets::group_box(ui, "groupBox22", "Tracking Gauges", group, |ui| {
        checkbox(
            ui,
            "cb_Options_Gauges_Show",
            "Show large Tracking Display on map",
            &mut g.tracking_gauges_show,
            designer_rect(group, [16, 19], [199, 17]),
            group,
        );
        let width = designer_rect(group, [174, 41], [63, 22]);
        let label = fitted_label(
            ui,
            "label188",
            "Azimuth/Elevation Gauge Width:",
            designer_rect(group, [7, 44], [162, 13]),
            width.left(),
        );
        gauge_width(ui, &mut g.gauge_width, width).labelled_by(label.id);
        let combo = designer_rect(group, [106, 69], [130, 21]);
        let label = fitted_label(
            ui,
            "label198",
            "Gauge ForeColor:",
            designer_rect(group, [10, 72], [90, 13]),
            combo.left(),
        );
        gauge_color_combo(ui, &mut g.gauge_forecolor, combo).labelled_by(label.id);
    });
    ui.advance_cursor_after_rect(group);
}

/// `groupBox49` "Multi-Path": the mark size and the call labels at the watchlist locations.
fn multi_path_group(ui: &mut Ui, s: &mut Settings, body: Rect) {
    let group = designer_rect(body, [415, 323], [242, 62]);
    widgets::group_box(ui, "groupBox49", "Multi-Path", group, |ui| {
        checkbox(
            ui,
            "cb_Options_Map_SmallMarkers",
            "Use Small Markers for all Path Marks",
            &mut s.web.map_small_markers,
            designer_rect(group, [18, 16], [199, 17]),
            group,
        );
        checkbox(
            ui,
            "cb_Options_Map_LabelCalls",
            "Show Labels with Callsign at Locations",
            &mut s.gui.map_label_calls,
            designer_rect(group, [18, 35], [209, 17]),
            group,
        );
    });
    ui.advance_cursor_after_rect(group);
}

/// `text` in the body font if it is at most `max_width` wide, else in the small font, so it stays
/// clear of the control to its right and inside its group.
fn fitted(ui: &Ui, text: &str, max_width: f32) -> RichText {
    let font = TextStyle::Body.resolve(ui.style());
    let width = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
        .size()
        .x;
    if width <= max_width {
        RichText::new(text)
    } else {
        RichText::new(text).small()
    }
}

/// A check box at its Designer `rect`, left-aligned, its text fitted inside `group`.
fn checkbox(
    ui: &mut Ui,
    name: &str,
    text: &str,
    value: &mut bool,
    rect: Rect,
    group: Rect,
) -> egui::Response {
    trace::record(ui, name, Kind::Text, text, rect);
    let spacing = ui.spacing();
    let icon = spacing.icon_width + spacing.icon_spacing + spacing.button_padding.x * 2.0;
    let text = fitted(ui, text, group.right() - rect.left() - icon - 2.0);
    widgets::put_left(ui, rect, egui::Checkbox::new(value, text))
}

/// A label at its Designer `rect`, fitted to end before `right` (the next control).
fn fitted_label(ui: &mut Ui, name: &str, text: &str, rect: Rect, right: f32) -> egui::Response {
    trace::record(ui, name, Kind::Text, text, rect);
    let text = fitted(ui, text, right - rect.left() - 2.0);
    widgets::put_left(ui, rect, egui::Label::new(text).extend())
}

/// `ud_Options_Gauges_Width`: a whole-number field; an edit is rounded and clamped to
/// [`GAUGE_WIDTH_RANGE`], a stored value outside it is shown as is.
fn gauge_width(ui: &mut Ui, value: &mut u32, rect: Rect) -> egui::Response {
    trace::record(ui, "ud_Options_Gauges_Width", Kind::Text, "", rect);
    let (min, max) = (
        f64::from(*GAUGE_WIDTH_RANGE.start()),
        f64::from(*GAUGE_WIDTH_RANGE.end()),
    );
    ui.put(
        rect,
        egui::DragValue::from_get_set(|edit| {
            if let Some(edit) = edit.filter(|edit| edit.is_finite()) {
                // In 175..=1000 after the clamp, so the cast is exact.
                *value = edit.round().clamp(min, max) as u32;
            }
            f64::from(*value)
        })
        .range(min..=max)
        .speed(1.0)
        .max_decimals(0)
        .custom_parser(|text| {
            let text = text.trim();
            text.parse::<i64>()
                .map(|v| v as f64)
                .ok()
                .or_else(|| text.parse::<f64>().ok().map(f64::round))
        })
        .clamp_existing_to_range(false)
        .min_size(rect.size()),
    )
}

/// `cb_Options_Gauges_ForeColor`: a drop-down list of the known colours with a swatch, shown as
/// `Color [Name]` and stored as the name (`OptionsDlg.cs:1212-1349`). The stored name matches
/// ignoring case (`Color.FromName`); an unknown stored name selects nothing and is kept.
fn gauge_color_combo(ui: &mut Ui, value: &mut String, rect: Rect) -> egui::Response {
    trace::record(ui, "cb_Options_Gauges_ForeColor", Kind::Text, "", rect);
    let current = known_color(value).map(|(n, _)| n);
    let selected = current.map(|n| format!("Color [{n}]")).unwrap_or_default();
    widgets::scoped(ui, rect, |ui| {
        let row = ui.spacing().interact_size.y;
        ComboBox::from_id_salt("cb_Options_Gauges_ForeColor")
            .width(rect.width())
            .truncate()
            .height(row * 10.0)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (name, [r, g, b, a]) in KNOWN_COLORS {
                    ui.horizontal(|ui| {
                        // The swatch selects like its text.
                        let (swatch, swatch_response) = ui.allocate_exact_size(
                            vec2(2.0 * (row - 4.0), row - 4.0),
                            Sense::click(),
                        );
                        ui.painter().rect(
                            swatch,
                            0.0,
                            Color32::from_rgba_unmultiplied(r, g, b, a),
                            Stroke::new(1.0, Color32::BLACK),
                            StrokeKind::Inside,
                        );
                        let text = format!("Color [{name}]");
                        let label = ui.selectable_label(current == Some(name), text);
                        if label.clicked() || swatch_response.clicked() {
                            *value = name.to_string();
                        }
                    });
                }
            })
            .response
    })
}
