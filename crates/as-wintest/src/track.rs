//! Track messages sent to the rotator program (`MapDlg.cs:7864-7895`).

use crate::dotnet::{clamp_elevation, fixed, wrap_azimuth, zero3};
use crate::{WtKind, WtMessage};

fn msg(kind: WtKind, server: &str, dst: &str, data: String, extra: String) -> WtMessage {
    WtMessage {
        kind,
        src: server.into(),
        dst: dst.into(),
        data,
        extra,
    }
}

pub fn set_azimuth(server: &str, az: f64) -> WtMessage {
    msg(
        WtKind::SetAzimuth,
        server,
        "",
        "AUTO".into(),
        format!(" 00 {}", zero3(wrap_azimuth(az))),
    )
}

pub fn set_elevation(server: &str, el: f64) -> WtMessage {
    msg(
        WtKind::SetElevation,
        server,
        "",
        "AUTO".into(),
        format!(" 00 {}", zero3(clamp_elevation(el))),
    )
}

pub fn set_azel(server: &str, az: f64, el: f64) -> WtMessage {
    msg(
        WtKind::SetAzEl,
        server,
        "",
        "AUTO".into(),
        format!(" 00 {} {}", fixed(az, 8), fixed(el, 8)),
    )
}

pub fn as_nearest(server: &str, dst: &str, data: String) -> WtMessage {
    msg(WtKind::AsNearest, server, dst, data, String::new())
}
