//! Drawing only: every widget calls [`crate::app::AppState`] methods for its logic.

pub mod analysis;
pub mod control;
pub mod dialogs;
pub mod forms;
pub mod map;
pub mod map_geom;
pub mod map_track;
pub mod multi;
mod news;
pub mod options;
pub mod planes;
pub mod profile;
pub mod spectrum;
pub mod status;

use crate::app::{AppState, BottomTab, MapTab};
use crate::ui::forms::geometry::{Orientation, SPLITTER_WIDTH};
use crate::ui::forms::trace;
use crate::ui::forms::widgets::{scoped, split_panels, tab_control_with_selection};
use eframe::egui::{self, pos2, vec2, Rect};

/// Height of the Rust-only menu bar (D-002), in points.
pub const MENU_HEIGHT: f32 = 24.0;

/// `sc_Main.Panel2` width when nothing is saved (sc_Main.Panel2 = 152 in the Designer).
const DEFAULT_SIDE_PANEL_WIDTH: f32 = 152.0;
/// The side panel never shrinks below its Designer width.
const MIN_SIDE_PANEL_WIDTH: f32 = 152.0;
/// `sc_Map.Panel2` height when nothing is saved (sc_Map.Panel2 = 360 in the Designer).
const DEFAULT_BOTTOM_PANEL_HEIGHT: f32 = 360.0;

/// `tc_Map` pages: the map and the news page.
const MAP_PAGES: [(&str, &str); 2] = [("tp_Map", "Map"), ("tp_News", "Latest News")];

/// `tc_Main` pages, in Designer order; `tp_Planes` is the Rust-only extra (D-006).
const MAIN_PAGES: [(&str, &str); 4] = [
    ("tp_Elevation", "Path Profile"),
    ("tp_Spectrum", "Spectrum"),
    ("tp_Analysis", "Analysis"),
    ("tp_Planes", "Planes"),
];

/// Independent map widgets for the main and Spectrum tabs.
pub struct Maps {
    pub main: map::MapView,
    pub nearest: map::MapView,
}

impl Maps {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            main: map::MapView::new(data_dir),
            nearest: map::MapView::new(data_dir),
        }
    }

    pub fn without_tiles() -> Self {
        Self {
            main: map::MapView::without_tiles(),
            nearest: map::MapView::without_tiles(),
        }
    }
}

/// The whole window: menu bar, status strip, right control panel, central map area.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, maps: &mut Maps) {
    // The harness (and eframe) give the root `ui` an inner margin, so the frame
    // is derived from the margin-free viewport rect.
    let vp = ui.ctx().viewport_rect();

    // Rust-only menu bar (D-002): File → "Download data for the area…".
    menu_bar(
        ui,
        state,
        Rect::from_min_size(vp.min, vec2(vp.width(), MENU_HEIGHT)),
    );

    // The client area below the menu bar maps 1:1 onto the upstream client size.
    let client = Rect::from_min_max(pos2(vp.min.x, vp.min.y + MENU_HEIGHT), vp.max);
    trace::record_window(ui, client);

    // ss_Main: the status strip, docked Bottom.
    let status_rect = Rect::from_min_max(
        pos2(client.min.x, client.max.y - status::STATUS_HEIGHT),
        client.max,
    );
    status::show(ui, state, status_rect);

    // sc_Main fills the rest of the client.
    let sc_main = Rect::from_min_max(client.min, pos2(client.max.x, status_rect.min.y));

    let side = state
        .settings
        .gui
        .side_panel_width
        .unwrap_or(DEFAULT_SIDE_PANEL_WIDTH)
        .max(MIN_SIDE_PANEL_WIDTH);
    // The persisted width is the right panel (`sc_Main.Panel2`); the splitter
    // distance is the left panel (`sc_Main.Panel1`, 852 in the Designer).
    let mut split = (sc_main.width() - side - SPLITTER_WIDTH).max(0.0);
    let dragged = split_panels(
        ui,
        "sc_Main",
        Orientation::Vertical,
        &mut split,
        sc_main,
        |ui, idx| {
            if idx == 0 {
                map_area(ui, state, maps);
            } else {
                control::show(ui, state);
            }
        },
    );
    if dragged {
        state.settings.gui.side_panel_width =
            Some((sc_main.width() - split - SPLITTER_WIDTH).max(MIN_SIDE_PANEL_WIDTH));
    }

    dialogs::show(ui.ctx(), state);
    news::prompt(ui.ctx(), state);
}

