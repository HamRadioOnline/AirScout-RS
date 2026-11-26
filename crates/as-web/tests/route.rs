//! The C# route order (`Webserver.cs:1121-1151` static files, `:1153-1222` content).
use as_web::route::{route, ApiRoute, Route};

fn asset_path(url: &str) -> Option<&'static str> {
    match route(url) {
        Route::Asset(a) => Some(a.path),
        _ => None,
    }
}

#[test]
fn routes_in_csharp_order() {
    // A URL without "/" and "/" itself are the start page.
    assert_eq!(asset_path("/"), Some("index.html"));
    assert_eq!(asset_path("*"), Some("index.html"));
    assert_eq!(asset_path(""), Some("index.html"));
    assert_eq!(asset_path("/dist/index.html"), Some("dist/index.html"));
    // The query is cut off the file name, and the path is unescaped first.
    assert_eq!(asset_path("/dist/index.html?x=1"), Some("dist/index.html"));
    assert_eq!(asset_path("/dist%2Findex.html"), Some("dist/index.html"));
    assert_eq!(
        asset_path("/resources/js/airscout.core.js"),
        Some("resources/js/airscout.core.js")
    );
    assert_eq!(
        asset_path("/LICENSES/THIRD-PARTY.txt"),
        Some("LICENSES/THIRD-PARTY.txt")
    );

    assert_eq!(route("/settings.json?x"), Route::Api(ApiRoute::Settings));
    assert_eq!(route("/SETTINGS.JSON"), Route::Api(ApiRoute::Settings));
    // A prefix match, 1:1.
    assert_eq!(route("/settings.jsonfoo"), Route::Api(ApiRoute::Settings));

    // Exact and case-sensitive.
    assert_eq!(
        route("/AircraftList.json"),
        Route::Api(ApiRoute::AircraftList)
    );
    assert_eq!(route("/aircraftlist.json"), Route::NotFound);
    assert_eq!(route("/AircraftList.json?x=1"), Route::NotFound);
    // Exact, case-insensitive.
    assert_eq!(route("/PLANES.json"), Route::Api(ApiRoute::Planes));
    assert_eq!(route("/planes.json?x=1"), Route::NotFound);

    let prefixes = [
        ("/version.json", ApiRoute::Version),
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
    for (path, api) in prefixes {
        assert_eq!(route(path), Route::Api(api), "{path}");
        assert_eq!(
            route(&format!("{path}?call=TE1ST")),
            Route::Api(api),
            "{path}"
        );
        assert_eq!(route(&path.to_uppercase()), Route::Api(api), "{path}");
    }

    // The table is exact and case-sensitive; the server proxies no tiles.
    assert_eq!(route("/dist/INDEX.HTML"), Route::NotFound);
    assert_eq!(route("/tiles/1/2/3.png"), Route::NotFound);
    assert_eq!(route("/dist/"), Route::NotFound);
    assert_eq!(route("/nothing"), Route::NotFound);
}

#[test]
fn traversal_never_escapes() {
    for url in [
        "/../Cargo.toml",
        "/%2e%2e/%2e%2e/etc/passwd",
        "//etc/passwd",
        "/C:/Windows/win.ini",
        "/dist/..%2f..%2fCargo.toml",
        "/..\\..\\Cargo.toml",
        "/%00",
        "/resources/../index.html",
        "/dist/../index.html",
        "/./index.html",
        "/dist\\index.html",
        "/%2e%2e%2fCargo.toml",
    ] {
        assert_eq!(route(url), Route::NotFound, "{url}");
    }
}
