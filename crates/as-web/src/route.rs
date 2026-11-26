//! The C# route order (`Webserver.cs:1121-1222`): a static file first, then the
//! content routes in their `if` order.

use crate::assets::{asset, Asset};
use crate::query::unescape_data_string;

/// A content (JSON) route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiRoute {
    AircraftList,
    Planes,
    Version,
    Settings,
    Bands,
    BandValues,
    AircraftCategories,
    Location,
    Qrv,
    ElevationPath,
    PropagationPath,
    NearestPlanes,
    Airports,
    PlaneFeedSettings,
}

/// What a request target is answered with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Asset(&'static Asset),
    Api(ApiRoute),
    NotFound,
}

/// The routes matched by `RawUrl.ToLower().StartsWith(..)`, in the C# order
/// (`Webserver.cs:1164-1222`).
const PREFIX_ROUTES: [(&str, ApiRoute); 12] = [
    ("/version.json", ApiRoute::Version),
    ("/settings.json", ApiRoute::Settings),
    ("/bands.json", ApiRoute::Bands),
    ("/bandvalues.json", ApiRoute::BandValues),
    ("/aircraftcategories.json", ApiRoute::AircraftCategories),
    ("/location.json", ApiRoute::Location),
    ("/qrv.json", ApiRoute::Qrv),
    ("/elevationpath.json", ApiRoute::ElevationPath),
    ("/propagationpath.json", ApiRoute::PropagationPath),
    ("/nearestplanes.json", ApiRoute::NearestPlanes),
    ("/airports.json", ApiRoute::Airports),
    ("/planefeedsettings.json", ApiRoute::PlaneFeedSettings),
];

/// The static file name of a raw request target (`Webserver.cs:1121-1145`): unescaped,
/// `/index.html` for a target without `/` or equal to `/`, the first character
/// dropped and the query cut off.
fn file_name(raw: &str) -> String {
    let mut p = unescape_data_string(raw);
    if !p.contains('/') || p == "/" {
        p = "/index.html".to_string();
    }
    // `p` contains "/", so it has a first character.
    let rest: &str = p.char_indices().nth(1).map_or("", |(i, _)| &p[i..]);
    rest.split('?').next().unwrap_or("").to_string()
}

/// Routes a raw request target as C# does. A static file is an exact, case-sensitive
/// hit in the asset table: the table replaces `File.Exists` below the web directory,
/// so `..`, `\`, drive letters and NUL never reach a file (security deviation).
pub fn route(raw: &str) -> Route {
    if let Some(a) = asset(&file_name(raw)) {
        return Route::Asset(a);
    }
    if raw == "/AircraftList.json" {
        return Route::Api(ApiRoute::AircraftList);
    }
    let lower = raw.to_lowercase();
    if lower == "/planes.json" {
        return Route::Api(ApiRoute::Planes);
    }
    PREFIX_ROUTES
        .iter()
        .find(|(prefix, _)| lower.starts_with(prefix))
        .map_or(Route::NotFound, |(_, api)| Route::Api(*api))
}
