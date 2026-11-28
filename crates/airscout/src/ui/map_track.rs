//! Tracking readout on the map (`gm_Main_Paint`, `MapDlg.cs:5602-5662`): an Az/El frame and, with
//! a Doppler strategy, the rig frequencies above it, both at the bottom right of the map
//! (`gm_Main_SizeChanged`, `:5664-5694`). The C# paints dial gauges inside the frame; the port
//! prints the values there. Width and colour follow `Map_TrackingGaugeWidth` and
//! `Map_TrackingGaugeColor`.

use crate::app::AppState;
use crate::tracking::TrackValues;
use as_settings::DopplerStrategy;
use eframe::egui::{
    pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2,
};

/// The values to show: gauges on, playing, TRACK and values present (`:5604`).
pub fn shown(state: &AppState) -> Option<&TrackValues> {
    (state.settings.gui.tracking_gauges_show && state.is_playing())
        .then(|| state.tooltip_track())
        .flatten()
}

/// Height of each gauge (`ag_Azimuth`/`ag_Elevation`, 175×175 in `MapDlg.Designer.cs:312,333`).
const GAUGE_HEIGHT: f32 = 175.0;
/// Narrowest gauge the C# accepts (`gm_Main_SizeChanged`, `:5673`).
const GAUGE_MIN_WIDTH: f32 = 175.0;

/// The Az/El frame and the rig panel above it for a map at `map` (`gm_Main_SizeChanged`): each
/// gauge is `width` wide, at least 175 and at most half the map width.
pub fn gauge_rects(map: Rect, width: u32) -> (Rect, Rect) {
    let mut w = width as f32;
    if w < GAUGE_MIN_WIDTH {
        w = GAUGE_MIN_WIDTH;
    }
    if w > map.width() / 2.0 {
        w = map.width() / 2.0;
    }
    let left = map.right() - 2.0 * w - 40.0;
    let top = map.bottom() - GAUGE_HEIGHT - 20.0;
    let frame = Rect::from_min_size(pos2(left, top), vec2(2.0 * w + 20.0, GAUGE_HEIGHT));
    let rig_height = (GAUGE_HEIGHT / 2.0).floor();
    let rig = Rect::from_min_size(
        pos2(left, map.bottom() - GAUGE_HEIGHT - rig_height - 30.0),
        vec2(2.0 * w + 20.0, rig_height),
    );
    (frame, rig)
}

/// Paint the Az/El frame and the rig panel on the map at `map`.
pub fn draw(painter: &Painter, map: Rect, state: &AppState) {
    let Some(v) = shown(state) else { return };
    let gui = &state.settings.gui;
    let color = gauge_color(&gui.gauge_forecolor);
    let (frame, rig) = gauge_rects(map, gui.gauge_width);
    let pen = Stroke::new(3.0, color);
    let fill = Color32::from_rgba_unmultiplied(255, 255, 255, 200);
    painter.rect(frame, 0.0, fill, pen, StrokeKind::Inside);
    let half = (frame.width() - 20.0) / 2.0;
    for (i, text) in [
        format!("Az\n{:6.2}°", v.my_azimuth),
        format!("El\n{:6.2}°", v.my_elevation),
    ]
    .into_iter()
    .enumerate()
    {
        let x = frame.left() + i as f32 * (half + 20.0) + half / 2.0;
        painter.text(
            pos2(x, frame.center().y),
            Align2::CENTER_CENTER,
            text,
            FontId::monospace(18.0),
            color,
        );
    }
    if state.settings.doppler.strategy == DopplerStrategy::None {
        return;
    }
    painter.rect(rig, 0.0, fill, pen, StrokeKind::Inside);
    // `Courier New`, `pa_Rig.Height / 7`, at the C# offsets (`:5645-5660`).
    let font = FontId::monospace((rig.height() / 7.0).floor().max(8.0));
    let at = |dx: f32, row: f32| rig.min + Vec2::new(10.0 + dx, rig.height() / 4.0 * row);
    let line = |p: Pos2, text: String| {
        painter.text(p, Align2::LEFT_TOP, text, font.clone(), color);
    };
    line(at(0.0, 0.0), format!("MyDop: {:.0}", v.my_doppler));
    line(
        at(rig.width() / 2.0, 0.0),
        format!("DXDop: {:.0}", v.dx_doppler),
    );
    line(
        at(0.0, 1.0),
        format!("Dial : {}", state.settings.doppler.dial_freq_hz),
    );
    line(at(0.0, 2.0), format!("RX   : {}", v.rx_hz));
    line(at(0.0, 3.0), format!("TX   : {}", v.tx_hz));
}

