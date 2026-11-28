//! GUI-free plane colours, icon sizes and tooltip text (`MapDlg.cs` `CreatePlaneSimple` /
//! `CreatePlaneDetailed`), including the tracking lines (MyAzimuth etc., shown while
//! `TrackMode` is TRACK, `MapDlg.cs:4913-4940`).

use crate::tracking::TrackValues;
use as_aircraft::crossing::minutes_to_crossing;
use as_aircraft::units::{km_mi, m_ft};
use as_aircraft::{PlaneCategory, PlaneInfo};
use as_geo::latlon::GPoint;
use as_settings::InfoWin;
use chrono::{DateTime, Duration, Utc};

/// Plane colour class by crossing potential.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneColor {
    Magenta,
    Red,
    DarkOrange,
    Gray,
}

/// 100 magenta, 75 red, 50 dark orange, anything else gray.
pub fn plane_color(potential: i32) -> PlaneColor {
    match potential {
        100 => PlaneColor::Magenta,
        75 => PlaneColor::Red,
        50 => PlaneColor::DarkOrange,
        _ => PlaneColor::Gray,
    }
}

/// RGB of a colour class.
pub fn rgb(c: PlaneColor) -> [u8; 3] {
    match c {
        PlaneColor::Magenta => [255, 0, 255],
        PlaneColor::Red => [255, 0, 0],
        PlaneColor::DarkOrange => [255, 140, 0],
        PlaneColor::Gray => [128, 128, 128],
    }
}

/// Icon size in px; `sizes` is `[Light, Medium, Heavy, Superheavy]` (`GuiSettings::planes_icon_size`).
/// Category `None` uses the medium icon, as in the C#.
pub fn plane_icon_px(cat: PlaneCategory, sizes: [u32; 4]) -> u32 {
    match cat {
        PlaneCategory::Light => sizes[0],
        PlaneCategory::Heavy => sizes[2],
        PlaneCategory::Superheavy => sizes[3],
        PlaneCategory::Medium | PlaneCategory::None => sizes[1],
    }
}

/// .NET `ToString("F0")`: round half away from zero.
fn f0(x: f64) -> String {
    format!("{:.0}", x.round())
}

/// .NET `ToString("00.00")` / `("000.00")`: zero-padded integer part, `-` prefix if negative.
fn pad2(x: f64, int_digits: usize) -> String {
    let s = format!("{:.2}", x.abs());
    let width = int_digits + 3;
    format!("{}{:0>width$}", if x < 0.0 { "-" } else { "" }, s)
}

/// .NET `ToString("+#;-#;0")`.
fn signed(x: f64) -> String {
    let r = x.round();
    if r == 0.0 {
        "0".into()
    } else if r > 0.0 {
        format!("+{r:.0}")
    } else {
        format!("-{:.0}", -r)
    }
}

fn truncate20(s: &str) -> String {
    s.chars().take(20).collect()
}

/// Tooltip lines of a plane. As in `DrawPlanes` (MapDlg.cs:4980-5006), potential 100/75/50 gets
/// the `CreatePlaneDetailed` lines, every other potential the `CreatePlaneSimple` lines
/// (no altitude difference, no speed, no crossing lines). `now` is the map's `CurrentTime`, used for
/// the predicted crossing time. `_my` is unused by the C# but kept in the interface. `track` is
/// `Some` only while `TrackMode` is TRACK and values exist; detailed tooltips of every plane
/// then carry them (not only the tracked one, as in the C#).
pub fn tooltip_lines(
    p: &PlaneInfo,
    info: &InfoWin,
    _my: GPoint,
    now: DateTime<Utc>,
    track: Option<&TrackValues>,
) -> Vec<String> {
    let mut l = vec![p.call.clone(), "--------------------".to_string()];
    if info.position {
        let ns = if p.lat >= 0.0 { 'N' } else { 'S' };
        let ew = if p.lon >= 0.0 { 'E' } else { 'W' };
        l.push(format!(
            "Pos: {}°{ns} , {}°{ew}",
            pad2(p.lat.abs(), 2),
            pad2(p.lon.abs(), 3)
        ));
    }
    let detailed = matches!(p.potential, 100 | 75 | 50);
    if info.alt {
        let estimate = if p.altitude_estimate() {
            " [estimated]"
        } else {
            ""
        };
        if !detailed {
            if info.metric {
                l.push(format!("Alt: {}m{estimate}", p.alt_m() as i64));
            } else {
                l.push(format!("Alt: {}ft{estimate}", p.alt_ft as i64));
            }
        } else if info.metric {
            l.push(format!(
                "Alt: {}m{estimate} [{}m]",
                p.alt_m() as i64,
                signed(p.alt_diff)
            ));
        } else {
            l.push(format!(
                "Alt: {}ft{estimate} [{}ft]",
                p.alt_ft as i64,
                signed(m_ft(p.alt_diff))
            ));
        }
    }
    if info.track {
        l.push(format!("Track: {}°", p.track as i64));
    }
    if detailed && info.speed {
        if info.metric {
            l.push(format!("Speed: {}km/h", f0(p.speed_kmh())));
        } else {
            l.push(format!("Speed: {}kts", f0(p.speed_kts)));
        }
    }
    if info.type_ {
        l.push(format!(
            "Type: {} {} [{}]",
            truncate20(&p.manufacturer),
            truncate20(&p.model),
            p.category.short()
        ));
    }
    if detailed {
        if info.dist {
            if info.metric {
                l.push(format!("Dist: {}km", f0(p.int_qrb)));
            } else {
                l.push(format!("Dist: {}mi", f0(km_mi(p.int_qrb))));
            }
        }
        if info.time {
            if let Some(mins) = minutes_to_crossing(p) {
                let t = now + Duration::minutes(i64::from(mins));
                l.push(format!("Time: {} [ {mins}min]", t.format("%H:%M")));
            } else {
                l.push("Time: n/a".to_string());
            }
        }
        if info.angle {
            l.push(format!("Angle: {}°", f0(p.angle.to_degrees())));
        }
        if info.epsilon {
            l.push(format!(
                "Eps: {}° <> {}°",
                pad2(p.eps1.to_degrees(), 2),
                pad2(p.eps2.to_degrees(), 2)
            ));
        }
        if info.squint {
            l.push(format!("Squint: {}°", pad2(p.squint.to_degrees(), 2)));
        }
        if let Some(t) = track {
            let my = [
                (info.my_azimuth, "MyAzimuth", pad2(t.my_azimuth, 2), "°"),
                (
                    info.my_elevation,
                    "MyElevation",
                    pad2(t.my_elevation, 2),
                    "°",
                ),
                (info.my_doppler, "MyDoppler", f0(t.my_doppler), "Hz"),
            ];
            let dx = [
                (info.dx_azimuth, "DXAzimuth", pad2(t.dx_azimuth, 2), "°"),
                (
                    info.dx_elevation,
                    "DXElevation",
                    pad2(t.dx_elevation, 2),
                    "°",
                ),
                (info.dx_doppler, "DXDoppler", f0(t.dx_doppler), "Hz"),
            ];
            for rows in [my, dx] {
                if rows.iter().any(|r| r.0) {
                    l.push("--------------------".to_string());
                    for (on, name, v, unit) in rows {
                        if on {
                            l.push(format!("{name}: {v}{unit}"));
                        }
                    }
                }
            }
        }
    }
    l
}
