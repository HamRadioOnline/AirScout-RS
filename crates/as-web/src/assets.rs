//! The embedded web page (D2): a fixed table of the files that `wwwroot/index.html`
//! and `wwwroot/dist/index.html` use, at their `wwwroot` paths, and the licence texts
//! under `LICENSES/`.
//!
//! C# reads any file below its web directory (`Webserver.cs:1121-1151`). Here only
//! a path in this table is ever served, so no request can reach another file.

use std::fmt;

/// One embedded file.
#[derive(Clone, Copy)]
pub struct Asset {
    /// The path below `wwwroot`, with `/` separators and no leading `/`.
    pub path: &'static str,
    /// The `Content-Type`, by file extension.
    pub mime: &'static str,
    pub bytes: &'static [u8],
}

impl fmt::Debug for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Asset")
            .field("path", &self.path)
            .field("mime", &self.mime)
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// Paths are unique in the table, so the path names the asset.
impl PartialEq for Asset {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for Asset {}

/// The MIME type by extension, as `MimeTypeMap.GetMimeType` gives it for the
/// extensions of the table.
const fn mime(path: &str) -> &'static str {
    const TYPES: [(&str, &str); 7] = [
        (".html", "text/html"),
        (".js", "application/javascript"),
        (".css", "text/css"),
        (".png", "image/png"),
        (".svg", "image/svg+xml"),
        (".ico", "image/x-icon"),
        (".txt", "text/plain"),
    ];
    let mut i = 0;
    while i < TYPES.len() {
        if ends_with(path.as_bytes(), TYPES[i].0.as_bytes()) {
            return TYPES[i].1;
        }
        i += 1;
    }
    panic!("embedded asset with an unknown extension");
}

const fn ends_with(s: &[u8], suffix: &[u8]) -> bool {
    if suffix.len() > s.len() {
        return false;
    }
    let off = s.len() - suffix.len();
    let mut i = 0;
    while i < suffix.len() {
        if s[off + i] != suffix[i] {
            return false;
        }
        i += 1;
    }
    true
}

macro_rules! asset {
    ($path:literal) => {
        Asset {
            path: $path,
            mime: mime($path),
            bytes: include_bytes!(concat!("../assets/", $path)),
        }
    };
}

