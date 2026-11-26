//! Tracking readout on the map (`gm_Main_Paint`, `MapDlg.cs:5602-5662`): an Az/El box and, with a
//! Doppler strategy, the rig frequencies. The C# paints dial gauges; the port prints the values.

use crate::app::AppState;
use crate::tracking::TrackValues;
use as_settings::DopplerStrategy;
use eframe::egui::{Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

/// The values to show: gauges on, playing, TRACK and values present (`:5604`).
pub fn shown(state: &AppState) -> Option<&TrackValues> {
    (state.settings.gui.tracking_gauges_show && state.is_playing())
        .then(|| state.tooltip_track())
        .flatten()
}

/// Paint the Az/El box at `corner` and the rig box below it.
pub fn draw(painter: &Painter, corner: Pos2, state: &AppState) {
    let Some(v) = shown(state) else { return };
    let mut lines = vec![
        format!("Az {:6.2}°", v.my_azimuth),
        format!("El {:6.2}°", v.my_elevation),
    ];
    let y = box_at(painter, corner, &lines);
    if state.settings.doppler.strategy == DopplerStrategy::None {
        return;
    }
    lines = vec![
        format!("MyDop: {:.0}", v.my_doppler),
        format!("DXDop: {:.0}", v.dx_doppler),
        format!("Dial : {}", state.settings.doppler.dial_freq_hz),
        format!("RX   : {}", v.rx_hz),
        format!("TX   : {}", v.tx_hz),
    ];
    box_at(painter, corner + Vec2::new(0.0, y + 6.0), &lines);
}

/// Paint `lines` in a box with its top-left at `at`; returns the height of the box.
fn box_at(painter: &Painter, at: Pos2, lines: &[String]) -> f32 {
    let galley = painter.layout_no_wrap(lines.join("\n"), FontId::monospace(14.0), Color32::BLACK);
    let r = Rect::from_min_size(at, galley.size() + Vec2::splat(10.0));
    painter.rect(
        r,
        3.0,
        Color32::from_rgba_unmultiplied(255, 255, 255, 220),
        Stroke::new(2.0, Color32::BLACK),
        StrokeKind::Inside,
    );
    painter.galley(r.min + Vec2::splat(5.0), galley, Color32::BLACK);
    r.height()
}