/// The Rust-only menu bar (D-002): File → "Download data for the area…".
fn menu_bar(ui: &mut egui::Ui, state: &mut AppState, rect: Rect) {
    scoped(ui, rect, |ui| {
        ui.horizontal(|ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Download data for the area…").clicked() {
                    state.open_download_area();
                    ui.close();
                }
            });
        });
    });
}

/// `sc_Main.Panel1`: the map area (`sc_Map` split into the map tab control and the
/// bottom tab control, which is skipped when the lower panel is collapsed).
fn map_area(ui: &mut egui::Ui, state: &mut AppState, maps: &mut Maps) {
    let rect = ui.max_rect();
    if state.lower_panel_collapsed() {
        // MULTI or News: the map (or news) tab control fills the whole area.
        trace::scope(ui, "sc_Map", rect, |ui| {
            trace::scope(ui, "sc_Map.Panel1", rect, |ui| {
                map_tabs(ui, state, &mut maps.main)
            });
        });
        return;
    }
    let bottom = state
        .settings
        .gui
        .bottom_panel_height
        .unwrap_or(DEFAULT_BOTTOM_PANEL_HEIGHT);
    // The persisted height is the lower panel (`sc_Map.Panel2`); the splitter
    // distance is the map panel (`sc_Map.Panel1`, 342 in the Designer).
    let mut split = (rect.height() - bottom - SPLITTER_WIDTH).max(0.0);
    let dragged = split_panels(
        ui,
        "sc_Map",
        Orientation::Horizontal,
        &mut split,
        rect,
        |ui, idx| {
            if idx == 0 {
                map_tabs(ui, state, &mut maps.main);
            } else {
                bottom_tabs(ui, state, &mut maps.nearest);
            }
        },
    );
    if dragged {
        state.settings.gui.bottom_panel_height =
            Some((rect.height() - split - SPLITTER_WIDTH).max(0.0));
    }
}

/// `sc_Map.Panel1`: the `tc_Map` tab control (Map / Latest News).
fn map_tabs(ui: &mut egui::Ui, state: &mut AppState, map: &mut map::MapView) {
    let rect = ui.max_rect();
    let current = state.map_tab as usize;
    let mut selected = current;
    tab_control_with_selection(
        ui,
        "tc_Map",
        rect,
        &MAP_PAGES,
        &mut selected,
        !state.is_playing(),
        |ui, idx| {
            if idx == 0 {
                map.show(ui, state);
            } else {
                news::show(ui, &state.settings.gui.news_url);
            }
        },
    );
    if selected != current {
        state.set_map_tab(if selected == 0 {
            MapTab::Map
        } else {
            MapTab::News
        });
    }
}

/// `sc_Map.Panel2`: the `tc_Main` tab control (path profile, spectrum, analysis, planes).
fn bottom_tabs(ui: &mut egui::Ui, state: &mut AppState, nearest: &mut map::MapView) {
    let rect = ui.max_rect();
    let current = state.bottom_tab as usize;
    let mut selected = current;
    tab_control_with_selection(
        ui,
        "tc_Main",
        rect,
        &MAIN_PAGES,
        &mut selected,
        !state.is_playing(),
        |ui, idx| match idx {
            0 => profile::show(ui, state),
            1 => spectrum::show(ui, state, nearest),
            2 => analysis::show(ui, state),
            _ => planes::show(ui, state),
        },
    );
    if selected != current {
        state.set_bottom_tab(match selected {
            0 => BottomTab::Profile,
            1 => BottomTab::Spectrum,
            2 => BottomTab::Analysis,
            _ => BottomTab::Planes,
        });
    }
}