/// The colour of a `KnownColor` name; an unknown name paints black (the C# falls back to
/// `Color.Black`, `MapDlg.cs:5611-5616`).
pub fn gauge_color(name: &str) -> Color32 {
    known_color(name).map_or(Color32::BLACK, |(_, [r, g, b, a])| {
        Color32::from_rgba_unmultiplied(r, g, b, a)
    })
}

/// The `KnownColor` entry of `name`, ignoring case like `Color.FromName`.
pub fn known_color(name: &str) -> Option<(&'static str, [u8; 4])> {
    KNOWN_COLORS
        .iter()
        .copied()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
}

/// `System.Drawing.KnownColor` in enum order (.NET Framework, 174 values) with its ARGB as
/// `[r, g, b, a]`. The 33 system colours use the Windows 10 defaults; the others are the fixed
/// named colours.
pub const KNOWN_COLORS: [(&str, [u8; 4]); 174] = [
    ("ActiveBorder", [180, 180, 180, 255]),
    ("ActiveCaption", [153, 180, 209, 255]),
    ("ActiveCaptionText", [0, 0, 0, 255]),
    ("AppWorkspace", [171, 171, 171, 255]),
    ("Control", [240, 240, 240, 255]),
    ("ControlDark", [160, 160, 160, 255]),
    ("ControlDarkDark", [105, 105, 105, 255]),
    ("ControlLight", [227, 227, 227, 255]),
    ("ControlLightLight", [255, 255, 255, 255]),
    ("ControlText", [0, 0, 0, 255]),
    ("Desktop", [0, 0, 0, 255]),
    ("GrayText", [109, 109, 109, 255]),
    ("Highlight", [0, 120, 215, 255]),
    ("HighlightText", [255, 255, 255, 255]),
    ("HotTrack", [0, 102, 204, 255]),
    ("InactiveBorder", [244, 247, 252, 255]),
    ("InactiveCaption", [191, 205, 219, 255]),
    ("InactiveCaptionText", [0, 0, 0, 255]),
    ("Info", [255, 255, 225, 255]),
    ("InfoText", [0, 0, 0, 255]),
    ("Menu", [240, 240, 240, 255]),
    ("MenuText", [0, 0, 0, 255]),
    ("ScrollBar", [200, 200, 200, 255]),
    ("Window", [255, 255, 255, 255]),
    ("WindowFrame", [100, 100, 100, 255]),
    ("WindowText", [0, 0, 0, 255]),
    ("Transparent", [255, 255, 255, 0]),
    ("AliceBlue", [240, 248, 255, 255]),
    ("AntiqueWhite", [250, 235, 215, 255]),
    ("Aqua", [0, 255, 255, 255]),
    ("Aquamarine", [127, 255, 212, 255]),
    ("Azure", [240, 255, 255, 255]),
    ("Beige", [245, 245, 220, 255]),
    ("Bisque", [255, 228, 196, 255]),
    ("Black", [0, 0, 0, 255]),
    ("BlanchedAlmond", [255, 235, 205, 255]),
    ("Blue", [0, 0, 255, 255]),
    ("BlueViolet", [138, 43, 226, 255]),
    ("Brown", [165, 42, 42, 255]),
    ("BurlyWood", [222, 184, 135, 255]),
    ("CadetBlue", [95, 158, 160, 255]),
    ("Chartreuse", [127, 255, 0, 255]),
    ("Chocolate", [210, 105, 30, 255]),
    ("Coral", [255, 127, 80, 255]),
    ("CornflowerBlue", [100, 149, 237, 255]),
    ("Cornsilk", [255, 248, 220, 255]),
    ("Crimson", [220, 20, 60, 255]),
    ("Cyan", [0, 255, 255, 255]),
    ("DarkBlue", [0, 0, 139, 255]),
    ("DarkCyan", [0, 139, 139, 255]),
    ("DarkGoldenrod", [184, 134, 11, 255]),
    ("DarkGray", [169, 169, 169, 255]),
    ("DarkGreen", [0, 100, 0, 255]),
    ("DarkKhaki", [189, 183, 107, 255]),
    ("DarkMagenta", [139, 0, 139, 255]),
    ("DarkOliveGreen", [85, 107, 47, 255]),
    ("DarkOrange", [255, 140, 0, 255]),
    ("DarkOrchid", [153, 50, 204, 255]),
    ("DarkRed", [139, 0, 0, 255]),
    ("DarkSalmon", [233, 150, 122, 255]),
    ("DarkSeaGreen", [143, 188, 139, 255]),
    ("DarkSlateBlue", [72, 61, 139, 255]),
    ("DarkSlateGray", [47, 79, 79, 255]),
    ("DarkTurquoise", [0, 206, 209, 255]),
    ("DarkViolet", [148, 0, 211, 255]),
    ("DeepPink", [255, 20, 147, 255]),
    ("DeepSkyBlue", [0, 191, 255, 255]),
    ("DimGray", [105, 105, 105, 255]),
    ("DodgerBlue", [30, 144, 255, 255]),
    ("Firebrick", [178, 34, 34, 255]),
    ("FloralWhite", [255, 250, 240, 255]),
    ("ForestGreen", [34, 139, 34, 255]),
    ("Fuchsia", [255, 0, 255, 255]),
    ("Gainsboro", [220, 220, 220, 255]),
    ("GhostWhite", [248, 248, 255, 255]),
    ("Gold", [255, 215, 0, 255]),
    ("Goldenrod", [218, 165, 32, 255]),
    ("Gray", [128, 128, 128, 255]),
    ("Green", [0, 128, 0, 255]),
    ("GreenYellow", [173, 255, 47, 255]),
    ("Honeydew", [240, 255, 240, 255]),
    ("HotPink", [255, 105, 180, 255]),
    ("IndianRed", [205, 92, 92, 255]),
    ("Indigo", [75, 0, 130, 255]),
    ("Ivory", [255, 255, 240, 255]),
    ("Khaki", [240, 230, 140, 255]),
    ("Lavender", [230, 230, 250, 255]),
    ("LavenderBlush", [255, 240, 245, 255]),
    ("LawnGreen", [124, 252, 0, 255]),
    ("LemonChiffon", [255, 250, 205, 255]),
    ("LightBlue", [173, 216, 230, 255]),
    ("LightCoral", [240, 128, 128, 255]),
    ("LightCyan", [224, 255, 255, 255]),
    ("LightGoldenrodYellow", [250, 250, 210, 255]),
    ("LightGray", [211, 211, 211, 255]),
    ("LightGreen", [144, 238, 144, 255]),
    ("LightPink", [255, 182, 193, 255]),
    ("LightSalmon", [255, 160, 122, 255]),
    ("LightSeaGreen", [32, 178, 170, 255]),
    ("LightSkyBlue", [135, 206, 250, 255]),
    ("LightSlateGray", [119, 136, 153, 255]),
    ("LightSteelBlue", [176, 196, 222, 255]),
    ("LightYellow", [255, 255, 224, 255]),
    ("Lime", [0, 255, 0, 255]),
    ("LimeGreen", [50, 205, 50, 255]),
    ("Linen", [250, 240, 230, 255]),
    ("Magenta", [255, 0, 255, 255]),
    ("Maroon", [128, 0, 0, 255]),
    ("MediumAquamarine", [102, 205, 170, 255]),
    ("MediumBlue", [0, 0, 205, 255]),
    ("MediumOrchid", [186, 85, 211, 255]),
    ("MediumPurple", [147, 112, 219, 255]),
    ("MediumSeaGreen", [60, 179, 113, 255]),
    ("MediumSlateBlue", [123, 104, 238, 255]),
    ("MediumSpringGreen", [0, 250, 154, 255]),
    ("MediumTurquoise", [72, 209, 204, 255]),
    ("MediumVioletRed", [199, 21, 133, 255]),
    ("MidnightBlue", [25, 25, 112, 255]),
    ("MintCream", [245, 255, 250, 255]),
    ("MistyRose", [255, 228, 225, 255]),
    ("Moccasin", [255, 228, 181, 255]),
    ("NavajoWhite", [255, 222, 173, 255]),
    ("Navy", [0, 0, 128, 255]),
    ("OldLace", [253, 245, 230, 255]),
    ("Olive", [128, 128, 0, 255]),
    ("OliveDrab", [107, 142, 35, 255]),
    ("Orange", [255, 165, 0, 255]),
    ("OrangeRed", [255, 69, 0, 255]),
    ("Orchid", [218, 112, 214, 255]),
    ("PaleGoldenrod", [238, 232, 170, 255]),
    ("PaleGreen", [152, 251, 152, 255]),
    ("PaleTurquoise", [175, 238, 238, 255]),
    ("PaleVioletRed", [219, 112, 147, 255]),
    ("PapayaWhip", [255, 239, 213, 255]),
    ("PeachPuff", [255, 218, 185, 255]),
    ("Peru", [205, 133, 63, 255]),
    ("Pink", [255, 192, 203, 255]),
    ("Plum", [221, 160, 221, 255]),
    ("PowderBlue", [176, 224, 230, 255]),
    ("Purple", [128, 0, 128, 255]),
    ("Red", [255, 0, 0, 255]),
    ("RosyBrown", [188, 143, 143, 255]),
    ("RoyalBlue", [65, 105, 225, 255]),
    ("SaddleBrown", [139, 69, 19, 255]),
    ("Salmon", [250, 128, 114, 255]),
    ("SandyBrown", [244, 164, 96, 255]),
    ("SeaGreen", [46, 139, 87, 255]),
    ("SeaShell", [255, 245, 238, 255]),
    ("Sienna", [160, 82, 45, 255]),
    ("Silver", [192, 192, 192, 255]),
    ("SkyBlue", [135, 206, 235, 255]),
    ("SlateBlue", [106, 90, 205, 255]),
    ("SlateGray", [112, 128, 144, 255]),
    ("Snow", [255, 250, 250, 255]),
    ("SpringGreen", [0, 255, 127, 255]),
    ("SteelBlue", [70, 130, 180, 255]),
    ("Tan", [210, 180, 140, 255]),
    ("Teal", [0, 128, 128, 255]),
    ("Thistle", [216, 191, 216, 255]),
    ("Tomato", [255, 99, 71, 255]),
    ("Turquoise", [64, 224, 208, 255]),
    ("Violet", [238, 130, 238, 255]),
    ("Wheat", [245, 222, 179, 255]),
    ("White", [255, 255, 255, 255]),
    ("WhiteSmoke", [245, 245, 245, 255]),
    ("Yellow", [255, 255, 0, 255]),
    ("YellowGreen", [154, 205, 50, 255]),
    ("ButtonFace", [240, 240, 240, 255]),
    ("ButtonHighlight", [255, 255, 255, 255]),
    ("ButtonShadow", [160, 160, 160, 255]),
    ("GradientActiveCaption", [185, 209, 234, 255]),
    ("GradientInactiveCaption", [215, 228, 242, 255]),
    ("MenuBar", [240, 240, 240, 255]),
    ("MenuHighlight", [51, 153, 255, 255]),
];