/// Every embedded file, sorted by path. `tests/assets.rs` checks that this table
/// matches the `assets` directory.
pub static ASSETS: &[Asset] = &[
    asset!("LICENSES/CC-BY-4.0.txt"),
    asset!("LICENSES/GPL-3.0.txt"),
    asset!("LICENSES/THIRD-PARTY.txt"),
    asset!("LICENSES/chart.js.txt"),
    asset!("LICENSES/coreui-chartjs.txt"),
    asset!("LICENSES/coreui-free-bootstrap-admin-template.txt"),
    asset!("LICENSES/coreui-icons.txt"),
    asset!("LICENSES/coreui-utils.txt"),
    asset!("LICENSES/coreui.txt"),
    asset!("LICENSES/jquery.txt"),
    asset!("LICENSES/leaflet-rotatedmarker.txt"),
    asset!("LICENSES/leaflet-tooltip-layout.txt"),
    asset!("LICENSES/leaflet.txt"),
    asset!("LICENSES/reconnecting-websocket.txt"),
    asset!("LICENSES/simplebar.txt"),
    asset!("LICENSES/tablesorter.txt"),
    asset!("dist/assets/brand/coreui.svg"),
    asset!("dist/assets/favicon/favicon.ico"),
    asset!("dist/assets/favicon/ms-icon-144x144.png"),
    asset!("dist/assets/icons/user.svg"),
    asset!("dist/css/examples.css"),
    asset!("dist/css/style.css"),
    asset!("dist/css/vendors/simplebar.css"),
    asset!("dist/index.html"),
    asset!("dist/vendors/@coreui/chartjs/css/coreui-chartjs.css"),
    asset!("dist/vendors/@coreui/chartjs/js/coreui-chartjs.js"),
    asset!("dist/vendors/@coreui/coreui/js/coreui.bundle.min.js"),
    asset!("dist/vendors/@coreui/icons/svg/free.svg"),
    asset!("dist/vendors/@coreui/utils/js/coreui-utils.js"),
    asset!("dist/vendors/chart.js/js/chart.min.js"),
    asset!("dist/vendors/jquery/js/jquery.min.js"),
    asset!("dist/vendors/leaflet/css/images/layers-2x.png"),
    asset!("dist/vendors/leaflet/css/images/layers.png"),
    asset!("dist/vendors/leaflet/css/images/marker-icon-2x.png"),
    asset!("dist/vendors/leaflet/css/images/marker-icon.png"),
    asset!("dist/vendors/leaflet/css/images/marker-shadow.png"),
    asset!("dist/vendors/leaflet/css/leaflet.css"),
    asset!("dist/vendors/leaflet/js/leaflet-src.js"),
    asset!("dist/vendors/leaflet/js/leaflet-tooltip-layout.js"),
    asset!("dist/vendors/leaflet/js/leaflet.rotatedMarker.js"),
    asset!("dist/vendors/reconnecting-websocket/js/reconnecting-websocket.min.js"),
    asset!("dist/vendors/simplebar/css/simplebar.css"),
    asset!("dist/vendors/simplebar/js/simplebar.min.js"),
    asset!("dist/vendors/tablesorter/css/theme.bootstrap_4.min.css"),
    asset!("dist/vendors/tablesorter/js/jquery.tablesorter.min.js"),
    asset!("dist/vendors/tablesorter/js/jquery.tablesorter.widgets.min.js"),
    asset!("dist/vendors/tablesorter/js/parsers/parser-input-select.min.js"),
    asset!("index.html"),
    asset!("resources/css/airscout.css"),
    asset!("resources/icons/aircrafts/darkorange.png"),
    asset!("resources/icons/aircrafts/gray.png"),
    asset!("resources/icons/aircrafts/magenta.png"),
    asset!("resources/icons/aircrafts/red.png"),
    asset!("resources/icons/markers/airport.png"),
    asset!("resources/icons/markers/arrow.png"),
    asset!("resources/icons/markers/arrowshadow.png"),
    asset!("resources/icons/markers/black_small.png"),
    asset!("resources/icons/markers/blue-dot.png"),
    asset!("resources/icons/markers/blue-pushpin.png"),
    asset!("resources/icons/markers/blue.png"),
    asset!("resources/icons/markers/blue_small.png"),
    asset!("resources/icons/markers/brown_small.png"),
    asset!("resources/icons/markers/drag_cross_67_16.png"),
    asset!("resources/icons/markers/go-big-green.png"),
    asset!("resources/icons/markers/gray_small.png"),
    asset!("resources/icons/markers/green-dot.png"),
    asset!("resources/icons/markers/green-dot1.png"),
    asset!("resources/icons/markers/green.png"),
    asset!("resources/icons/markers/green_small.png"),
    asset!("resources/icons/markers/grn-pushpin.png"),
    asset!("resources/icons/markers/lightblue.png"),
    asset!("resources/icons/markers/ltblu-pushpin.png"),
    asset!("resources/icons/markers/ltblue-dot.png"),
    asset!("resources/icons/markers/orange-dot.png"),
    asset!("resources/icons/markers/orange.png"),
    asset!("resources/icons/markers/orange_small.png"),
    asset!("resources/icons/markers/pause-big-red.png"),
    asset!("resources/icons/markers/pink-dot.png"),
    asset!("resources/icons/markers/pink-pushpin.png"),
    asset!("resources/icons/markers/pink.png"),
    asset!("resources/icons/markers/purple-dot.png"),
    asset!("resources/icons/markers/purple-pushpin.png"),
    asset!("resources/icons/markers/purple.png"),
    asset!("resources/icons/markers/purple_small.png"),
    asset!("resources/icons/markers/pushpin_shadow.png"),
    asset!("resources/icons/markers/red-dot.png"),
    asset!("resources/icons/markers/red-pushpin.png"),
    asset!("resources/icons/markers/red.png"),
    asset!("resources/icons/markers/red_small.png"),
    asset!("resources/icons/markers/shadow-dot.png"),
    asset!("resources/icons/markers/shadow_small.png"),
    asset!("resources/icons/markers/stop-big-red.png"),
    asset!("resources/icons/markers/white_small.png"),
    asset!("resources/icons/markers/yellow-dot.png"),
    asset!("resources/icons/markers/yellow.png"),
    asset!("resources/icons/markers/yellow_small.png"),
    asset!("resources/icons/markers/ylw-pushpin.png"),
    asset!("resources/js/airscout.aircrafts.js"),
    asset!("resources/js/airscout.core.js"),
    asset!("resources/js/airscout.feeds.js"),
    asset!("resources/js/airscout.ui.js"),
    asset!("resources/js/airscout.wtkst.js"),
    asset!("resources/js/scoutbase.core.js"),
    asset!("resources/js/scoutbase.elevation.js"),
    asset!("resources/js/scoutbase.propagation.js"),
    asset!("resources/js/scoutbase.stations.js"),
];

/// The asset at `path` (exact and case-sensitive), if any.
pub fn asset(path: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|a| a.path == path)
}
