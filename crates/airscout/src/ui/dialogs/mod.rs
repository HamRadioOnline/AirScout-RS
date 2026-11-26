//! Modal dialogs, drawn as egui windows over the main window.

pub mod cleanup;
pub mod crossing_history;
pub mod db_maintenance;
pub mod delete_station;
pub mod disclaimer;
pub mod feed_settings;
pub mod history_url;
pub mod horizon;
pub mod map_station;
pub mod obstructions;
pub mod plane_history;
pub mod set_time;
pub mod traffic;
pub mod watchlist;
pub mod wizard;

use crate::app::AppState;
use crate::ui::options::OptionsOutcome;
use eframe::egui;

/// Show the open dialogs.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    if let Some(mut dlg) = state.watchlist_dlg.take() {
        match watchlist::show(ctx, &mut dlg) {
            watchlist::DlgResult::Open => state.watchlist_dlg = Some(dlg),
            watchlist::DlgResult::Ok => state.apply_watchlist(dlg.selected()),
            watchlist::DlgResult::Cancel => {}
        }
    }
    if let Some(mut dlg) = state.options_dlg.take() {
        state.feed_options_dlg(&mut dlg);
        match dlg.show(ctx) {
            None => {
                dlg.refresh_qrvs(&mut |call, loc| state.qrvs_of(call, loc));
                let request = dlg.take_request().map(|r| (r, dlg.draft.clone()));
                let maint = dlg.take_maint_request();
                let views = dlg.take_view_requests();
                state.options_dlg = Some(dlg);
                for r in views {
                    state.on_view_request(r);
                }
                if let Some((action, draft)) = request {
                    state.open_station_dialog(action, &draft);
                }
                if let Some(a) = maint {
                    state.on_maint_action(a);
                }
            }
            Some(OptionsOutcome::Ok(settings)) => {
                state.cancel_view_jobs();
                let qrvs = dlg.pending_qrvs();
                state.apply_options(settings);
                state.write_qrvs(&qrvs);
                state.options_closed();
            }
            Some(OptionsOutcome::Cancel) => {
                state.cancel_view_jobs();
                state.options_closed();
            }
        }
    }
    db_maintenance::show(ctx, state);
    if let Some(mut dlg) = state.obstructions_dlg.take() {
        match dlg.show(ctx) {
            watchlist::DlgResult::Open => state.obstructions_dlg = Some(dlg),
            watchlist::DlgResult::Ok => state.store_obstruction(dlg.model, &dlg.result()),
            watchlist::DlgResult::Cancel => {}
        }
    }
    if let Some(mut dlg) = state.set_time_dlg.take() {
        match dlg.show(ctx) {
            set_time::SetTimeResult::Open => state.set_time_dlg = Some(dlg),
            set_time::SetTimeResult::Ok(online, t) => state.apply_time(online, t),
            set_time::SetTimeResult::Cancel => {}
        }
    }
    if let Some(mut dlg) = state.horizon_dlg.take() {
        let action = dlg.show(ctx);
        // the plot mode is a setting: it survives the dialog and an OK of Options
        let polar = dlg.polar;
        state.settings.gui.horizon_plot_polar = polar;
        if let Some(o) = &mut state.options_dlg {
            o.draft.gui.horizon_plot_polar = polar;
        }
        state.horizon_dlg = Some(dlg);
        match action {
            horizon::HorizonAction::None => {}
            horizon::HorizonAction::Calculate => state.horizon_start(),
            horizon::HorizonAction::Export => state.horizon_export(),
            horizon::HorizonAction::Close => state.horizon_close(),
        }
    }
}
